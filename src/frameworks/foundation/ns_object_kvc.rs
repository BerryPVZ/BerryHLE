use crate::Environment;
use crate::frameworks::core_animation::ca_transform3d::CATransform3D;
use crate::frameworks::core_graphics::cg_affine_transform::CGAffineTransform;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSUInteger;
use crate::mem::{SafeRead, guest_size_of};
use crate::objc::{
    Class, SEL, class_getInstanceMethod, id, method_getTypeEncoding, msg, msg_class, msg_send,
};
use crate::mem::SafeWrite;

pub(super) fn objc_argument_encoding(encoding: &[u8], argument_index: usize) -> Option<&[u8]> {
    crate::objc::type_encoding::method_argument_type(encoding, argument_index)
}

pub(super) fn objc_argument_type(encoding: &[u8], argument_index: usize) -> Option<u8> {
    objc_argument_encoding(encoding, argument_index)?
        .first()
        .copied()
}

pub(super) fn kvc_setter_argument_encoding(
    env: &mut Environment,
    class: Class,
    selector: SEL,
) -> Option<Vec<u8>> {
    let method = class_getInstanceMethod(env, class, selector);
    if method.is_null() {
        return None;
    }
    let encoding = method_getTypeEncoding(env, method);
    if encoding.is_null() {
        return None;
    }
    let encoding = env.mem.cstr_at(encoding.cast()).to_vec();
    objc_argument_encoding(&encoding, 2).map(<[u8]>::to_vec)
}

/// Return-type encoding of a (getter) selector on `class`, e.g. `b"f"` for a
/// method returning `float` or `b"{CGPoint=ff}"` for one returning `CGPoint`.
pub(super) fn kvc_getter_return_encoding(
    env: &mut Environment,
    class: Class,
    selector: SEL,
) -> Option<Vec<u8>> {
    let method = class_getInstanceMethod(env, class, selector);
    if method.is_null() {
        return None;
    }
    let encoding = method_getTypeEncoding(env, method);
    if encoding.is_null() {
        return None;
    }
    let encoding = env.mem.cstr_at(encoding.cast()).to_vec();
    crate::objc::type_encoding::method_return_type(&encoding).map(<[u8]>::to_vec)
}

fn box_struct_getter<T: SafeRead + SafeWrite + crate::abi::GuestRet + 'static>(
    env: &mut Environment,
    object: id,
    selector: SEL,
    encoding: &[u8],
) -> id {
    let value: T = msg_send(env, (object, selector));
    let bytes_ptr = env.mem.alloc(guest_size_of::<T>());
    env.mem.write(bytes_ptr.cast(), value);
    let type_ptr: crate::mem::ConstVoidPtr =
        env.mem.alloc_and_write_cstr(encoding).cast_const().cast();
    let bytes_const: crate::mem::ConstVoidPtr = bytes_ptr.cast_const();
    let boxed: id = msg_class![env; NSValue valueWithBytes:bytes_const objCType:type_ptr];
    env.mem.free(bytes_ptr);
    boxed
}

/// KVC getter boxing: a `-valueForKey:` getter that returns a scalar or struct
/// must be wrapped in `NSNumber`/`NSValue` (Apple's KVC contract). Returns
/// `Some(boxed)` for scalar/struct return types and `None` for object-typed
/// (`@`/`#`/`:`) or `void` getters, where the caller should dispatch normally.
///
/// Dispatching with the correct Rust return type here is also what makes a
/// `float`/`double` getter read back from the VFP registers instead of `r0`;
/// returning the raw `r0` bits as an `id` (the old behaviour) handed callers a
/// float bit-pattern as an object pointer, which SpriteBuilder's CCBReader then
/// messaged, corrupting the object graph.
pub(super) fn box_kvc_getter_result(
    env: &mut Environment,
    object: id,
    selector: SEL,
) -> Option<id> {
    let class: Class = msg![env; object class];
    let encoding = kvc_getter_return_encoding(env, class, selector)?;
    let boxed: id = match encoding.first().copied()? {
        b'@' | b'#' | b':' | b'v' => return None,
        b'B' => {
            let v: bool = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithBool:v]
        }
        b'c' => {
            let v: i8 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithChar:v]
        }
        b'C' => {
            let v: u8 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithUnsignedChar:v]
        }
        b's' => {
            let v: i16 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithShort:v]
        }
        b'S' => {
            let v: u16 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithUnsignedShort:v]
        }
        b'i' => {
            let v: i32 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithInt:v]
        }
        b'I' => {
            let v: u32 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithUnsignedInt:v]
        }
        b'l' => {
            let v: i32 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithLong:v]
        }
        b'L' => {
            let v: u32 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithUnsignedLong:v]
        }
        b'q' => {
            let v: i64 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithLongLong:v]
        }
        b'Q' => {
            let v: u64 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithUnsignedLongLong:v]
        }
        b'f' => {
            let v: f32 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithFloat:v]
        }
        b'd' => {
            let v: f64 = msg_send(env, (object, selector));
            msg_class![env; NSNumber numberWithDouble:v]
        }
        b'{' => match encoding.as_slice() {
            b"{CGPoint=ff}" => box_struct_getter::<CGPoint>(env, object, selector, &encoding),
            b"{CGSize=ff}" => box_struct_getter::<CGSize>(env, object, selector, &encoding),
            b"{CGRect={CGPoint=ff}{CGSize=ff}}" => {
                box_struct_getter::<CGRect>(env, object, selector, &encoding)
            }
            b"{CGAffineTransform=ffffff}" => {
                box_struct_getter::<CGAffineTransform>(env, object, selector, &encoding)
            }
            b"{CATransform3D=ffffffffffffffff}" => {
                box_struct_getter::<CATransform3D>(env, object, selector, &encoding)
            }
            _ => return None,
        },
        _ => return None,
    };
    Some(boxed)
}

pub(super) fn kvc_setter_argument_type(
    env: &mut Environment,
    class: Class,
    selector: SEL,
) -> Option<u8> {
    kvc_setter_argument_encoding(env, class, selector)?
        .first()
        .copied()
}

pub(super) fn send_numeric_kvc_setter(
    env: &mut Environment,
    object: id,
    selector: SEL,
    value: id,
    argument_type: u8,
) -> bool {
    match argument_type {
        b'B' => {
            let value: bool = msg![env; value boolValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'c' => {
            let value: i8 = msg![env; value charValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'C' => {
            let value: u8 = msg![env; value unsignedCharValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b's' => {
            let value: i16 = msg![env; value shortValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'S' => {
            let value: u16 = msg![env; value unsignedShortValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'i' => {
            let value: i32 = msg![env; value intValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'I' => {
            let value: u32 = msg![env; value unsignedIntValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'l' => {
            let value: i32 = msg![env; value longValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'L' => {
            let value: NSUInteger = msg![env; value unsignedLongValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'q' => {
            let value: i64 = msg![env; value longLongValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'Q' => {
            let value: u64 = msg![env; value unsignedLongLongValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'f' => {
            let value: f32 = msg![env; value floatValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b'd' => {
            let value: f64 = msg![env; value doubleValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        _ => return false,
    }
    true
}

pub(super) fn send_nsvalue_kvc_setter(
    env: &mut Environment,
    object: id,
    selector: SEL,
    value: id,
    argument_encoding: &[u8],
) -> bool {
    match argument_encoding {
        b"{CGPoint=ff}" => {
            let value: CGPoint = msg![env; value CGPointValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b"{CGSize=ff}" => {
            let value: CGSize = msg![env; value CGSizeValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b"{CGRect={CGPoint=ff}{CGSize=ff}}" => {
            let value: CGRect = msg![env; value CGRectValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        b"{CGAffineTransform=ffffff}" => {
            let value: CGAffineTransform = read_nsvalue(env, value);
            let _: () = msg_send(env, (object, selector, value));
        }
        b"{CATransform3D=ffffffffffffffff}" => {
            let value: CATransform3D = msg![env; value CATransform3DValue];
            let _: () = msg_send(env, (object, selector, value));
        }
        _ => return false,
    }
    true
}

pub(super) fn send_kvc_setter(
    env: &mut Environment,
    class: Class,
    object: id,
    selector: SEL,
    value: id,
    is_number: bool,
    is_value: bool,
) -> bool {
    let Some(encoding) = kvc_setter_argument_encoding(env, class, selector) else {
        return false;
    };
    if is_number {
        if let Some(argument_type) = encoding.first().copied() {
            if send_numeric_kvc_setter(env, object, selector, value, argument_type) {
                return true;
            }
        }
        return false;
    }
    is_value && send_nsvalue_kvc_setter(env, object, selector, value, &encoding)
}

fn read_nsvalue<T: SafeRead>(env: &mut Environment, value: id) -> T {
    let buffer = env.mem.alloc(guest_size_of::<T>());
    let _: () = msg![env; value getValue:buffer];
    let result = env.mem.read(buffer.cast());
    env.mem.free(buffer);
    result
}
