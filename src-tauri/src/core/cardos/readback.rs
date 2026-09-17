//! What a built card's partitions hold, read back off the image (card round
//! 3, I4).
//!
//! **The manifest records what is on the card, not an estimate.** Before this
//! module the manifest's `files` and `bytes` came from measuring the sources
//! before the build — System's tree measured before WHDLoad, its prefs, the
//! Kickstarts and their `.RTB` files were written into it, and every
//! partition's `bytes` a block-rounded PFS3 estimate. Task 13's card said 3
//! files on System where hst-imager counted 7. Counting the finished
//! partition through libpfs3's own reader is the same answer whichever tool
//! wrote it — hst-imager's own copy reports no byte count at all (ART-125).

use std::path::Path;

use crate::core::card::CardImage;
use crate::core::cardos::kickstarts::MAX_WALK_NODES;
use crate::core::error::{CoreError, CoreResult};
use crate::core::preload::native::{from_pfs3, partition_region};

/// Files and bytes on one PFS3 partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionCount {
    pub files: u64,
    pub bytes: u64,
}

/// Count every file on PFS3 partition `index` of the card's Amiga area
/// `area`, and sum their sizes, reading only directories. The walk is bounded
/// by [`MAX_WALK_NODES`].
pub fn count_pfs3_partition(
    image: &Path,
    card: &CardImage,
    area: usize,
    index: usize,
) -> CoreResult<PartitionCount> {
    let missing = || CoreError::Malformed {
        format: "card".into(),
        detail: format!("the card has no partition {index} in Amiga area {area}"),
    };
    let amiga = card.areas.get(area).ok_or_else(missing)?;
    let part = amiga.rdb.partitions.get(index).ok_or_else(missing)?;
    let (offset, _, _) = partition_region(amiga, part)?;
    let mut volume = libpfs3::volume::Volume::open(image, offset).map_err(from_pfs3)?;

    let mut count = PartitionCount { files: 0, bytes: 0 };
    let mut stack = vec![String::new()];
    let mut nodes = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in volume.list_dir(&dir).map_err(from_pfs3)? {
            nodes += 1;
            if nodes > MAX_WALK_NODES {
                return Err(CoreError::LimitExceeded {
                    subject: "card partition".into(),
                    detail: format!(
                        "'{}' holds more than {MAX_WALK_NODES} files and folders",
                        part.drive_name
                    ),
                });
            }
            if entry.is_dir() {
                stack.push(if dir.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{dir}/{}", entry.name)
                });
            } else {
                count.files += 1;
                count.bytes += entry.file_size();
            }
        }
    }
    Ok(count)
}
