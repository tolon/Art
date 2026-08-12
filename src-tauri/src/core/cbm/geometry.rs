//! Where a Commodore disk image keeps its sectors.
//!
//! A `.d64` has no header: **the file is the sectors**, track 1 sector 0
//! first, laid end to end. So a track and a sector number become a byte offset
//! only through a table — the number of sectors per track changes as the head
//! moves inward, because the drive wrote more of them where the track is
//! longer.
//!
//! That table is the whole of this module, and it is the piece everything else
//! depends on: get one zone boundary wrong and every file past it reads
//! somebody else's bytes while nothing fails.
//!
//! ```text
//! 1541 / D64        tracks  1–17  21 sectors
//!                          18–24  19
//!                          25–30  18
//!                          31–35  17
//!                          36–40  17   (40-track images: SpeedDOS, DolphinDOS)
//!
//! 1571 / D71        the same 35-track layout, twice: side 2 is tracks 36–70
//!
//! 1581 / D81        80 tracks × 40 sectors, no zones at all
//! ```

use crate::core::error::{CoreError, CoreResult};

/// Bytes in one sector, on every Commodore disk ART reads.
pub const SECTOR_SIZE: usize = 256;

/// Which drive wrote the image, which is what decides the sector table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drive {
    /// 1541, single-sided.
    D64,
    /// 1571, double-sided — the 1541 layout twice over.
    D71,
    /// 1581, 3.5″ — no zones, 40 sectors on every track.
    D81,
}

/// A disk image's shape: which drive, how many tracks, and whether the file
/// carries the per-sector error bytes some copiers append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub drive: Drive,
    pub tracks: u8,
    /// True when the image has one error byte per sector appended after the
    /// sector data. ART does not interpret them; it only has to know they are
    /// there so they are not mistaken for a sector.
    pub error_bytes: bool,
}

/// Every image size ART accepts, and what each one means.
///
/// A size that is not in this table is **refused with the size in the
/// message** rather than guessed at: a `.d64` that is a few bytes short is
/// either truncated or not a `.d64`, and reading it as one would produce
/// confident nonsense.
const KNOWN: [(u64, Drive, u8, bool); 6] = [
    (174_848, Drive::D64, 35, false),
    (175_531, Drive::D64, 35, true),
    // SpeedDOS / DolphinDOS-era 40-track images, common in the wild.
    (196_608, Drive::D64, 40, false),
    (197_376, Drive::D64, 40, true),
    (349_696, Drive::D71, 70, false),
    (351_062, Drive::D71, 70, true),
];

/// The 1581's single size. Kept apart from [`KNOWN`] only because it has no
/// error-byte variant in circulation.
const D81_SIZE: u64 = 819_200;

impl Geometry {
    /// Work out the shape from the file's length alone.
    ///
    /// Length is all there is to go on: these formats have no header and no
    /// signature. What makes that safe is that the accepted sizes are exact
    /// and few — every one of them is a whole number of sectors for a real
    /// drive — and anything else is refused rather than rounded.
    pub fn from_len(len: u64) -> CoreResult<Self> {
        if len == D81_SIZE {
            return Ok(Self {
                drive: Drive::D81,
                tracks: 80,
                error_bytes: false,
            });
        }
        for (size, drive, tracks, error_bytes) in KNOWN {
            if len == size {
                return Ok(Self {
                    drive,
                    tracks,
                    error_bytes,
                });
            }
        }
        Err(CoreError::UnsupportedFormat(format!(
            "{len} bytes is not a Commodore disk image ART recognises. Known sizes: 174,848 and \
             175,531 (D64, 35 tracks), 196,608 and 197,376 (D64, 40 tracks), 349,696 and 351,062 \
             (D71), 819,200 (D81)"
        )))
    }

    /// How many sectors track `track` holds. Tracks are numbered from **1**.
    pub fn sectors_on(&self, track: u8) -> CoreResult<u8> {
        if track == 0 || track > self.tracks {
            return Err(CoreError::Malformed {
                format: "cbm".into(),
                detail: format!("track {track} is outside this image's 1..={}", self.tracks),
            });
        }
        Ok(match self.drive {
            Drive::D81 => 40,
            Drive::D64 => zone_sectors(track),
            // Side 2 repeats side 1's zones: track 36 is track 1 again.
            Drive::D71 => {
                let side1 = if track > 35 { track - 35 } else { track };
                zone_sectors(side1)
            }
        })
    }

    /// Sectors on every track before `track` — which is where that track's
    /// first sector begins, counted in sectors.
    fn sectors_before(&self, track: u8) -> CoreResult<u64> {
        let mut total = 0u64;
        for t in 1..track {
            total += self.sectors_on(t)? as u64;
        }
        Ok(total)
    }

    /// The byte offset of sector `sector` on track `track`.
    ///
    /// Error bytes, when the image has them, are all appended **after** the
    /// sector data rather than interleaved, so they do not move this — they
    /// only mean the file is longer than the sectors.
    pub fn offset_of(&self, track: u8, sector: u8) -> CoreResult<u64> {
        let count = self.sectors_on(track)?;
        if sector >= count {
            return Err(CoreError::Malformed {
                format: "cbm".into(),
                detail: format!("track {track} has {count} sectors; {sector} is not one of them"),
            });
        }
        let index = self.sectors_before(track)? + sector as u64;
        Ok(index * SECTOR_SIZE as u64)
    }

    /// Total sectors on the disk — the length of the data area, in sectors.
    pub fn total_sectors(&self) -> CoreResult<u64> {
        self.sectors_before(self.tracks)
            .map(|before| before + self.sectors_on(self.tracks).unwrap_or(0) as u64)
    }

    /// The track the directory lives on. 18 for the 1541 and 1571, 40 for the
    /// 1581.
    pub fn directory_track(&self) -> u8 {
        match self.drive {
            Drive::D64 | Drive::D71 => 18,
            Drive::D81 => 40,
        }
    }

    /// The sector the directory's first block sits in.
    ///
    /// On a 1541/1571 the BAM is at 18/0 and the directory follows at 18/1.
    /// On a 1581 the header is at 40/0, two BAM sectors follow, and the
    /// directory starts at 40/3.
    pub fn directory_sector(&self) -> u8 {
        match self.drive {
            Drive::D64 | Drive::D71 => 1,
            Drive::D81 => 3,
        }
    }
}

/// The 1541's four zones. Tracks 36–40 continue the innermost one.
fn zone_sectors(track: u8) -> u8 {
    match track {
        1..=17 => 21,
        18..=24 => 19,
        25..=30 => 18,
        _ => 17,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d64(tracks: u8) -> Geometry {
        Geometry {
            drive: Drive::D64,
            tracks,
            error_bytes: false,
        }
    }

    /// Every zone boundary, from both sides. A table that is right in the
    /// middle of a zone and wrong at its edge reads the wrong sector for every
    /// file past it, and nothing about that failure looks like a failure.
    #[test]
    fn the_sector_count_changes_at_exactly_the_documented_tracks() {
        let g = d64(40);
        for (track, expected) in [
            (1, 21),
            (17, 21),
            (18, 19), // zone 1 → 2
            (24, 19),
            (25, 18), // zone 2 → 3
            (30, 18),
            (31, 17), // zone 3 → 4
            (35, 17),
            (36, 17), // the 40-track continuation (amendment A3)
            (40, 17),
        ] {
            assert_eq!(g.sectors_on(track).unwrap(), expected, "track {track}");
        }
    }

    /// The offsets those counts produce, checked against the arithmetic
    /// independently: track 18 starts after 17 tracks of 21 sectors.
    #[test]
    fn a_track_starts_where_the_tracks_before_it_end() {
        let g = d64(35);
        assert_eq!(g.offset_of(1, 0).unwrap(), 0);
        assert_eq!(g.offset_of(1, 20).unwrap(), 20 * 256);
        assert_eq!(g.offset_of(2, 0).unwrap(), 21 * 256);
        // 17 × 21 = 357 sectors before track 18.
        assert_eq!(g.offset_of(18, 0).unwrap(), 357 * 256);
        // …and the directory sector is the next one along.
        assert_eq!(g.offset_of(18, 1).unwrap(), 358 * 256);
    }

    /// The whole 35-track disk is exactly 683 sectors — the number every 1541
    /// reference gives, reached here by summing the table rather than by
    /// being told.
    #[test]
    fn the_sector_total_matches_the_documented_disk() {
        assert_eq!(d64(35).total_sectors().unwrap(), 683);
        assert_eq!(683 * SECTOR_SIZE as u64, 174_848);
        assert_eq!(d64(40).total_sectors().unwrap(), 768);
        assert_eq!(768 * SECTOR_SIZE as u64, 196_608);
    }

    /// A 1571 is a 1541 twice: track 36 is side 2's track 1.
    #[test]
    fn the_second_side_of_a_1571_repeats_the_first_sides_zones() {
        let g = Geometry {
            drive: Drive::D71,
            tracks: 70,
            error_bytes: false,
        };
        assert_eq!(g.sectors_on(1).unwrap(), 21);
        assert_eq!(g.sectors_on(36).unwrap(), 21, "side 2 track 1");
        assert_eq!(g.sectors_on(53).unwrap(), 19, "side 2 track 18");
        assert_eq!(g.sectors_on(70).unwrap(), 17);
        assert_eq!(g.total_sectors().unwrap(), 1366, "683 twice");
    }

    #[test]
    fn a_1581_has_no_zones() {
        let g = Geometry {
            drive: Drive::D81,
            tracks: 80,
            error_bytes: false,
        };
        for track in [1u8, 40, 80] {
            assert_eq!(g.sectors_on(track).unwrap(), 40);
        }
        assert_eq!(g.total_sectors().unwrap(), 3200);
        assert_eq!(3200 * SECTOR_SIZE as u64, 819_200);
        assert_eq!(g.directory_track(), 40);
        assert_eq!(g.directory_sector(), 3);
    }

    /// The four D64 sizes amendment A3 names, and the two D71 ones.
    #[test]
    fn every_accepted_size_is_recognised_for_what_it_is() {
        for (len, drive, tracks, errors) in [
            (174_848u64, Drive::D64, 35u8, false),
            (175_531, Drive::D64, 35, true),
            (196_608, Drive::D64, 40, false),
            (197_376, Drive::D64, 40, true),
            (349_696, Drive::D71, 70, false),
            (351_062, Drive::D71, 70, true),
            (819_200, Drive::D81, 80, false),
        ] {
            let g = Geometry::from_len(len).unwrap();
            assert_eq!(g.drive, drive, "{len}");
            assert_eq!(g.tracks, tracks, "{len}");
            assert_eq!(g.error_bytes, errors, "{len}");
        }
    }

    /// Any other size is refused, and the message carries the size — never a
    /// guess at what it might have been.
    #[test]
    fn an_unknown_size_is_refused_with_the_size_in_the_message() {
        for len in [0u64, 1, 174_847, 174_849, 200_000, 819_199] {
            let err = Geometry::from_len(len).unwrap_err();
            assert_eq!(err.code(), "ART-FORMAT-UNSUPPORTED", "{len}");
            assert!(err.to_string().contains(&len.to_string()), "{len}: {err}");
        }
    }

    /// A track or sector outside the geometry is an error, never an index —
    /// `panic = "abort"` in release turns an out-of-range index into a dead
    /// application, and every one of these numbers comes from the image.
    #[test]
    fn a_track_or_sector_outside_the_disk_is_an_error() {
        let g = d64(35);
        assert!(g.sectors_on(0).is_err());
        assert!(g.sectors_on(36).is_err(), "past a 35-track disk");
        assert!(g.offset_of(1, 21).is_err(), "track 1 has 21 sectors, 0..20");
        assert!(g.offset_of(18, 19).is_err());
        assert!(g.offset_of(255, 255).is_err());
    }
}
