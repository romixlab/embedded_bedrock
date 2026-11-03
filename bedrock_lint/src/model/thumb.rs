//! What the elf checks read from Arm Thumb code without a full disassembler: the stack frame a function's prologue
//! sets up, and the functions it calls with `bl` (and tail-calls with `b.w`).
//!
//! The prologue is read until the first branch, at most [`PROLOGUE`] halfwords: `push` / `push.w` / `str.w ..,
//! [sp, #-4]!` / `vpush`, then `sub sp, #imm`, `sub.w` / `subw sp, sp, #imm`, or a literal load (`ldr rN, [pc, #]`,
//! `movw` / `movt`) followed by `add sp, rN` / `sub.w sp, sp, rN` for frames too large for an immediate. Calls are
//! every `bl` in the function's bytes and every `b` / `b.w` that leaves it (a tail call), decoded linearly: a literal pool in the middle of a function can read as an
//! instruction, so a call graph from this is an estimate, and calls through a pointer (`blx rN`) are only counted.

/// Halfwords of a prologue looked at
pub const PROLOGUE: usize = 32;

/// What one function's code says
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Code {
    /// Bytes the prologue takes from the stack (pushes and the frame); `None` when no prologue was recognized
    pub frame: Option<u32>,
    /// Targets of `bl` and of `b.w` that leave the function, as addresses
    pub calls: Vec<u64>,
    /// `blx rN`: calls through a pointer
    pub indirect: u32,
}

fn hw(code: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*code.get(i)?, *code.get(i + 1)?]))
}

/// A 32-bit Thumb-2 instruction starts with 0b11101, 0b11110 or 0b11111
fn is_wide(h: u16) -> bool {
    matches!(h >> 11, 0b11101..=0b11111)
}

/// The `bl` / `b.w` (T4) target of a 32-bit instruction at `pc`, and whether it links
fn branch_target(pc: u64, h1: u16, h2: u16) -> Option<(u64, bool)> {
    if h1 & 0xF800 != 0xF000 {
        return None;
    }
    let link = match h2 & 0xD000 {
        0xD000 => true,
        0x9000 => false,
        _ => return None,
    };
    let s = ((h1 >> 10) & 1) as u32;
    let imm10 = (h1 & 0x3FF) as u32;
    let j1 = ((h2 >> 13) & 1) as u32;
    let j2 = ((h2 >> 11) & 1) as u32;
    let imm11 = (h2 & 0x7FF) as u32;
    let i1 = !(j1 ^ s) & 1;
    let i2 = !(j2 ^ s) & 1;
    let imm = (s << 24) | (i1 << 23) | (i2 << 22) | (imm10 << 12) | (imm11 << 1);
    // sign-extend 25 bits
    let off = ((imm << 7) as i32) >> 7;
    Some(((pc as i64 + 4 + off as i64) as u64, link))
}

/// ThumbExpandImm of `i:imm3:imm8` (the rotated forms included)
fn expand_imm(i: u32, imm3: u32, imm8: u32) -> u32 {
    let imm12 = (i << 11) | (imm3 << 8) | imm8;
    if imm12 >> 10 == 0 {
        match (imm12 >> 8) & 3 {
            0 => imm8,
            1 => (imm8 << 16) | imm8,
            2 => (imm8 << 24) | (imm8 << 8),
            _ => (imm8 << 24) | (imm8 << 16) | (imm8 << 8) | imm8,
        }
    } else {
        let unrot = 0x80 | (imm12 & 0x7F);
        unrot.rotate_right(imm12 >> 7)
    }
}

/// Reads the function whose code is `code`, starting at `addr`. `literal` reads a word of the image (for the
/// literal pool a large frame is loaded from).
pub fn read(addr: u64, code: &[u8], literal: impl Fn(u64) -> Option<u32>) -> Code {
    let mut out = Code {
        frame: prologue(addr, code, &literal),
        ..Code::default()
    };
    let end = addr + code.len() as u64;
    let mut i = 0;
    while let Some(h1) = hw(code, i) {
        if is_wide(h1) {
            let Some(h2) = hw(code, i + 2) else { break };
            let pc = addr + i as u64;
            if let Some((t, link)) = branch_target(pc, h1, h2) {
                // a b.w inside the function is a jump, not a call
                if link || !(addr..end).contains(&t) {
                    out.calls.push(t);
                }
            }
            i += 4;
        } else {
            // blx rN
            if h1 & 0xFF87 == 0x4780 {
                out.indirect += 1;
            }
            // b (T2) out of the function: a tail call
            if h1 & 0xF800 == 0xE000 {
                let off = (((h1 & 0x7FF) << 5) as i16 >> 4) as i64;
                let t = (addr as i64 + i as i64 + 4 + off) as u64;
                if !(addr..end).contains(&t) {
                    out.calls.push(t);
                }
            }
            i += 2;
        }
    }
    out.calls.sort_unstable();
    out.calls.dedup();
    out
}

fn prologue(addr: u64, code: &[u8], literal: &impl Fn(u64) -> Option<u32>) -> Option<u32> {
    let mut total: u32 = 0;
    let mut seen = false;
    // registers loaded with a constant: (register, value)
    let mut consts: [Option<u32>; 16] = [None; 16];
    let mut i = 0;
    while i < PROLOGUE * 2 {
        let h1 = hw(code, i)?;
        let pc = addr + i as u64;
        if is_wide(h1) {
            let h2 = hw(code, i + 2)?;
            i += 4;
            // push.w {reglist} (stmdb sp!, {..})
            if h1 == 0xE92D {
                total += 4 * (h2 & 0xDFFF).count_ones();
                seen = true;
            // str.w rT, [sp, #-4]!
            } else if h1 == 0xF84D && h2 & 0x0FFF == 0x0D04 {
                total += 4;
                seen = true;
            // vpush {d..} / {s..}: imm8 words
            } else if (h1 & 0xFFBF) == 0xED2D && (h2 & 0x0E00) == 0x0A00 {
                total += 4 * (h2 & 0xFF) as u32;
                seen = true;
            // sub.w sp, sp, #const (T3)
            } else if (h1 & 0xFBFF) == 0xF1AD && (h2 & 0x8F00) == 0x0D00 {
                let v = expand_imm(
                    ((h1 >> 10) & 1) as u32,
                    ((h2 >> 12) & 7) as u32,
                    (h2 & 0xFF) as u32,
                );
                return Some(total + v);
            // subw sp, sp, #imm12 (T4)
            } else if (h1 & 0xFBFF) == 0xF2AD && (h2 & 0x8F00) == 0x0D00 {
                let v = (((h1 >> 10) & 1) as u32) << 11
                    | (((h2 >> 12) & 7) as u32) << 8
                    | (h2 & 0xFF) as u32;
                return Some(total + v);
            // sub.w sp, sp, rM (T2 register, no shift)
            } else if h1 == 0xEBAD && (h2 & 0xFFF0) == 0x0D00 {
                let rm = (h2 & 0xF) as usize;
                return consts[rm].map(|v| total + v);
            // movw rD, #imm16
            } else if (h1 & 0xFBF0) == 0xF240 && h2 & 0x8000 == 0 {
                let rd = ((h2 >> 8) & 0xF) as usize;
                let v = ((h1 & 0xF) as u32) << 12
                    | (((h1 >> 10) & 1) as u32) << 11
                    | (((h2 >> 12) & 7) as u32) << 8
                    | (h2 & 0xFF) as u32;
                consts[rd] = Some(v);
            // movt rD, #imm16
            } else if (h1 & 0xFBF0) == 0xF2C0 && h2 & 0x8000 == 0 {
                let rd = ((h2 >> 8) & 0xF) as usize;
                let v = ((h1 & 0xF) as u32) << 12
                    | (((h1 >> 10) & 1) as u32) << 11
                    | (((h2 >> 12) & 7) as u32) << 8
                    | (h2 & 0xFF) as u32;
                consts[rd] = consts[rd].map(|lo| (lo & 0xFFFF) | (v << 16));
            // ldr.w rT, [pc, #imm12]
            } else if (h1 & 0xFF7F) == 0xF85F {
                let rt = ((h2 >> 12) & 0xF) as usize;
                let off = (h2 & 0xFFF) as i64;
                let base = ((pc + 4) & !3) as i64;
                let at = if h1 & 0x80 != 0 {
                    base + off
                } else {
                    base - off
                };
                consts[rt] = literal(at as u64);
            } else if branch_target(pc, h1, h2).is_some() {
                break;
            }
        } else {
            i += 2;
            match h1 {
                // push {reglist[, lr]}
                h if h & 0xFE00 == 0xB400 => {
                    total += 4 * (h & 0xFF).count_ones() + 4 * ((h >> 8) & 1) as u32;
                    seen = true;
                }
                // sub sp, #imm7*4
                h if h & 0xFF80 == 0xB080 => return Some(total + 4 * (h & 0x7F) as u32),
                // ldr rT, [pc, #imm8*4]
                h if h & 0xF800 == 0x4800 => {
                    let rt = ((h >> 8) & 7) as usize;
                    let at = ((pc + 4) & !3) + 4 * (h & 0xFF) as u64;
                    consts[rt] = literal(at);
                }
                // movs rD, #imm8
                h if h & 0xF800 == 0x2000 => {
                    consts[((h >> 8) & 7) as usize] = Some((h & 0xFF) as u32)
                }
                // add sp, rM: the constant is negative (the frame)
                h if h & 0xFF87 == 0x4485 => {
                    let rm = ((h >> 3) & 0xF) as usize;
                    return consts[rm].map(|v| total + v.wrapping_neg());
                }
                // b, bx, blx, a conditional branch, pop: the prologue is over
                h if h & 0xF800 == 0xE000
                    || h & 0xFF00 == 0x4700
                    || h & 0xF000 == 0xD000
                    || h & 0xFE00 == 0xBC00 =>
                {
                    break;
                }
                _ => {}
            }
        }
    }
    seen.then_some(total)
}

/// Thumb encodings for the synthetic ELFs of the tests: a function with a prologue of `frame` bytes that calls
/// the given addresses
#[doc(hidden)]
pub mod encode {
    /// `push {r7, lr}`, `sub sp, ...`, one `bl` per call, `add sp, ...`, `pop {r7, pc}`; the frame is rounded down
    /// to a multiple of 4 and counts the 8 bytes of the push
    pub fn function(addr: u64, frame: u32, calls: &[u64]) -> Vec<u8> {
        let mut out: Vec<u16> = vec![0xB580];
        let sub = frame.saturating_sub(8) & !3;
        if sub > 0 && sub <= 508 {
            out.push(0xB080 | (sub / 4) as u16);
        } else if sub > 508 {
            // subw sp, sp, #imm12 (T4), up to 4095
            let v = sub.min(4092);
            out.push(0xF2AD | (((v >> 11) & 1) << 10) as u16);
            out.push((((v >> 8) & 7) << 12) as u16 | 0x0D00 | (v & 0xFF) as u16);
        }
        for &t in calls {
            let pc = addr + 2 * out.len() as u64;
            let off = t as i64 - (pc as i64 + 4);
            let imm = off as u32;
            let s = (imm >> 24) & 1;
            let i1 = (imm >> 23) & 1;
            let i2 = (imm >> 22) & 1;
            let j1 = (!(i1 ^ s)) & 1;
            let j2 = (!(i2 ^ s)) & 1;
            out.push((0xF000 | (s << 10) | ((imm >> 12) & 0x3FF)) as u16);
            out.push((0xD000 | (j1 << 13) | (j2 << 11) | ((imm >> 1) & 0x7FF)) as u16);
        }
        out.push(0xBD80);
        out.iter().flat_map(|h| h.to_le_bytes()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(h: &[u16]) -> Vec<u8> {
        h.iter().flat_map(|h| h.to_le_bytes()).collect()
    }

    #[test]
    fn frames_of_usual_prologues() {
        let none = |_| None;
        // push {r4-r7, lr}; sub sp, #16
        assert_eq!(read(0, &bytes(&[0xB5F0, 0xB084]), none).frame, Some(36));
        // push.w {r4-r11, lr}; sub.w sp, sp, #0x400
        assert_eq!(
            read(0, &bytes(&[0xE92D, 0x4FF0, 0xF5AD, 0x6D80]), none).frame,
            Some(36 + 0x400)
        );
        // push {r7, lr}; subw sp, sp, #0xFF0
        assert_eq!(
            read(0, &bytes(&[0xB580, 0xF6AD, 0x7DF0]), none).frame,
            Some(8 + 0xFF0)
        );
        // M0+: push {r4, lr}; ldr r4, [pc, #4]; add sp, r4; b .; .word -0x2000
        let code = bytes(&[0xB510, 0x4C01, 0x44A5, 0xE7FE, 0xE000, 0xFFFF]);
        let lit = |a: u64| {
            let a = a as usize;
            code.get(a..a + 4)
                .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        };
        assert_eq!(read(0, &code, lit).frame, Some(8 + 0x2000));
        // movw r12, #0x7c30; sub.w sp, sp, r12 after push.w {r4-r11, lr}
        assert_eq!(
            read(
                0,
                &bytes(&[0xE92D, 0x4FF0, 0xF647, 0x4C30, 0xEBAD, 0x0D0C]),
                none
            )
            .frame,
            Some(36 + 0x7C30)
        );
        // a leaf without a prologue
        assert_eq!(read(0, &bytes(&[0x4770]), none).frame, None);
    }

    #[test]
    fn calls_round_trip_through_the_encoder() {
        let code = encode::function(0x0800_0100, 0x300, &[0x0800_0000, 0x0804_0000]);
        let c = read(0x0800_0100, &code, |_| None);
        assert_eq!(c.frame, Some(0x300));
        assert_eq!(c.calls, vec![0x0800_0000, 0x0804_0000]);
        let code = encode::function(0x100, 1000, &[]);
        assert_eq!(read(0x100, &code, |_| None).frame, Some(1000));
        // HardFault: mov r0, lr; ...; b _HardFault (T2) 8 bytes on
        let c = read(0x10, &bytes(&[0x4670, 0xE002]), |_| None);
        assert_eq!(c.calls, vec![0x10 + 2 + 4 + 4]);
    }
}
