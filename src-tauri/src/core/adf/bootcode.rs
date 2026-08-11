//! The boot code ART writes into a bootable floppy's boot block.
//!
//! # What Kickstart actually requires
//!
//! The `strap` module reads blocks 0 and 1 (1024 bytes) into memory, checks
//! the `DOS` signature and the boot block checksum, and then **jumps to offset
//! 12** of what it read. From the AmigaOS documentation, the contract at that
//! point is:
//!
//! - `A1` holds an open `trackdisk.device` I/O request.
//! - The code must return `D0 = 0` for success, and `A0` must then hold the
//!   address to jump to. `strap` frees the boot sector, closes the I/O
//!   request, and jumps there.
//! - **A non-zero `D0` raises a system alert and reboots the machine.**
//!
//! That last point is why ART's previous boot block did nothing useful: it
//! wrote a bare `RTS` (`0x4E75`) and returned with whatever happened to be in
//! `D0` (ART-063).
//!
//! # What this code does
//!
//! Exactly what the contract asks and nothing more: find the resident tag for
//! `dos.library`, hand `strap` its init routine, and report success.
//!
//! ```text
//!     move.l  4.w,a6              ; ExecBase — absolute address 4, always
//!     lea     dosName(pc),a1
//!     jsr     -96(a6)             ; exec/FindResident
//!     tst.l   d0
//!     beq.s   fail
//!     movea.l d0,a0               ; a0 -> struct Resident
//!     movea.l 22(a0),a0           ; a0 -> rt_Init
//!     moveq   #0,d0               ; success
//!     rts
//! fail:
//!     moveq   #-1,d0              ; alert and reboot
//!     rts
//! dosName:
//!     dc.b    'dos.library',0
//! ```
//!
//! `ExecBase` is read from absolute address 4 rather than trusting `A6` on
//! entry: address 4 is guaranteed by the architecture, whereas `A6`'s contents
//! at the boot entry point are convention rather than documented contract.
//!
//! # On writing our own
//!
//! This is ART's own implementation, assembled here from the documented
//! contract and the published exec LVO table. Commodore's boot block is
//! copyrighted code and **ART ships no Amiga content, ever** — but the
//! sequence above is dictated by the interface, not chosen for expression:
//! any correct boot block has to locate `dos.library` and return its init
//! routine, and there is essentially one way to say that in 68000.
//!
//! # What this does *not* give you
//!
//! A disk that boots is not the same as a disk that boots to Workbench. Once
//! DOS is running it looks for `S/Startup-Sequence`, and what that script needs
//! — `c/` commands, `libs/`, `l/` handlers — is AmigaOS content ART cannot
//! supply. A disk carrying only this boot code reaches a CLI prompt or a
//! "cannot open S/Startup-Sequence" message, rather than hanging at the
//! insert-disk hand. That is the whole of what this module claims.

/// exec.library `FindResident`, as a two's-complement 16-bit displacement.
const LVO_FIND_RESIDENT: i16 = -96;

/// `rt_Init`'s byte offset inside `struct Resident`.
const RT_INIT: u16 = 22;

/// Where `strap` begins executing, measured from the start of the boot block.
pub const BB_ENTRY: usize = 12;

/// The library the boot code hands control to.
const DOS_NAME: &[u8] = b"dos.library\0";

/// ART's boot code, ready to be copied to [`BB_ENTRY`].
///
/// Assembled by [`boot_code`] rather than written as a byte literal so the two
/// relative displacements are *derived* from the hand-counted landmark
/// offsets below (`LEA_AT`/`BEQ_AT`/`FAIL_AT`/`NAME_AT`) instead of being
/// hand-counted themselves — those four offsets are still hand-counted, but a
/// displacement computed from them cannot drift out of step the way a second,
/// independently hand-counted number could. Miscounting a landmark is what
/// produces a disk which hangs a real machine rather than failing a test; the
/// landmark test and the `debug_assert` below are the real protection.
pub fn boot_code() -> Vec<u8> {
    // Offsets below are relative to the start of this fragment, which lands at
    // BB_ENTRY in the block. Sizes are fixed, so both displacements can be
    // derived rather than assumed.
    const LEA_AT: usize = 4; // `lea d(pc),a1` opcode word
    const BEQ_AT: usize = 14; // `beq.s d` opcode word
    const FAIL_AT: usize = 26; // `moveq #-1,d0`
    const NAME_AT: usize = 30; // "dos.library\0"

    // A 68000 PC-relative displacement counts from the extension word, which
    // sits immediately after the opcode word.
    let lea_disp = (NAME_AT - (LEA_AT + 2)) as u16;
    // A short branch counts from the word after the opcode word too.
    let beq_disp = (FAIL_AT - (BEQ_AT + 2)) as u8;

    let mut code = Vec::with_capacity(NAME_AT + DOS_NAME.len());

    // move.l 4.w,a6 — ExecBase
    code.extend_from_slice(&[0x2C, 0x78, 0x00, 0x04]);
    // lea dosName(pc),a1
    code.extend_from_slice(&[0x43, 0xFA]);
    code.extend_from_slice(&lea_disp.to_be_bytes());
    // jsr -96(a6) — FindResident
    code.extend_from_slice(&[0x4E, 0xAE]);
    code.extend_from_slice(&LVO_FIND_RESIDENT.to_be_bytes());
    // tst.l d0
    code.extend_from_slice(&[0x4A, 0x80]);
    // beq.s fail
    code.extend_from_slice(&[0x67, beq_disp]);
    // movea.l d0,a0
    code.extend_from_slice(&[0x20, 0x40]);
    // movea.l 22(a0),a0 — rt_Init
    code.extend_from_slice(&[0x20, 0x68]);
    code.extend_from_slice(&RT_INIT.to_be_bytes());
    // moveq #0,d0 — success
    code.extend_from_slice(&[0x70, 0x00]);
    // rts
    code.extend_from_slice(&[0x4E, 0x75]);
    // fail: moveq #-1,d0 — strap raises an alert and reboots
    code.extend_from_slice(&[0x70, 0xFF]);
    // rts
    code.extend_from_slice(&[0x4E, 0x75]);
    // dosName:
    code.extend_from_slice(DOS_NAME);

    debug_assert_eq!(code.len(), NAME_AT + DOS_NAME.len());
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout constants in `boot_code` are load-bearing: if an instruction
    /// changes size and they are not updated, the two displacements point at
    /// the wrong bytes and the disk hangs a real Amiga while every other test
    /// here still passes. This pins each landmark independently.
    #[test]
    fn the_layout_landmarks_are_where_the_displacements_assume() {
        let code = boot_code();

        // `lea d(pc),a1` at 4, `beq.s` at 14, `moveq #-1,d0` at 26.
        assert_eq!(&code[4..6], &[0x43, 0xFA], "lea is not at offset 4");
        assert_eq!(code[14], 0x67, "beq.s is not at offset 14");
        assert_eq!(&code[26..28], &[0x70, 0xFF], "fail is not at offset 26");
        assert_eq!(&code[30..], DOS_NAME, "the name is not at offset 30");
    }

    #[test]
    fn the_pc_relative_load_points_at_the_library_name() {
        let code = boot_code();
        let disp = u16::from_be_bytes([code[6], code[7]]) as usize;
        // The displacement counts from the extension word at offset 6.
        let target = 6 + disp;
        assert_eq!(
            &code[target..],
            DOS_NAME,
            "lea resolves to {target}, which is not the library name"
        );
    }

    #[test]
    fn the_failure_branch_lands_on_the_failure_path() {
        let code = boot_code();
        let disp = code[15] as usize;
        // A short branch counts from the word after the opcode word.
        let target = 16 + disp;
        assert_eq!(
            &code[target..target + 4],
            &[0x70, 0xFF, 0x4E, 0x75],
            "beq.s resolves to {target}, which is not `moveq #-1,d0; rts`"
        );
    }

    #[test]
    fn success_returns_zero_and_failure_returns_non_zero() {
        let code = boot_code();
        // strap treats a non-zero d0 as fatal: alert, then reboot. The
        // success path must therefore end `moveq #0,d0` / `rts`.
        assert_eq!(&code[22..26], &[0x70, 0x00, 0x4E, 0x75]);
        assert_eq!(&code[26..30], &[0x70, 0xFF, 0x4E, 0x75]);
    }

    #[test]
    fn find_resident_is_called_at_its_documented_offset() {
        let code = boot_code();
        assert_eq!(&code[8..10], &[0x4E, 0xAE], "not a jsr d(a6)");
        let lvo = i16::from_be_bytes([code[10], code[11]]);
        assert_eq!(lvo, -96, "exec/FindResident is at -96");
    }

    #[test]
    fn exec_base_comes_from_absolute_four_not_from_a6() {
        // `A6`'s contents at the boot entry point are convention, not
        // contract; address 4 is guaranteed.
        assert_eq!(&boot_code()[0..4], &[0x2C, 0x78, 0x00, 0x04]);
    }

    #[test]
    fn the_code_fits_in_the_boot_block_after_the_entry_point() {
        // Two 512-byte blocks are read, and execution starts at 12.
        assert!(BB_ENTRY + boot_code().len() <= 1024);
    }
}
