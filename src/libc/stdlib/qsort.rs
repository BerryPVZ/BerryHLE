/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! stdlib's qsort

use crate::abi::{CallFromHost, GuestFunction};
use crate::dyld::{export_c_func, FunctionExports};
use crate::mem::MutPtr;
use crate::Environment;

fn qsort(
    env: &mut Environment,
    base: MutPtr<u8>,
    nitems: u32,
    size: u32,
    compar: GuestFunction, // int (*compar)(const void *, const void*))
) {
    qsort_generic(
        env,
        nitems,
        &mut |env, i, j| {
            let i_ptr = base + i * size;
            let j_ptr = base + j * size;
            compar.call_from_host(env, (i_ptr.cast_const(), j_ptr.cast_const()))
        },
        &mut |env: &mut Environment, mut i: u32, mut j: u32| {
            if i == j {
                return;
            }
            if i > j {
                std::mem::swap(&mut i, &mut j);
            }
            let base_slice = env.mem.bytes_at_mut(base, nitems * size);
            let (_, slice_after_first) = base_slice.split_at_mut((i * size) as usize);
            let offset = (j * size - i * size) as usize;
            let (left, right) = slice_after_first.split_at_mut(offset);
            left[..size as usize].swap_with_slice(&mut right[..size as usize]);
        },
    );
}

/// Sorts `nitems` items that are only reachable through `compar` and `swap`
/// callbacks taking item indices.
///
/// This is a stable in-place merge sort: insertion sort on small blocks, then
/// SymMerge (Kim & Kutzner) to merge them, which is what Go's `sort.Stable`
/// does. It makes O(n log n) comparisons and O(n log² n) swaps whatever the
/// input order, and only recurses O(log n) deep. The quicksort this replaced
/// always picked the first item as its pivot, so already-sorted input took
/// n²/2 comparisons, each a call into the guest. Gravity Guy sorts its ~12,000
/// level sprites when loading a level, which never finished.
///
/// Being stable also matches NSArray's sorting methods on Apple platforms.
pub fn qsort_generic<T, Compare, Swap>(
    user_data: &mut T,
    nitems: u32,
    compar: &mut Compare, // int (*compar)(const void *, const void*))
    swap: &mut Swap,
) where
    Compare: FnMut(&mut T, u32, u32) -> i32,
    Swap: FnMut(&mut T, u32, u32),
{
    const BLOCK_SIZE: u32 = 20;

    let n = nitems;
    if n < 2 {
        return;
    }
    let mut sorter = Sorter {
        user_data,
        compar,
        swap,
    };

    let mut a = 0;
    while n - a > BLOCK_SIZE {
        sorter.insertion_sort(a, a + BLOCK_SIZE);
        a += BLOCK_SIZE;
    }
    sorter.insertion_sort(a, n);

    let mut block_size = BLOCK_SIZE;
    while block_size < n {
        let mut a = 0;
        while n - a >= 2 * block_size {
            sorter.sym_merge(a, a + block_size, a + 2 * block_size);
            a += 2 * block_size;
        }
        if n - a > block_size {
            sorter.sym_merge(a, a + block_size, n);
        }
        block_size *= 2;
    }
}

struct Sorter<'a, T, Compare, Swap> {
    user_data: &'a mut T,
    compar: &'a mut Compare,
    swap: &'a mut Swap,
}

impl<T, Compare, Swap> Sorter<'_, T, Compare, Swap>
where
    Compare: FnMut(&mut T, u32, u32) -> i32,
    Swap: FnMut(&mut T, u32, u32),
{
    fn less(&mut self, i: u32, j: u32) -> bool {
        (self.compar)(self.user_data, i, j) < 0
    }
    fn swap(&mut self, i: u32, j: u32) {
        (self.swap)(self.user_data, i, j)
    }

    /// Sorts the range `a..b`.
    fn insertion_sort(&mut self, a: u32, b: u32) {
        for i in (a + 1)..b {
            let mut j = i;
            while j > a && self.less(j, j - 1) {
                self.swap(j, j - 1);
                j -= 1;
            }
        }
    }

    /// Merges the sorted ranges `a..m` and `m..b`.
    fn sym_merge(&mut self, a: u32, m: u32, b: u32) {
        // A single item is inserted with a binary search, then rotated into
        // place.
        if m - a == 1 {
            let (mut i, mut j) = (m, b);
            while i < j {
                let h = i + (j - i) / 2;
                if self.less(h, a) {
                    i = h + 1;
                } else {
                    j = h;
                }
            }
            for k in a..(i - 1) {
                self.swap(k, k + 1);
            }
            return;
        }
        if b - m == 1 {
            let (mut i, mut j) = (a, m);
            while i < j {
                let h = i + (j - i) / 2;
                if !self.less(m, h) {
                    i = h + 1;
                } else {
                    j = h;
                }
            }
            for k in ((i + 1)..=m).rev() {
                self.swap(k, k - 1);
            }
            return;
        }

        let mid = a + (b - a) / 2;
        let n = mid + m;
        let (mut start, mut r) = if m > mid { (n - b, mid) } else { (a, m) };
        let p = n - 1;
        while start < r {
            let c = start + (r - start) / 2;
            if !self.less(p - c, c) {
                start = c + 1;
            } else {
                r = c;
            }
        }
        let end = n - start;
        if start < m && m < end {
            self.rotate(start, m, end);
        }
        if a < start && start < mid {
            self.sym_merge(a, start, mid);
        }
        if mid < end && end < b {
            self.sym_merge(mid, end, b);
        }
    }

    /// Swaps the ranges `a..a+n` and `b..b+n`.
    fn swap_range(&mut self, a: u32, b: u32, n: u32) {
        for i in 0..n {
            self.swap(a + i, b + i);
        }
    }

    /// Rotates `a..b` so that `m..b` comes before `a..m`.
    fn rotate(&mut self, a: u32, m: u32, b: u32) {
        let mut i = m - a;
        let mut j = b - m;
        while i != j {
            if i > j {
                self.swap_range(m - i, m, j);
                i -= j;
            } else {
                self.swap_range(m - i, m + j - i, i);
                j -= i;
            }
        }
        self.swap_range(m - i, m, i);
    }
}

#[cfg(test)]
mod tests {
    use super::qsort_generic;

    /// Sorts `items` by their first field, counting comparisons.
    fn sort(items: &mut Vec<(i32, u32)>) -> u32 {
        let mut compares = 0;
        let len = items.len() as u32;
        let mut data = (items, &mut compares);
        qsort_generic(
            &mut data,
            len,
            &mut |(items, compares), i, j| {
                **compares += 1;
                items[i as usize].0 - items[j as usize].0
            },
            &mut |(items, _), i, j| items.swap(i as usize, j as usize),
        );
        compares
    }

    #[test]
    fn sorts_stably() {
        let mut seed = 12345u32;
        for len in [0, 1, 2, 3, 19, 20, 21, 39, 40, 41, 100, 1000, 4097] {
            let mut items: Vec<(i32, u32)> = (0..len)
                .map(|i| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    (((seed >> 16) % 50) as i32, i)
                })
                .collect();
            let mut expected = items.clone();
            expected.sort_by_key(|&(key, _)| key);
            sort(&mut items);
            assert_eq!(items, expected, "len {}", len);
        }
    }

    #[test]
    fn sorts_with_comparator_that_never_returns_zero() {
        // Like Gravity Guy's XtriggerComparator: -1 if a <= b, else 1.
        let mut seed = 777u32;
        for len in [2, 5, 20, 21, 64, 1000, 3000] {
            let mut items: Vec<i32> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    ((seed >> 16) % 30) as i32
                })
                .collect();
            let mut data = &mut items;
            qsort_generic(
                &mut data,
                len,
                &mut |items, i, j| {
                    if items[i as usize] <= items[j as usize] {
                        -1
                    } else {
                        1
                    }
                },
                &mut |items, i, j| items.swap(i as usize, j as usize),
            );
            assert!(items.windows(2).all(|w| w[0] <= w[1]), "len {}", len);
        }
    }

    #[test]
    fn sorted_and_reversed_input_is_not_quadratic() {
        let len = 12756;
        let mut sorted: Vec<(i32, u32)> = (0..len).map(|i| (i as i32, i)).collect();
        let compares = sort(&mut sorted);
        assert!(compares < 400_000, "{} comparisons", compares);
        assert!(sorted.windows(2).all(|w| w[0].0 <= w[1].0));

        let mut reversed: Vec<(i32, u32)> = (0..len).map(|i| (-(i as i32), i)).collect();
        let compares = sort(&mut reversed);
        assert!(compares < 400_000, "{} comparisons", compares);
        assert!(reversed.windows(2).all(|w| w[0].0 <= w[1].0));

        let mut equal: Vec<(i32, u32)> = (0..len).map(|i| (7, i)).collect();
        sort(&mut equal);
        assert!(equal.iter().enumerate().all(|(i, &(_, j))| i as u32 == j));
    }
}

pub const FUNCTIONS: FunctionExports = &[export_c_func!(qsort(_, _, _, _))];
