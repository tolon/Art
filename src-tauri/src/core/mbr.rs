//! The MBR partition table — enough of it to find the Amiga on a PiStorm card.
//!
//! **Why ART needs this at all.** A PiStorm card is not an Amiga disk with an
//! Amiga disk's layout. It is an MBR-partitioned card: a FAT32 partition the
//! Raspberry Pi firmware boots from, then one or more areas the Amiga sees as
//! its own disks, each starting with its own `RDSK`. ART looked for that
//! `RDSK` in the first sixteen blocks of the file and therefore could not open
//! a real card at all (ART-095).
//!
//! Read-only, and deliberately shallow: this parses the four primary entries
//! and nothing else. No extended partitions, no GPT, no logical drives — a
//! PiStorm card has never needed any of them, and a parser that handles cases
//! nobody has is a parser with untested branches in it.

use serde::{Deserialize, Serialize};

/// Where the partition table starts inside the first sector.
const TABLE_OFFSET: usize = 446;

/// Four primary entries, sixteen bytes each.
const ENTRY_COUNT: usize = 4;
const ENTRY_BYTES: usize = 16;

/// The signature the last two bytes of a valid MBR carry.
const SIGNATURE: [u8; 2] = [0x55, 0xAA];

/// A sector, as every card ART meets defines it.
pub const SECTOR_BYTES: u64 = 512;

/// The partition types ART has an opinion about.
///
/// Everything else is carried through as its raw byte. ART is not a partition
/// manager; it needs to know which area is the Amiga's and which is the boot
/// partition, and to leave the rest alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "code")]
pub enum PartitionKind {
    /// `0x0B` / `0x0C` — the FAT32 partition the Pi firmware boots from.
    Fat32,
    /// `0x76`. What both real PiStorm distributions use for an Amiga area, and
    /// what the Amiga finds its `RDSK` at the start of.
    ///
    /// Not an officially assigned type; it is a convention, which is exactly
    /// why ART recognises it by number and does not pretend to know more.
    AmigaRdb,
    /// Anything else, carried rather than interpreted.
    Other(u8),
}

impl PartitionKind {
    fn from_byte(code: u8) -> Self {
        match code {
            0x0B | 0x0C => Self::Fat32,
            0x76 => Self::AmigaRdb,
            other => Self::Other(other),
        }
    }
}

/// One primary partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MbrPartition {
    /// Which of the four slots it occupies. Kept because a card's own
    /// documentation talks about "partition 1", and a listing that renumbers
    /// them is a listing that disagrees with the user's notes.
    pub index: usize,
    pub kind: PartitionKind,
    /// The raw type byte, whatever `kind` made of it.
    pub type_byte: u8,
    pub bootable: bool,
    pub start_lba: u64,
    pub sector_count: u64,
}

impl MbrPartition {
    /// Where this partition begins, in bytes.
    pub fn start_bytes(&self) -> u64 {
        self.start_lba * SECTOR_BYTES
    }

    /// How long it is, in bytes.
    pub fn length_bytes(&self) -> u64 {
        self.sector_count * SECTOR_BYTES
    }
}

/// What the first sector says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mbr {
    pub partitions: Vec<MbrPartition>,
}

impl Mbr {
    /// The Amiga areas, in the order the table lists them.
    ///
    /// Several is normal: MultibootOS 2.2 has two, each with its own RDB, its
    /// own geometry and its own partitions (ART-097).
    pub fn amiga_areas(&self) -> Vec<MbrPartition> {
        self.partitions
            .iter()
            .copied()
            .filter(|p| p.kind == PartitionKind::AmigaRdb)
            .collect()
    }

    /// The FAT32 boot partition, if there is one.
    pub fn boot_partition(&self) -> Option<MbrPartition> {
        self.partitions
            .iter()
            .copied()
            .find(|p| p.kind == PartitionKind::Fat32)
    }
}

/// Read the partition table out of a card's first sector.
///
/// `None` when this is not an MBR at all — which is the ordinary answer for a
/// plain HDF, and not an error. The caller falls back to treating the file as
/// one Amiga disk starting at byte zero, which is what an HDF is.
///
/// `sector` may be longer than 512 bytes; only the first sector is read.
pub fn parse_mbr(sector: &[u8]) -> Option<Mbr> {
    if sector.len() < SECTOR_BYTES as usize {
        return None;
    }
    if sector[510..512] != SIGNATURE {
        return None;
    }

    let mut partitions = Vec::new();
    for index in 0..ENTRY_COUNT {
        let at = TABLE_OFFSET + index * ENTRY_BYTES;
        let entry = &sector[at..at + ENTRY_BYTES];

        let type_byte = entry[4];
        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
        let sector_count = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;

        // An empty slot is type 0 with no sectors. A type of 0 *with* sectors
        // is a table somebody has damaged, and skipping it is the same answer.
        if type_byte == 0 || sector_count == 0 {
            continue;
        }

        partitions.push(MbrPartition {
            index,
            kind: PartitionKind::from_byte(type_byte),
            type_byte,
            bootable: entry[0] == 0x80,
            start_lba,
            sector_count,
        });
    }

    // A signature and four empty slots is not a partition table worth
    // reporting — a zeroed sector ending in 55 AA is a coincidence an Amiga
    // image could produce.
    (!partitions.is_empty()).then_some(Mbr { partitions })
}

/// Where the Amiga disks on this card start, in bytes.
///
/// The whole point of the module in one function, and it answers for a plain
/// HDF too: no MBR means one Amiga disk beginning at zero, which is exactly
/// what an HDF is. So a caller can use this unconditionally rather than
/// branching on what kind of file it has.
pub fn amiga_bases(first_sector: &[u8]) -> Vec<u64> {
    match parse_mbr(first_sector) {
        Some(mbr) => {
            let areas = mbr.amiga_areas();
            if areas.is_empty() {
                // An MBR with no `0x76` area: possibly somebody's own layout,
                // possibly a card ART does not understand. Offering byte zero
                // would be offering the MBR itself as an Amiga disk.
                Vec::new()
            } else {
                areas.iter().map(|p| p.start_bytes()).collect()
            }
        }
        None => vec![0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A first sector with the given entries, built the way a real one is.
    fn sector(entries: &[(u8, u8, u32, u32)]) -> Vec<u8> {
        let mut out = vec![0u8; 512];
        for (slot, (status, ptype, lba, count)) in entries.iter().enumerate() {
            let at = TABLE_OFFSET + slot * ENTRY_BYTES;
            out[at] = *status;
            out[at + 4] = *ptype;
            out[at + 8..at + 12].copy_from_slice(&lba.to_le_bytes());
            out[at + 12..at + 16].copy_from_slice(&count.to_le_bytes());
        }
        out[510..512].copy_from_slice(&SIGNATURE);
        out
    }

    /// The layout both real distributions ship, measured 2026-08-13:
    /// a 1.10 GiB FAT32 at LBA 2048, then the Amiga area at LBA 2301952.
    fn real_card() -> Vec<u8> {
        sector(&[
            (0x00, 0x0C, 2048, 2_299_904),
            (0x00, 0x76, 2_301_952, 118_235_136),
        ])
    }

    #[test]
    fn a_real_pistorm_card_is_read_the_way_it_is_laid_out() {
        let mbr = parse_mbr(&real_card()).expect("a real card has an MBR");
        assert_eq!(mbr.partitions.len(), 2);

        let boot = mbr.boot_partition().expect("a FAT32 boot partition");
        assert_eq!(boot.start_bytes(), 1_048_576);
        assert_eq!(boot.kind, PartitionKind::Fat32);

        let areas = mbr.amiga_areas();
        assert_eq!(areas.len(), 1);
        // The number ART could not find: the RDB is here, not at byte 0.
        assert_eq!(areas[0].start_bytes(), 1_178_599_424);
    }

    #[test]
    fn a_card_with_two_amiga_areas_reports_both() {
        // MultibootOS 2.2. Reporting only the first is ART-097.
        let card = sector(&[
            (0x00, 0x0C, 2048, 2_299_904),
            (0x00, 0x76, 2_301_952, 96_468_992),
            (0x00, 0x76, 98_770_944, 138_731_104),
        ]);
        assert_eq!(
            amiga_bases(&card),
            vec![1_178_599_424, 50_570_723_328],
            "a card may carry several Amiga disks"
        );
    }

    #[test]
    fn a_plain_hdf_is_one_amiga_disk_starting_at_zero() {
        // No MBR, so the caller can ask this unconditionally rather than
        // branching on what kind of file it has.
        assert_eq!(amiga_bases(&[0u8; 512]), vec![0]);
        assert_eq!(amiga_bases(b"RDSK"), vec![0], "too short to be an MBR");
    }

    #[test]
    fn a_zeroed_sector_that_happens_to_end_in_the_signature_is_not_a_table() {
        // An Amiga image can produce those two bytes by coincidence, and
        // reading it as a partitioned card would hide the disk inside it.
        let mut sector = vec![0u8; 512];
        sector[510..512].copy_from_slice(&SIGNATURE);
        assert_eq!(parse_mbr(&sector), None);
        assert_eq!(amiga_bases(&sector), vec![0]);
    }

    #[test]
    fn an_mbr_with_no_amiga_area_offers_nothing_rather_than_byte_zero() {
        // Somebody's own layout, or a card ART does not understand. Offering
        // byte zero would be offering the MBR itself as an Amiga disk.
        let card = sector(&[(0x80, 0x07, 2048, 1000)]);
        assert!(parse_mbr(&card).is_some());
        assert!(amiga_bases(&card).is_empty());
    }

    #[test]
    fn empty_slots_are_skipped_and_the_rest_keep_their_numbers() {
        // A card's own documentation talks about "partition 2"; a listing that
        // renumbers them disagrees with the user's notes.
        let card = sector(&[
            (0x00, 0x00, 0, 0),
            (0x00, 0x0C, 2048, 1000),
            (0x00, 0x00, 0, 0),
            (0x00, 0x76, 4096, 2000),
        ]);
        let mbr = parse_mbr(&card).unwrap();
        assert_eq!(mbr.partitions.len(), 2);
        assert_eq!(mbr.partitions[0].index, 1);
        assert_eq!(mbr.partitions[1].index, 3);
    }

    #[test]
    fn a_slot_claiming_a_type_but_no_sectors_is_ignored() {
        let card = sector(&[(0x00, 0x76, 2048, 0), (0x00, 0x0C, 4096, 1000)]);
        let mbr = parse_mbr(&card).unwrap();
        assert_eq!(mbr.partitions.len(), 1);
        assert_eq!(mbr.partitions[0].kind, PartitionKind::Fat32);
    }

    #[test]
    fn the_type_byte_is_carried_even_when_art_has_no_name_for_it() {
        let card = sector(&[(0x00, 0x83, 2048, 1000)]);
        let mbr = parse_mbr(&card).unwrap();
        assert_eq!(mbr.partitions[0].kind, PartitionKind::Other(0x83));
        assert_eq!(mbr.partitions[0].type_byte, 0x83);
    }

    #[test]
    fn a_partition_past_four_gigabytes_is_measured_without_overflowing() {
        // MultibootOS's second area starts at LBA 98 770 944 and runs 66 GiB.
        // The fields are 32-bit sector counts; the arithmetic must not be.
        let card = sector(&[(0x00, 0x76, 98_770_944, 138_731_104)]);
        let area = parse_mbr(&card).unwrap().amiga_areas()[0];
        assert_eq!(area.start_bytes(), 50_570_723_328);
        assert_eq!(area.length_bytes(), 71_030_325_248);
    }

    #[test]
    fn a_sector_shorter_than_a_sector_is_not_read() {
        for len in [0usize, 1, 445, 511] {
            assert_eq!(parse_mbr(&vec![0u8; len]), None, "{len}");
        }
    }
}
