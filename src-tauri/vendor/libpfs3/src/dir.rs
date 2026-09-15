//! Directory block reading and entry iteration.
//!
//! Directories are stored as chains of dirblocks (id='DB').
//! Each dirblock contains packed variable-length direntries.
//!
//! Modified by ART on 2026-09-15 (the scoped re-review's follow-ups 1 and 6,
//! ART-324 and ART-330): an entry's `fsizex` is part of its size only on a
//! largefile volume; a walk that meets a malformed entry is refused
//! `Error::DamagedDirectory`, naming the directory, the block and the offset;
//! and a lookup stops at its match, as pfs3aio's `SearchInDir` does. 0.1.3
//! ended a block's walk at a malformed entry silently, so a name behind it was
//! "not found" and a listing came back short. `ART-PATCH.md` in this crate's
//! root says what and why.

use crate::anode::AnodeReader;
use crate::cache::BlockCache;
use crate::error::{Error, Result};
use crate::io::BlockDevice;
use crate::ondisk::*;
use crate::util;

/// ART (follow-up 6): how a refusal names directory `dir_anode` when only its
/// anode is known.
fn anode_phrase(dir_anode: u32) -> String {
    if dir_anode == ANODE_ROOTDIR {
        "the root directory".to_string()
    } else {
        format!("the directory at anode {dir_anode}")
    }
}

/// ART (follow-up 6): how a refusal names the directory at `path`.
pub(crate) fn path_phrase(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        "the root directory".to_string()
    } else {
        format!("the directory '{}'", parts.join("/"))
    }
}

/// ART (follow-ups 1 and 6): the entries of directory `dir_anode`, in order,
/// each handed to `visit` until it answers `true`; that entry is returned,
/// and nothing after it is read — pfs3aio's `SearchInDir` stops at its match
/// the same way (`directory.c:729-740`, `tonioni/pfs3aio` `211f7f0`).
/// `Ok(None)` when every block was walked to its end. A malformed entry met
/// first is `Error::DamagedDirectory` naming `dir`: what lies behind it
/// cannot be read, so the walk can say neither "found" nor "not there".
#[allow(clippy::too_many_arguments)]
fn walk_entries(
    dir_anode: u32,
    dir: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
    visit: &mut dyn FnMut(&DirEntry) -> bool,
) -> Result<Option<DirEntry>> {
    let chain = anodes.get_chain(dir_anode, dev, cache)?;
    for an in &chain {
        for i in 0..an.clustersize {
            let blk = an.blocknr as u64 + i as u64;
            let data = cache.read_reserved(dev, blk, reserved_blksize)?;
            if data.len() < DIR_BLOCK_HEADER_SIZE + 1 {
                continue;
            }
            let id = u16::from_be_bytes(data[0..2].try_into().unwrap());
            if id != DBLKID {
                continue;
            }
            let mut offset = DIR_BLOCK_HEADER_SIZE;
            loop {
                match DirEntry::parse(data, offset, largefile) {
                    Ok(Some((entry, next))) => {
                        if visit(&entry) {
                            return Ok(Some(entry));
                        }
                        offset = next;
                    }
                    Ok(None) => break,
                    Err(m) => {
                        return Err(Error::DamagedDirectory {
                            dir: dir.to_string(),
                            block: blk,
                            offset: m.offset,
                            size: m.size,
                        });
                    }
                }
            }
        }
    }
    Ok(None)
}

/// List all entries in a directory given its anode number.
///
/// ART (2026-09-15, the scoped re-review's follow-ups 1 and 6): `largefile` is
/// the volume's `Rootblock::has_largefile`. A malformed entry is
/// `Error::DamagedDirectory`, never a shorter list.
pub fn list_entries(
    dir_anode: u32,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<Vec<DirEntry>> {
    list_entries_named(
        dir_anode,
        &anode_phrase(dir_anode),
        anodes,
        dev,
        cache,
        reserved_blksize,
        largefile,
    )
}

/// ART (follow-up 6): `list_entries`, its refusal naming the directory `dir`.
pub(crate) fn list_entries_named(
    dir_anode: u32,
    dir: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    walk_entries(
        dir_anode,
        dir,
        anodes,
        dev,
        cache,
        reserved_blksize,
        largefile,
        &mut |entry| {
            entries.push(entry.clone());
            false
        },
    )?;
    Ok(entries)
}

/// Look up a single name in a directory (case-insensitive).
///
/// ART (2026-09-15, the scoped re-review's follow-up 6): `Error::NotFound`
/// only when the whole directory was read; a malformed entry met before the
/// name is `Error::DamagedDirectory`.
pub fn lookup(
    dir_anode: u32,
    name: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<DirEntry> {
    lookup_named(
        dir_anode,
        &anode_phrase(dir_anode),
        name,
        anodes,
        dev,
        cache,
        reserved_blksize,
        largefile,
    )
}

/// ART (follow-up 6): `lookup`, its refusal naming the directory `dir`.
#[allow(clippy::too_many_arguments)]
fn lookup_named(
    dir_anode: u32,
    dir: &str,
    name: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<DirEntry> {
    walk_entries(
        dir_anode,
        dir,
        anodes,
        dev,
        cache,
        reserved_blksize,
        largefile,
        &mut |entry| util::name_eq_ci(&entry.name, name),
    )?
    .ok_or_else(|| Error::NotFound(name.to_string()))
}

/// Resolve a '/'-separated path to a DirEntry.
/// Returns `Ok(None)` for the root directory or if the final component doesn't exist.
/// Returns `Err(NotFound)` if an intermediate directory doesn't exist.
/// Returns `Err(NotADirectory)` if an intermediate component is a file.
///
/// ART (follow-up 6): `Err(DamagedDirectory)` when a directory on the way
/// stops at a malformed entry before the component could be found.
pub fn resolve_path(
    path: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<Option<DirEntry>> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Ok(None); // root
    }

    let mut dir_anode = ANODE_ROOTDIR;
    for (i, part) in parts.iter().enumerate() {
        let dir = path_phrase(&parts[..i].join("/"));
        let result = lookup_named(
            dir_anode,
            &dir,
            part,
            anodes,
            dev,
            cache,
            reserved_blksize,
            largefile,
        );
        if i < parts.len() - 1 {
            // Intermediate component must exist and be a directory
            let entry = result?;
            if !entry.is_dir() {
                return Err(Error::NotADirectory);
            }
            dir_anode = entry.anode;
        } else {
            // Final component: NotFound → Ok(None)
            match result {
                Ok(entry) => return Ok(Some(entry)),
                Err(Error::NotFound(_)) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }
    Ok(None)
}

/// Resolve a path to a directory anode number.
///
/// ART (follow-up 6): `Err(DamagedDirectory)` as `resolve_path` says.
pub fn resolve_dir_path(
    path: &str,
    anodes: &AnodeReader,
    dev: &dyn BlockDevice,
    cache: &mut BlockCache,
    reserved_blksize: u16,
    largefile: bool,
) -> Result<u32> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Ok(ANODE_ROOTDIR);
    }

    let mut dir_anode = ANODE_ROOTDIR;
    for (i, part) in parts.iter().enumerate() {
        let dir = path_phrase(&parts[..i].join("/"));
        let entry = lookup_named(
            dir_anode,
            &dir,
            part,
            anodes,
            dev,
            cache,
            reserved_blksize,
            largefile,
        )?;
        if !entry.is_dir() {
            return Err(Error::NotADirectory);
        }
        dir_anode = entry.anode;
    }
    Ok(dir_anode)
}
