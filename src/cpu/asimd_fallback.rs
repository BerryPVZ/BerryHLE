/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Software fallback for Advanced SIMD (NEON) instructions that the bundled
//! dynarmic does not implement.
//!
//! dynarmic raises an UndefinedInstruction for these, so the guest traps out
//! of the JIT. [emulate] performs the instruction on the saved register file
//! instead, and the caller resumes at the next instruction. Each emulated
//! instruction costs a trip out of the JIT, so this is only meant for
//! instructions that are rare enough not to matter for performance.
//!
//! Currently covered: the "add/subtract and narrow, returning high half"
//! group (`VADDHN`, `VRADDHN`, `VSUBHN`, `VRSUBHN`). Compilers emit
//! `VADDHN.I16` for the `(x + (x >> 8)) >> 8` idiom used to divide by 255
//! when blending pixels, and Unity's image code is full of it.

/// One of the instructions [emulate] knows.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum HighNarrowOp {
    Add,
    RoundingAdd,
    Sub,
    RoundingSub,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct HighNarrow {
    op: HighNarrowOp,
    /// Size of each result element in bits: 8, 16 or 32. The operands have
    /// elements twice that size.
    esize: u32,
    /// Destination D register.
    d: usize,
    /// First operand Q register.
    n: usize,
    /// Second operand Q register.
    m: usize,
}

/// Decode an ARM-state (A1) encoding.
fn decode_arm(instr: u32) -> Option<HighNarrow> {
    // 1111001U 1Dss nnnn dddd 0x00 N0M0 mmmm, opcode 0100 (add) or 0110 (sub).
    if instr & 0xfe80_0d50 != 0xf280_0400 {
        return None;
    }
    let size = (instr >> 20) & 0b11;
    if size == 0b11 {
        // This encoding space belongs to other instructions (VEXT etc).
        return None;
    }
    let vn = (instr >> 16) & 0xf;
    let vm = instr & 0xf;
    if vn & 1 != 0 || vm & 1 != 0 {
        // Odd D registers can't name a Q register: UNDEFINED.
        return None;
    }
    let rounding = (instr >> 24) & 1 != 0;
    let sub = (instr >> 9) & 1 != 0;
    let op = match (sub, rounding) {
        (false, false) => HighNarrowOp::Add,
        (false, true) => HighNarrowOp::RoundingAdd,
        (true, false) => HighNarrowOp::Sub,
        (true, true) => HighNarrowOp::RoundingSub,
    };
    let d = ((instr >> 22) & 1) << 4 | ((instr >> 12) & 0xf);
    let n = ((instr >> 7) & 1) << 4 | vn;
    let m = ((instr >> 5) & 1) << 4 | vm;
    Some(HighNarrow {
        op,
        esize: 8 << size,
        d: d as usize,
        n: (n / 2) as usize,
        m: (m / 2) as usize,
    })
}

/// Decode a Thumb-state (T1) encoding, given as its two halfwords.
fn decode_thumb(hw1: u16, hw2: u16) -> Option<HighNarrow> {
    // Thumb's 111U1111 prefix is ARM's 1111001U; the rest is identical.
    if hw1 & 0xef00 != 0xef00 {
        return None;
    }
    let u = u32::from((hw1 >> 12) & 1);
    let arm = 0xf200_0000 | u << 24 | u32::from(hw1 & 0xff) << 16 | u32::from(hw2);
    decode_arm(arm)
}

/// Compute one result lane. `a` and `b` are the 2×`esize`-bit operands.
fn high_narrow_lane(op: HighNarrowOp, esize: u32, a: u64, b: u64) -> u64 {
    let wide_mask = if esize == 32 {
        u64::MAX
    } else {
        (1u64 << (esize * 2)) - 1
    };
    let round = 1u64 << (esize - 1);
    let value = match op {
        HighNarrowOp::Add => a.wrapping_add(b),
        HighNarrowOp::RoundingAdd => a.wrapping_add(b).wrapping_add(round),
        HighNarrowOp::Sub => a.wrapping_sub(b),
        HighNarrowOp::RoundingSub => a.wrapping_sub(b).wrapping_add(round),
    } & wide_mask;
    (value >> esize) & ((1u64 << esize) - 1)
}

/// Compute the 64-bit result for the 128-bit operands `qn` and `qm`.
fn high_narrow(op: HighNarrowOp, esize: u32, qn: u128, qm: u128) -> u64 {
    let wide = esize * 2;
    let wide_mask = (1u128 << wide) - 1;
    let mut result = 0u64;
    for lane in 0..(64 / esize) {
        let a = ((qn >> (lane * wide)) & wide_mask) as u64;
        let b = ((qm >> (lane * wide)) & wide_mask) as u64;
        result |= high_narrow_lane(op, esize, a, b) << (lane * esize);
    }
    result
}

fn read_d(extregs: &[u32; 64], d: usize) -> u64 {
    u64::from(extregs[d * 2]) | u64::from(extregs[d * 2 + 1]) << 32
}

fn read_q(extregs: &[u32; 64], q: usize) -> u128 {
    u128::from(read_d(extregs, q * 2)) | u128::from(read_d(extregs, q * 2 + 1)) << 64
}

fn write_d(extregs: &mut [u32; 64], d: usize, value: u64) {
    extregs[d * 2] = value as u32;
    extregs[d * 2 + 1] = (value >> 32) as u32;
}

fn execute(instr: HighNarrow, extregs: &mut [u32; 64]) {
    let qn = read_q(extregs, instr.n);
    let qm = read_q(extregs, instr.m);
    write_d(extregs, instr.d, high_narrow(instr.op, instr.esize, qn, qm));
}

/// Try to perform the instruction that raised an UndefinedInstruction.
///
/// `read_u16` reads a guest halfword. In Thumb state the instruction starts at
/// `pc`; in ARM state it is the little-endian word there. Returns the length
/// of the emulated instruction, so the caller can resume after it, or `None`
/// if the instruction isn't one this module knows (the register file is then
/// untouched).
pub fn emulate(
    pc: u32,
    thumb: bool,
    read_u16: impl Fn(u32) -> Option<u16>,
    extregs: &mut [u32; 64],
) -> Option<u32> {
    let hw1 = read_u16(pc)?;
    let hw2 = read_u16(pc.wrapping_add(2))?;
    let instr = if thumb {
        decode_thumb(hw1, hw2)?
    } else {
        decode_arm(u32::from(hw1) | u32::from(hw2) << 16)?
    };
    execute(instr, extregs);
    Some(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_neighbors_og_instructions() {
        // From the Unity pixel blend loop in NeighborsOG:
        //   vaddhn.i16 d19, q10, q9  (Thumb: efc4 34a2)
        //   vaddhn.i16 d18, q11, q8  (Thumb: efc6 24a0)
        assert_eq!(
            decode_thumb(0xefc4, 0x34a2),
            Some(HighNarrow {
                op: HighNarrowOp::Add,
                esize: 8,
                d: 19,
                n: 10,
                m: 9,
            })
        );
        assert_eq!(
            decode_thumb(0xefc6, 0x24a0),
            Some(HighNarrow {
                op: HighNarrowOp::Add,
                esize: 8,
                d: 18,
                n: 11,
                m: 8,
            })
        );
    }

    #[test]
    fn decodes_each_variant_and_size() {
        // vaddhn.i32 d0, q1, q2
        assert_eq!(
            decode_arm(0xf2920404).map(|i| (i.op, i.esize, i.d, i.n, i.m)),
            Some((HighNarrowOp::Add, 16, 0, 1, 2))
        );
        // vraddhn.i64 d31, q15, q0
        assert_eq!(
            decode_arm(0xf3eef480).map(|i| (i.op, i.esize, i.d, i.n, i.m)),
            Some((HighNarrowOp::RoundingAdd, 32, 31, 15, 0))
        );
        // vsubhn.i16 d1, q2, q3
        assert_eq!(
            decode_arm(0xf2841606).map(|i| (i.op, i.esize, i.d, i.n, i.m)),
            Some((HighNarrowOp::Sub, 8, 1, 2, 3))
        );
        // vrsubhn.i16 d1, q2, q3
        assert_eq!(
            decode_arm(0xf3841606).map(|i| (i.op, i.esize, i.d, i.n, i.m)),
            Some((HighNarrowOp::RoundingSub, 8, 1, 2, 3))
        );
    }

    #[test]
    fn rejects_other_instructions() {
        // vaddl.u8 q8, d0, d1 (same space, opcode 0000)
        assert_eq!(decode_arm(0xf3c00001), None);
        // vadd.i16 d0, d1, d2 (three registers of the same length)
        assert_eq!(decode_arm(0xf2110802), None);
        // vaddhn with an odd D register as a Q operand is UNDEFINED.
        assert_eq!(decode_arm(0xf2810404), None);
        // size == 0b11 is VEXT.
        assert_eq!(decode_arm(0xf2b00400), None);
        // An ordinary Thumb-2 instruction (ldr.w r0, [r5, #432]).
        assert_eq!(decode_thumb(0xf8d5, 0x01b0), None);
    }

    #[test]
    fn divides_by_255_like_the_blend_loop() {
        // With t = x + 128, (t + (t >> 8)) >> 8 is x / 255 rounded to nearest
        // for every product x of two 8-bit values.
        for x in 0..=255u64 * 255 {
            let t = x + 128;
            let expected = (x * 2 + 255) / 510;
            assert_eq!(high_narrow_lane(HighNarrowOp::Add, 8, t, t >> 8), expected);
        }
    }

    #[test]
    fn lane_arithmetic() {
        use HighNarrowOp::*;
        // Wrapping and rounding at each size.
        assert_eq!(high_narrow_lane(Add, 8, 0xffff, 0x0001), 0x00);
        assert_eq!(high_narrow_lane(Add, 8, 0x1280, 0x0000), 0x12);
        assert_eq!(high_narrow_lane(RoundingAdd, 8, 0x1280, 0x0000), 0x13);
        assert_eq!(high_narrow_lane(RoundingAdd, 8, 0x127f, 0x0000), 0x12);
        assert_eq!(high_narrow_lane(Sub, 8, 0x0000, 0x0001), 0xff);
        assert_eq!(high_narrow_lane(RoundingSub, 8, 0x1300, 0x0080), 0x13);
        assert_eq!(high_narrow_lane(Add, 16, 0x1234_5678, 0x0001_0000), 0x1235);
        assert_eq!(
            high_narrow_lane(RoundingAdd, 32, 0xffff_ffff_8000_0000, 0),
            0
        );
        assert_eq!(
            high_narrow_lane(Sub, 32, 0x1_0000_0000, 0x2_0000_0000),
            0xffff_ffff
        );
    }

    #[test]
    fn executes_on_the_register_file() {
        let mut extregs = [0u32; 64];
        // q9 = d18:d19, q10 = d20:d21. Lanes of q10 are 0x0100 * lane.
        let q10: u128 = (0..8u128).map(|i| (0x0100 * i) << (16 * i)).sum();
        let q9: u128 = (0..8u128).map(|i| 0x0001 << (16 * i)).sum();
        for (i, word) in (0..4).map(|w| (w, (q10 >> (32 * w)) as u32)) {
            extregs[40 + i] = word;
        }
        for (i, word) in (0..4).map(|w| (w, (q9 >> (32 * w)) as u32)) {
            extregs[36 + i] = word;
        }
        let read_u16 = |addr: u32| Some(if addr == 0x1000 { 0xefc4 } else { 0x34a2 });
        assert_eq!(emulate(0x1000, true, read_u16, &mut extregs), Some(4));
        // d19 = high bytes of q10 + q9 lanes: 0, 1, ..., 7.
        assert_eq!(read_d(&extregs, 19), 0x0706_0504_0302_0100);
        // Operands are untouched.
        assert_eq!(read_q(&extregs, 10), q10);
    }
}
