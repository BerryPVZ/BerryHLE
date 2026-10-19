/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Thread-specific data keys.
//!
//! The values live in host-side maps, but each thread also gets a guest
//! thread-specific data (TSD) array that mirrors them, indexed by key. The
//! CPU's TPIDRURO register points at the current thread's array, like on a
//! real device: code built against iPhone OS's libSystem may inline
//! `pthread_getspecific` as a TPIDRURO read plus a load from the array,
//! bypassing the function entirely.

use crate::abi::GuestFunction;
use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::{ConstVoidPtr, MutPtr, MutVoidPtr, Ptr};
use crate::{Environment, ThreadId};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    /// The `pthread_key_t` value, with 1 subtracted, is the index into this
    /// vector. The tuple contains the map of thread-specific data pointers plus
    /// the destructor pointer.
    keys: Vec<(HashMap<ThreadId, MutVoidPtr>, GuestFunction)>,
    /// Each thread's guest TSD array, see the module documentation.
    tsd_arrays: HashMap<ThreadId, MutPtr<MutVoidPtr>>,
}

/// Number of slots in a guest TSD array. Matches libpthread's
/// `_INTERNAL_POSIX_THREAD_KEYS_END`. Keys beyond this still work through
/// the functions, they just aren't mirrored.
const TSD_SLOTS: u32 = 768;

/// Get the address of a thread's guest TSD array (allocating it if needed),
/// which is the value of the TPIDRURO register while that thread is running.
pub fn tsd_array_for_thread(env: &mut Environment, thread: ThreadId) -> MutPtr<MutVoidPtr> {
    if let Some(&array) = get_state(env).tsd_arrays.get(&thread) {
        return array;
    }
    let array: MutPtr<MutVoidPtr> = env.mem.alloc(TSD_SLOTS * 4).cast();
    let state = &env.libc_state.pthread.key;
    for (idx, (data, _)) in state.keys.iter().enumerate() {
        if let Some(&value) = data.get(&thread) {
            let key = idx as u32 + 1;
            if key < TSD_SLOTS {
                env.mem.write(array + key, value);
            }
        }
    }
    get_state(env).tsd_arrays.insert(thread, array);
    array
}

/// Write a value to a key's slot in a thread's guest TSD array, if it has one.
fn mirror_to_tsd_array(
    env: &mut Environment,
    thread: ThreadId,
    key: pthread_key_t,
    value: MutVoidPtr,
) {
    if key >= TSD_SLOTS {
        return;
    }
    if let Some(&array) = get_state(env).tsd_arrays.get(&thread) {
        env.mem.write(array + key, value);
    }
}

fn get_state(env: &mut Environment) -> &mut State {
    &mut env.libc_state.pthread.key
}

type pthread_key_t = u32;

fn pthread_key_create(
    env: &mut Environment,
    key_ptr: MutPtr<pthread_key_t>,
    destructor: GuestFunction, // void (*destructor)(void *), may be NULL
) -> i32 {
    let idx = get_state(env).keys.len();
    let key: pthread_key_t = (idx + 1).try_into().unwrap();
    get_state(env).keys.push((HashMap::new(), destructor));
    env.mem.write(key_ptr, key);
    0 // success
}

fn pthread_getspecific(env: &mut Environment, key: pthread_key_t) -> MutVoidPtr {
    // Per Apple's pthread_getspecific(3): "The effect of calling
    // pthread_getspecific() with a key value that was not obtained from
    // pthread_key_create(), or after a key has been deleted with
    // pthread_key_delete(), is undefined." The documented return contract is
    // that, when no thread-specific value is associated with the key, NULL is
    // returned. Real iOS implementations never abort the process for a bad
    // key, so neither do we: a corrupt/garbage key (which guest
    // use-after-free or uninitialised reads can produce) simply yields NULL
    // instead of panicking and taking the whole emulator down.
    let Some(idx) = key.checked_sub(1).and_then(|i| usize::try_from(i).ok()) else {
        return Ptr::null();
    };
    let current_thread = env.current_thread;
    let state = get_state(env);
    let Some((data, _)) = state.keys.get(idx) else {
        return Ptr::null();
    };
    data.get(&current_thread).copied().unwrap_or(Ptr::null())
}

fn pthread_setspecific(env: &mut Environment, key: pthread_key_t, value: ConstVoidPtr) -> i32 {
    // Per Apple's pthread_setspecific(3): it "will fail if: [EINVAL] The key
    // value is invalid." A key not obtained from pthread_key_create() is
    // undefined behaviour on real iOS, but it must not crash the host
    // process. Return EINVAL for an out-of-range/garbage key rather than
    // panicking on an out-of-bounds index.
    let Some(idx) = key.checked_sub(1).and_then(|i| usize::try_from(i).ok()) else {
        return 22; // EINVAL
    };
    let current_thread = env.current_thread;
    let state = get_state(env);
    let Some((data, _)) = state.keys.get_mut(idx) else {
        return 22; // EINVAL
    };
    data.insert(current_thread, value.cast_mut());
    mirror_to_tsd_array(env, current_thread, key, value.cast_mut());
    0 // success
}

fn pthread_key_delete(env: &mut Environment, key: pthread_key_t) -> i32 {
    // POSIX `int pthread_key_delete(pthread_key_t key)`: returns 0 on
    // success, EINVAL if `key` is not a valid key. We don't currently
    // free the slot in the keys table — Mono only deletes keys at
    // teardown so the leaked slot is bounded — but we do clear out
    // any per-thread values stored under it so the next
    // `pthread_getspecific` after a fresh `pthread_key_create` reuse
    // doesn't see a stale pointer.
    let Some(idx) = key.checked_sub(1).and_then(|i| usize::try_from(i).ok()) else {
        return 22; // EINVAL
    };
    let state = get_state(env);
    if let Some((data, _)) = state.keys.get_mut(idx) {
        let threads: Vec<ThreadId> = data.drain().map(|(thread, _)| thread).collect();
        for thread in threads {
            mirror_to_tsd_array(env, thread, key, Ptr::null());
        }
        log_dbg!("pthread_key_delete({}) cleared per-thread values", key);
        0
    } else {
        log_dbg!(
            "pthread_key_delete({}) on unknown key, returning EINVAL",
            key
        );
        22
    }
}

// --- ОБНОВЛЕННЫЙ СПИСОК ЭКСПОРТА ---
pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(pthread_key_create(_, _)),
    export_c_func!(pthread_getspecific(_)),
    export_c_func!(pthread_setspecific(_, _)),
    export_c_func!(pthread_key_delete(_)), // Регистрируем новую функцию
];
