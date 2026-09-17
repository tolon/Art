//! Free space on the host volume, for the card build's own preflight check
//! (`core/cardos/prepare.rs`, design § 6).
//!
//! `core/` never touches a Windows API (CLAUDE.md's core-independence rule),
//! so this is where the question is actually asked: `GetDiskFreeSpaceExW`,
//! same shape as `tools/hst_imager.rs` and `tools/recycle_bin.rs`.

use std::path::Path;

/// The bytes the calling user may write on the volume holding `path`
/// (`lpFreeBytesAvailableToCaller`, which differs from the volume's raw free
/// space under per-user disk quotas). `path` does not have to exist yet —
/// the nearest existing ancestor is asked instead, since a build checks free
/// space before it creates the file it is about to write.
pub fn available_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut at = path;
    while !at.exists() {
        at = at.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no part of '{}' exists", path.display()),
            )
        })?;
    }

    let wide: Vec<u16> = at
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    // SAFETY: `wide` is NUL-terminated and outlives the call (it is not
    // dropped until after `GetDiskFreeSpaceExW` returns); the two null
    // out-pointers (`lpTotalNumberOfBytes`, `lpTotalNumberOfFreeBytes`) are
    // documented as optional by the Win32 API.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scratch_volume_reports_some_free_bytes() {
        let dir = std::env::temp_dir(); // TMP is E: by .cargo/config.toml
        assert!(available_bytes(&dir).unwrap() > 0);
    }

    #[test]
    fn a_path_that_does_not_exist_yet_is_answered_for_its_nearest_existing_folder() {
        // `dir` itself exists (the guard creates it); `deeper` does not, so
        // this still exercises the nearest-existing-ancestor walk without
        // building a scratch path by hand (`scripts/scratch-guard-sweep.py`).
        let (_guard, dir) = crate::core::ScratchDir::pair("art-free-space", "not-there");
        let deeper = dir.join("deeper");
        assert!(available_bytes(&deeper).unwrap() > 0);
    }

    // R8 (controller ruling): the brief's third test
    // (`it_agrees_with_what_windows_reports_for_the_same_volume`) asserts
    // nothing — two calls a moment apart on the same folder is not a check
    // against Windows, just against itself. It is deliberately not written;
    // the outside check is PowerShell `(Get-PSDrive E).Free` compared by
    // hand against the `#[ignore]`d test below, within 1 GiB (see the task
    // report for both numbers). M11 (return `total` instead of `free`)
    // survives the two unit tests above — both are still `> 0` — and is
    // caught only by that outside check. Disclosed survivor.

    #[test]
    #[ignore]
    fn prints_available_bytes_for_e_drive_for_the_outside_check() {
        let bytes = available_bytes(Path::new("E:\\")).unwrap();
        println!("available_bytes(E:\\) = {bytes}");
    }
}
