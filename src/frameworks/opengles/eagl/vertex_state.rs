//! State touched by the GLES2 presentation quad, in the guest's GL context.
use crate::gles::{gles2_raw as gl, GLES};
use std::ffi::c_void;

pub(super) struct SavedAttribute {
    values: [i32; 6],
    pointer: *mut c_void,
}

impl SavedAttribute {
    pub(super) unsafe fn capture(gles: &mut dyn GLES, attribute: u32) -> Self {
        let mut values = [0; 6];
        for (value, pname) in values.iter_mut().zip([
            gl::VERTEX_ATTRIB_ARRAY_ENABLED,
            gl::VERTEX_ATTRIB_ARRAY_SIZE,
            gl::VERTEX_ATTRIB_ARRAY_TYPE,
            gl::VERTEX_ATTRIB_ARRAY_NORMALIZED,
            gl::VERTEX_ATTRIB_ARRAY_STRIDE,
            gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING,
        ]) {
            gles.GetVertexAttribiv(attribute, pname, value);
        }
        let mut pointer = std::ptr::null_mut();
        gles.GetVertexAttribPointerv(attribute, gl::VERTEX_ATTRIB_ARRAY_POINTER, &mut pointer);
        Self { values, pointer }
    }

    pub(super) unsafe fn restore(self, gles: &mut dyn GLES, attribute: u32) {
        let [enabled, size, kind, normalized, stride, buffer] = self.values;
        gles.BindBuffer(gl::ARRAY_BUFFER, buffer as u32);
        gles.VertexAttribPointer(attribute, size, kind as u32, normalized as _, stride, self.pointer);
        if enabled != 0 {
            gles.EnableVertexAttribArray(attribute);
        } else {
            gles.DisableVertexAttribArray(attribute);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Context {
        values: [i32; 6],
        pointer: usize,
        bound: u32,
    }
    impl GLES for Context {
        unsafe fn GetVertexAttribiv(&mut self, _: u32, pname: u32, out: *mut i32) {
            let index = [gl::VERTEX_ATTRIB_ARRAY_ENABLED, gl::VERTEX_ATTRIB_ARRAY_SIZE,
                gl::VERTEX_ATTRIB_ARRAY_TYPE, gl::VERTEX_ATTRIB_ARRAY_NORMALIZED,
                gl::VERTEX_ATTRIB_ARRAY_STRIDE, gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING]
                .iter().position(|p| *p == pname).unwrap();
            *out = self.values[index];
        }
        unsafe fn GetVertexAttribPointerv(&mut self, _: u32, _: u32, out: *mut *mut c_void) {
            *out = self.pointer as *mut c_void;
        }
        unsafe fn BindBuffer(&mut self, target: u32, buffer: u32) {
            assert_eq!(target, gl::ARRAY_BUFFER);
            self.bound = buffer;
        }
        unsafe fn VertexAttribPointer(&mut self, _: u32, size: i32, kind: u32,
            normalized: u8, stride: i32, pointer: *const c_void) {
            self.values[1..].copy_from_slice(&[size, kind as i32, normalized as i32,
                stride, self.bound as i32]);
            self.pointer = pointer as usize;
        }
        unsafe fn EnableVertexAttribArray(&mut self, _: u32) { self.values[0] = 1; }
        unsafe fn DisableVertexAttribArray(&mut self, _: u32) { self.values[0] = 0; }
    }

    #[test]
    fn presentation_restores_vbo_offsets_and_disabled_client_arrays() {
        // glVertexAttribPointer records the current ARRAY_BUFFER binding.
        // https://registry.khronos.org/OpenGL-Refpages/es2.0/xhtml/glVertexAttribPointer.xml
        for (enabled, buffer, pointer) in [(1, 17, 24), (0, 0, 0x123400)] {
            let expected = [enabled, 3, gl::SHORT as i32, 1, 28, buffer];
            let mut gl = Context { values: expected, pointer, bound: 99 };
            unsafe {
                let saved = SavedAttribute::capture(&mut gl, 1);
                gl.BindBuffer(gl::ARRAY_BUFFER, 77);
                gl.VertexAttribPointer(1, 2, gl::FLOAT, 0, 16, std::ptr::null());
                gl.EnableVertexAttribArray(1);
                saved.restore(&mut gl, 1);
            }
            assert_eq!(gl.values, expected);
            assert_eq!(gl.pointer, pointer);
        }
    }
}
