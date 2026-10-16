//! VU0 micro mode and the VIF0 upload, proved against hand-assembled micro programs: what `vcallms` runs, and the
//! pipeline timing a program's results can depend on (`Q` 7 cycles after a divide, flags 4 cycles after the
//! instruction that set them, FMAC stalls, branch delay slots, the `I` constant).

use ps2_float::vu;
use ps2_oracle::Ee;

// --- the world's smallest VU assembler -------------------------------------------------------------

const X: u32 = 8;
const XYZW: u32 = 15;
/// The `I` bit (the lower word is a constant for `I`) and the `E` bit (the program ends after the next pair).
const IBIT: u32 = 1 << 31;
const EBIT: u32 = 1 << 30;

/// An upper word: `fun.dest fd, fs, ft`.
const fn up(fun: u32, dest: u32, fd: u32, fs: u32, ft: u32) -> u32 {
    dest << 21 | ft << 16 | fs << 11 | fd << 6 | fun
}
/// An upper word of the "special" group (accumulator targets, `nop`, …): `op.dest ft, fs`.
const fn ups(op: u32, dest: u32, ft: u32, fs: u32) -> u32 {
    dest << 21 | ft << 16 | fs << 11 | (op >> 2) << 6 | 0x3c | op & 3
}
/// A lower word of the second special group: `op.dest ft, fs` with field selectors for the divider.
const fn low2(op: u32, dest: u32, ft: u32, fs: u32) -> u32 {
    0x40 << 25 | dest << 21 | ft << 16 | fs << 11 | (op >> 2) << 6 | 0x3c | op & 3
}
/// `div Q, vf[fs].fsf, vf[ft].ftf`
const fn div(fs: u32, fsf: u32, ft: u32, ftf: u32) -> u32 {
    0x40 << 25 | ftf << 23 | fsf << 21 | ft << 16 | fs << 11 | (0x38 >> 2) << 6 | 0x3c
}
const fn iaddiu(it: u32, is: u32, imm: u32) -> u32 {
    0x08 << 25 | (imm >> 11) << 21 | it << 16 | is << 11 | imm & 0x7ff
}
const fn fsand(it: u32, imm: u32) -> u32 {
    0x16 << 25 | (imm >> 11) << 21 | it << 16 | imm & 0x7ff
}
const fn fmand(it: u32, is: u32) -> u32 {
    0x1a << 25 | it << 16 | is << 11
}
const fn ibne(it: u32, is: u32, off: i32) -> u32 {
    0x29 << 25 | it << 16 | is << 11 | off as u32 & 0x7ff
}
const UNOP: u32 = ups(0x2f, 0, 0, 0);
const LNOP: u32 = low2(0x30, 0, 0, 0); // move.0 vf0, vf0
const WAITQ: u32 = low2(0x3b, 0, 0, 0);
const ADD: u32 = 0x28;
const MUL: u32 = 0x2a;
const SUB: u32 = 0x2c;
const ADDQ: u32 = 0x20;
const ADDI: u32 = 0x22;

/// A machine with `prog` (pairs of upper, lower) in VU0's micro memory at 0.
fn micro(prog: &[(u32, u32)]) -> Ee {
    let mut ee = Ee::bare(2 << 20);
    for (k, &(u, l)) in prog.iter().enumerate() {
        ee.vu0_micro[8 * k..8 * k + 4].copy_from_slice(&l.to_le_bytes());
        ee.vu0_micro[8 * k + 4..8 * k + 8].copy_from_slice(&u.to_le_bytes());
    }
    ee
}

/// The last pair of a program: the `E` bit, then its delay slot.
const END: [(u32, u32); 2] = [(UNOP | EBIT, LNOP), (UNOP, LNOP)];

// --- the tests --------------------------------------------------------------------------------------

#[test]
fn vcallms_runs_a_program_a_vif0_dma_chain_uploaded() {
    // a refe tag pointing at VIF codes: nop, MPG of 4 instructions to address 0
    let mut ee = Ee::bare(2 << 20);
    let (tag, data) = (0x1000u32, 0x2000u32);
    let prog = [(up(ADD, XYZW, 3, 1, 2), LNOP), END[0], END[1], (UNOP, LNOP)];
    let mut words = vec![0, 0x4a00_0000 | (prog.len() as u32) << 16];
    words.extend(prog.iter().flat_map(|&(u, l)| [l, u]));
    for (k, w) in words.iter().enumerate() {
        ee.set_u32(data + 4 * k as u32, *w);
    }
    ee.write(tag, 8, ((data as u128) << 32) | (words.len() as u128).div_ceil(4));
    ee.vif0_dma(tag, false);

    // the EE side: load vf1/vf2, vcallms 0, then store vf3 through an interlocking qmfc2
    let code = 0x10_0000u32;
    let ee_prog = [
        0xd881_0000u32, // lqc2 vf1, 0(a0)
        0xd8a2_0000,    // lqc2 vf2, 0(a1)
        0x4a00_0038,    // vcallms 0
        0x4828_1801,    // qmfc2.i t0, vf3
        0x7cc8_0000,    // sq t0, 0(a2)
        0x03e0_0008,    // jr ra
        0,
    ];
    for (k, w) in ee_prog.iter().enumerate() {
        ee.set_u32(code + 4 * k as u32, *w);
    }
    let (a, b, out) = (ee.scratch(0), ee.scratch(1), ee.scratch(2));
    let (u, v) = ([1.5f32, -2.0, 1e-3, 7.0], [3.0f32, 0.5, -1.0, f32::MAX]);
    for k in 0..4 {
        ee.set_f32(a + 4 * k as u32, u[k]);
        ee.set_f32(b + 4 * k as u32, v[k]);
    }
    ee.call(code, &[a as u64, b as u64, out as u64], &[], 100);
    for k in 0..4 {
        assert_eq!(ee.f32_at(out + 4 * k as u32).to_bits(), vu::add(u[k], v[k]).to_bits(), "lane {k}");
    }
    assert_eq!(ee.vu0_cycles, 4, "three pairs, and the add issued at cycle 0 lands at 4");
}

#[test]
fn q_lands_seven_cycles_after_a_divide_and_waitq_waits_for_it() {
    let mut p = vec![(UNOP, div(1, 0, 2, 0))]; // Q = 1 / 3, issued at cycle 0
    for k in 1..=7 {
        p.push((up(ADDQ, X, k + 2, 0, 0), LNOP)); // vf3..vf9.x = 0 + Q at cycles 1..7
    }
    p.push((UNOP, div(2, 0, 1, 0))); // Q = 3 / 1 at cycle 8
    p.push((UNOP, WAITQ)); // stalls to cycle 15
    p.push((up(ADDQ, X, 10, 0, 0), LNOP));
    p.extend(END);
    let mut ee = micro(&p);
    ee.vf[1] = [1.0, 0.0, 0.0, 0.0];
    ee.vf[2] = [3.0, 0.0, 0.0, 0.0];
    ee.q = 5.0;
    ee.vu0_run(0, 100);
    let third = vu::div(1.0, 3.0);
    for k in 3..=8 {
        assert_eq!(ee.vf[k][0], 5.0, "vf{k}: before the divide lands, Q is the old value");
    }
    assert_eq!(ee.vf[9][0].to_bits(), third.to_bits(), "7 cycles after the divide");
    assert_eq!(ee.vf[10][0], 3.0, "after waitq");
    assert_eq!(ee.q, 3.0);
}

#[test]
fn flags_land_four_cycles_after_the_instruction_that_set_them() {
    let mut ee = micro(&[
        (up(SUB, X, 0, 1, 1), LNOP), // cycle 0: x − x = 0, Z
        (UNOP, fsand(1, 1)),         // cycle 1: too early
        (UNOP, LNOP),
        (UNOP, fsand(2, 1)),         // cycle 3: still too early
        (UNOP, fsand(3, 3)),         // cycle 4: Z
        (up(MUL, XYZW, 0, 2, 0), LNOP), // cycle 5: (−0, 1, −2, 0) × (0, 0, 0, 1) → −0, 0, −0, 0: S and Z on x and z
        (UNOP, LNOP),
        (UNOP, LNOP),
        (UNOP, LNOP),
        (UNOP, fmand(4, 5)),         // cycle 9: MAC & vi5
        (UNOP, fsand(6, 0xc3)),      // the sticky Z and S bits are there too
        END[0],
        END[1],
    ]);
    ee.vf[1] = [2.5, 0.0, 0.0, 0.0];
    ee.vf[2] = [-0.0, 1.0, -2.0, 0.0];
    ee.vi[5] = 0xffff;
    ee.vu0_run(0, 100);
    assert_eq!(ee.vi[1..4], [0, 0, 1]);
    // MAC: S bits 7..4 = x y z w, Z bits 3..0 — every lane is zero; x and z are negative zeros
    assert_eq!(ee.vi[4], 0b1010_1111);
    assert_eq!(ee.vi[6], 0xc3);
}

#[test]
fn a_dependent_read_stalls_and_a_branch_runs_its_delay_slot() {
    let mut ee = micro(&[
        (up(ADD, X, 3, 1, 1), iaddiu(1, 0, 1)), // cycle 0: vf3.x = 2·vf1.x
        (up(ADD, X, 4, 3, 1), LNOP),            // reads vf3: stalls to cycle 4
        (UNOP, ibne(1, 0, 2)),                  // vi1 ≠ 0: taken, to 2 past the delay slot
        (up(ADD, X, 5, 1, 1), LNOP),            // delay slot: runs
        (up(ADD, X, 6, 1, 1), LNOP),            // skipped
        (UNOP | IBIT, 0x4000_0000),             // I = 2.0 lands after this pair
        (up(ADDI, X, 7, 0, 0) | EBIT, LNOP),    // vf7.x = 0 + I
        (UNOP, LNOP),
    ]);
    ee.vf[1] = [1.0; 4];
    ee.vu0_run(0, 100);
    assert_eq!([ee.vf[3][0], ee.vf[4][0], ee.vf[5][0], ee.vf[6][0], ee.vf[7][0]], [2.0, 3.0, 2.0, 0.0, 2.0]);
    // stalled to 4, then branch 5, slot 6, I 7, E 8, its slot 9; vf7's add drains at 12
    assert_eq!(ee.vu0_cycles, 6 + 4 + 2);
}

#[test]
fn ctc2_and_cfc2_move_the_integer_and_control_registers() {
    let code = 0x10_0000u32;
    let mut ee = Ee::bare(2 << 20);
    for (k, w) in [
        0x48c4_0800u32, // ctc2 a0, vi1
        0x48c5_b000,    // ctc2 a1, Q (22)
        0x4842_0800,    // cfc2 v0, vi1
        0x4843_b000,    // cfc2 v1, Q
        0x03e0_0008,
        0,
    ]
    .iter()
    .enumerate()
    {
        ee.set_u32(code + 4 * k as u32, *w);
    }
    ee.call(code, &[0xffff_8001, 1.5f32.to_bits() as u64], &[], 100);
    assert_eq!(ee.vi[1], 0x8001);
    assert_eq!(ee.q, 1.5);
    assert_eq!(ee.r(2), 0x8001, "vi registers are 16 bits, read back zero-extended");
    assert_eq!(ee.r(3) as u32, 1.5f32.to_bits());
}
