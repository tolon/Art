//! The MBR partition table — enough of it to find the Amiga on a PiStorm card.
//!
//! **Why ART needs this at all.** A PiStorm card is not an Amiga disk with an
//! Amiga disk's layout. It is an MBR-partitioned card: a FAT32 partition the
//! Raspberry Pi firmware boots from, then one or more areas the Amiga sees as
//! its own disks, each starting with its own `RDSK`. ART looked for that
//! `RDSK` in the first sixteen blocks of the file and therefore could not open
//! a real card at all (ART-095).
//!
//! Deliberately shallow: four primary entries and nothing else. No extended
//! partitions, no GPT, no logical drives — a PiStorm card has never needed any
//! of them, and a parser that handles cases nobody has is a parser with
//! untested branches in it.
//!
//! Since SD-1's G2 it also **writes** one: [`plan_card`] decides a card's
//! shape and [`write_mbr`] serialises it. Both are built on the two real cards
//! read in `docs/sd2-card-layout.md` rather than on the specification alone —
//! see [`plan_card`] for what those cards settled and what they left open.

use serde::{Deserialize, Serialize};

use crate::core::error::{CoreError, CoreResult};

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

// ---------------------------------------------------------------------------
// Writing one (SD-1 · G2)
// ---------------------------------------------------------------------------

/// Where the first partition starts, in sectors — 1 MiB in.
///
/// Both real cards use exactly this, and so does every card any modern tool
/// writes: it is the alignment flash memory wants and the one Windows, Linux
/// and the Pi's own imager all produce.
pub const FIRST_PARTITION_LBA: u64 = 2048;

/// How big the FAT32 boot partition is by default, in sectors.
///
/// **1.10 GiB, measured rather than assumed.** The SD-0 research said "~200
/// MB"; both real cards say 2 299 904 sectors, to the sector, and they are the
/// two cards that boot. A gigabyte is not extravagance — it is several
/// Kickstarts, more than one Emu68 release and a `config_*.txt` per
/// distribution, which is what a multiboot card actually holds.
pub const DEFAULT_BOOT_SECTORS: u64 = 2_299_904;

/// Areas start on a 4 MiB boundary.
///
/// Both real cards' Amiga areas land on one — MultibootOS's first at 281 × 4
/// MiB — and it is the erase-block size flash of this size is built around, so
/// a partition that straddles one costs writes for the life of the card.
const AREA_ALIGN_SECTORS: u64 = 4 * 1024 * 1024 / SECTOR_BYTES;

/// The most Amiga disks a card can carry: four primaries, one spent on FAT32.
pub const MAX_AREAS: usize = 3;

/// A card's shape, before any of it exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardLayout {
    pub total_sectors: u64,
    /// The FAT32 partition the Pi firmware boots from. Always first, and
    /// always present — see [`plan_card`].
    pub boot: MbrPartition,
    /// One to three Amiga disks, each of which gets its own RDB.
    pub areas: Vec<MbrPartition>,
}

impl CardLayout {
    /// Every partition, in slot order.
    pub fn partitions(&self) -> Vec<MbrPartition> {
        let mut all = vec![self.boot];
        all.extend(self.areas.iter().copied());
        all
    }
}

/// Decide a card's shape: FAT32 first, then one to three Amiga areas.
///
/// `area_shares` gives each Amiga area's size in bytes, and the **last** of
/// them may be `0` for "everything that is left" — which is what both real
/// cards do with their final area, and what a one-distro card wants for its
/// only one. Only the last: a `0` anywhere else would be asking the remainder
/// to be divided by a rule nobody has stated.
///
/// Four rules are enforced here rather than left to a caller, because each of
/// them is a card that does not boot:
///
/// - **The FAT32 partition comes first, at sector 2048.** That is what the Pi
///   firmware looks for, and it is also what keeps byte zero of the card out
///   of Amiga hands: unit 0 is the *whole card, MBR included*, and an Amiga
///   tool let loose on it would take the partition table with it. SD-0 asks
///   that ART make such a layout impossible to generate, and the way to make
///   it impossible is to have no way to express it.
/// - **At most three Amiga areas.** Four primaries, one spent on the boot
///   partition. No extended partitions: the m68k side reads primaries.
/// - **Areas are 4 MiB aligned** and contiguous, which is what both real cards
///   are.
/// - **Nothing may claim space the card does not have**, and a card too small
///   for the boot partition plus one area is refused with both numbers rather
///   than silently shrunk.
pub fn plan_card(total_bytes: u64, boot_bytes: u64, area_shares: &[u64]) -> CoreResult<CardLayout> {
    if area_shares.is_empty() || area_shares.len() > MAX_AREAS {
        return Err(CoreError::InvalidInput(format!(
            "a card carries one to {MAX_AREAS} Amiga disks, not {}",
            area_shares.len()
        )));
    }
    if area_shares[..area_shares.len() - 1].contains(&0) {
        return Err(CoreError::InvalidInput(
            "only the last Amiga disk can take whatever is left; say how big the others are".into(),
        ));
    }

    let total_sectors = total_bytes / SECTOR_BYTES;
    let boot_sectors = if boot_bytes == 0 {
        DEFAULT_BOOT_SECTORS
    } else {
        boot_bytes.div_ceil(SECTOR_BYTES)
    };

    let boot = MbrPartition {
        index: 0,
        kind: PartitionKind::Fat32,
        type_byte: 0x0C,
        // MultibootOS marks it bootable and CaffeineOS does not, and both
        // cards boot — so this is not load-bearing. Set, because "the Pi boots
        // from this one" is true and a table that says so reads better in
        // every other tool.
        bootable: true,
        start_lba: FIRST_PARTITION_LBA,
        sector_count: boot_sectors,
    };

    let mut next = align_up(FIRST_PARTITION_LBA + boot_sectors, AREA_ALIGN_SECTORS);
    if next >= total_sectors {
        return Err(too_small(total_sectors, next));
    }

    let mut areas = Vec::with_capacity(area_shares.len());
    for (position, &share) in area_shares.iter().enumerate() {
        let remaining = total_sectors.saturating_sub(next);
        let sectors = if share == 0 {
            remaining
        } else {
            share.div_ceil(SECTOR_BYTES)
        };

        if sectors == 0 || sectors > remaining {
            return Err(too_small(total_sectors, next + sectors));
        }

        areas.push(MbrPartition {
            index: position + 1,
            kind: PartitionKind::AmigaRdb,
            type_byte: 0x76,
            bootable: false,
            start_lba: next,
            sector_count: sectors,
        });

        // The next area starts on the alignment boundary at or after this
        // one's end, so an area never begins in the middle of an erase block.
        next = align_up(next + sectors, AREA_ALIGN_SECTORS);
    }

    Ok(CardLayout {
        total_sectors,
        boot,
        areas,
    })
}

fn align_up(sectors: u64, to: u64) -> u64 {
    sectors.div_ceil(to) * to
}

fn too_small(total_sectors: u64, wanted: u64) -> CoreError {
    CoreError::InvalidInput(format!(
        "this layout needs {} MB and the card holds {} MB",
        wanted * SECTOR_BYTES / (1024 * 1024),
        total_sectors * SECTOR_BYTES / (1024 * 1024),
    ))
}

/// The card's first sector, ready to be written at byte zero.
///
/// Boot code is left as zeroes: the Pi's firmware reads the *partition table*,
/// not x86 boot code, and 440 bytes of instructions no machine in this story
/// executes would be 440 bytes nobody could account for.
///
/// **The CHS fields carry the "look at the LBA" sentinel** — `00 02 00` for
/// the start and `FE FF FF` for the end. That is what MultibootOS's table
/// holds for all three of its partitions. CaffeineOS's holds real-looking CHS
/// for two of its four fields and the same sentinel for the rest, and **both
/// cards boot** — so the field is measurably not load-bearing here, and the
/// sentinel is the honest thing to write for a card far past anything CHS can
/// address.
pub fn write_mbr(layout: &CardLayout) -> [u8; SECTOR_BYTES as usize] {
    const CHS_START_SENTINEL: [u8; 3] = [0x00, 0x02, 0x00];
    const CHS_END_SENTINEL: [u8; 3] = [0xFE, 0xFF, 0xFF];

    let mut sector = [0u8; SECTOR_BYTES as usize];

    for (slot, partition) in layout.partitions().iter().enumerate() {
        let at = TABLE_OFFSET + slot * ENTRY_BYTES;
        let entry = &mut sector[at..at + ENTRY_BYTES];

        entry[0] = if partition.bootable { 0x80 } else { 0x00 };
        entry[1..4].copy_from_slice(&CHS_START_SENTINEL);
        entry[4] = partition.type_byte;
        entry[5..8].copy_from_slice(&CHS_END_SENTINEL);
        // Little-endian, and truncating is not a risk anybody has to think
        // about: a `u32` of sectors is 2 TB, and this is a card.
        entry[8..12].copy_from_slice(&(partition.start_lba as u32).to_le_bytes());
        entry[12..16].copy_from_slice(&(partition.sector_count as u32).to_le_bytes());
    }

    sector[510..512].copy_from_slice(&SIGNATURE);
    sector
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

    // ---- writing one (SD-1 · G2) ----

    const GIB: u64 = 1024 * 1024 * 1024;

    /// The strongest check available without a card in the drive: build the
    /// table each real card carries, from that card's own numbers, and read it
    /// back with the parser that reads the cards themselves.
    ///
    /// The numbers are from `docs/sd2-card-layout.md`, taken off the two cards
    /// with a hex reader. They are facts about a layout, not content — no
    /// card's bytes are in this repository.
    #[test]
    fn the_two_real_cards_layouts_round_trip() {
        // MultibootOS 2.2, 128 GB: FAT32 then two Amiga disks.
        let multiboot = write_mbr(&CardLayout {
            total_sectors: 127_999_672_320 / SECTOR_BYTES,
            boot: MbrPartition {
                index: 0,
                kind: PartitionKind::Fat32,
                type_byte: 0x0C,
                bootable: true,
                start_lba: 2048,
                sector_count: 2_299_904,
            },
            areas: vec![
                MbrPartition {
                    index: 1,
                    kind: PartitionKind::AmigaRdb,
                    type_byte: 0x76,
                    bootable: false,
                    start_lba: 2_301_952,
                    sector_count: 96_468_992,
                },
                MbrPartition {
                    index: 2,
                    kind: PartitionKind::AmigaRdb,
                    type_byte: 0x76,
                    bootable: false,
                    start_lba: 98_770_944,
                    sector_count: 138_731_104,
                },
            ],
        });

        let read = parse_mbr(&multiboot).expect("ART must be able to read what ART writes");
        assert_eq!(read.partitions.len(), 3);
        assert_eq!(read.boot_partition().unwrap().start_bytes(), 1_048_576);

        let areas = read.amiga_areas();
        assert_eq!(areas.len(), 2, "MultibootOS carries two Amiga disks");
        // The offset that made ART-095 a bug: the Amiga's own table is 1.1 GB
        // into the card, not at byte zero.
        assert_eq!(areas[0].start_bytes(), 1_178_599_424);
        assert_eq!(areas[1].start_bytes(), 50_570_723_328);

        // CaffeineOS 9317, 64 GB: the same front, one Amiga disk.
        let caffeine = write_mbr(&CardLayout {
            total_sectors: 63_864_569_856 / SECTOR_BYTES,
            boot: MbrPartition {
                index: 0,
                kind: PartitionKind::Fat32,
                type_byte: 0x0C,
                bootable: true,
                start_lba: 2048,
                sector_count: 2_299_904,
            },
            areas: vec![MbrPartition {
                index: 1,
                kind: PartitionKind::AmigaRdb,
                type_byte: 0x76,
                bootable: false,
                start_lba: 2_301_952,
                sector_count: 118_235_136,
            }],
        });

        let read = parse_mbr(&caffeine).unwrap();
        assert_eq!(read.amiga_areas().len(), 1);
        assert_eq!(read.amiga_areas()[0].start_bytes(), 1_178_599_424);
    }

    /// Asked for MultibootOS's shape, the planner produces MultibootOS's
    /// layout — the same boot partition, the same two starts, to the sector.
    ///
    /// This is the test that says the defaults are not invented. Every number
    /// it asserts was read off a card that boots a real Amiga.
    ///
    /// One difference, and it is policy rather than correctness: asked for
    /// "the rest", ART gives the last disk **all** of it, while MultibootOS
    /// leaves about 6 GiB of the card unallocated at the end. Nothing needs
    /// that space — it is what a tool leaves when it was told a size rather
    /// than "the rest" — so ART allocating it is a choice, not a mistake, and
    /// a caller that wants the tail left alone says so with a size.
    #[test]
    fn asked_for_a_real_cards_shape_the_planner_produces_it() {
        let layout = plan_card(127_999_672_320, 0, &[49_392_123_904, 0]).unwrap();

        assert_eq!(layout.boot.start_lba, 2048);
        assert_eq!(layout.boot.sector_count, 2_299_904);
        assert_eq!(layout.boot.type_byte, 0x0C);

        assert_eq!(layout.areas[0].start_lba, 2_301_952, "the first Amiga disk");
        assert_eq!(layout.areas[0].sector_count, 96_468_992);
        assert_eq!(layout.areas[1].start_lba, 98_770_944, "the second");
        assert_eq!(layout.areas[1].type_byte, 0x76);

        // The tail: ART fills it, MultibootOS's 138_731_104 does not.
        assert_eq!(
            layout.areas[1].sector_count,
            249_999_360 - 98_770_944,
            "asked for the rest, the last disk gets the rest"
        );
    }

    /// A planned card and a written card agree — the plan is not a description
    /// of something else.
    #[test]
    fn a_planned_card_reads_back_as_it_was_planned() {
        let layout = plan_card(64 * GIB, 0, &[0]).unwrap();
        let read = parse_mbr(&write_mbr(&layout)).unwrap();

        assert_eq!(read.partitions.len(), 2);
        assert_eq!(read.partitions, layout.partitions());
        assert_eq!(
            read.boot_partition().unwrap().sector_count,
            DEFAULT_BOOT_SECTORS,
            "the default boot partition is the one both real cards carry"
        );
        // The Amiga disk takes what is left, and starts where both real cards
        // start theirs.
        assert_eq!(read.amiga_areas()[0].start_bytes(), 1_178_599_424);
    }

    /// **Byte zero of a card is never an Amiga disk.** Unit 0 is the whole
    /// card, MBR included, and SD-0 asks that ART make such a layout
    /// impossible to generate. It is impossible because there is no way to say
    /// it: the boot partition is not optional and it is first.
    #[test]
    fn no_planned_card_puts_an_amiga_disk_at_byte_zero() {
        for total in [8 * GIB, 32 * GIB, 128 * GIB] {
            for shares in [vec![0u64], vec![4 * GIB, 0], vec![2 * GIB, 2 * GIB, 0]] {
                let layout = plan_card(total, 0, &shares).unwrap();
                assert_eq!(layout.boot.start_lba, FIRST_PARTITION_LBA);
                for area in &layout.areas {
                    assert!(
                        area.start_lba > FIRST_PARTITION_LBA + layout.boot.sector_count - 1,
                        "an Amiga disk must start after the boot partition, not at {}",
                        area.start_lba
                    );
                    assert!(area.start_bytes() > 0);
                }
            }
        }
    }

    /// Every Amiga disk begins on a 4 MiB boundary, which is what both real
    /// cards do and what the flash underneath is built around.
    #[test]
    fn every_area_starts_on_an_erase_block_boundary() {
        let layout = plan_card(128 * GIB, 0, &[10 * GIB, 20 * GIB, 0]).unwrap();
        for area in &layout.areas {
            assert_eq!(
                area.start_bytes() % (4 * 1024 * 1024),
                0,
                "{} is not 4 MiB aligned",
                area.start_bytes()
            );
        }
    }

    #[test]
    fn areas_never_overlap_and_never_run_past_the_card() {
        let layout = plan_card(64 * GIB, 0, &[8 * GIB, 8 * GIB, 0]).unwrap();
        let mut previous_end = layout.boot.start_lba + layout.boot.sector_count;
        for area in &layout.areas {
            assert!(area.start_lba >= previous_end, "areas overlap");
            previous_end = area.start_lba + area.sector_count;
        }
        assert!(
            previous_end <= layout.total_sectors,
            "the last area runs off the end"
        );
    }

    #[test]
    fn a_card_carries_one_to_three_amiga_disks() {
        assert!(plan_card(64 * GIB, 0, &[]).is_err(), "none");
        assert!(
            plan_card(64 * GIB, 0, &[GIB, GIB, GIB, 0]).is_err(),
            "four primaries, one is the boot partition"
        );
        assert!(plan_card(64 * GIB, 0, &[GIB, GIB, 0]).is_ok());
    }

    /// "Whatever is left" belongs to the last disk — which is what both real
    /// cards do. Anywhere else it would be asking the remainder to be divided
    /// by a rule nobody stated.
    #[test]
    fn only_the_last_disk_may_take_the_rest() {
        assert!(plan_card(64 * GIB, 0, &[GIB, 0]).is_ok(), "last: fine");
        assert!(
            plan_card(64 * GIB, 0, &[0, GIB]).is_err(),
            "first: by what rule?"
        );
        assert!(plan_card(64 * GIB, 0, &[0, 0]).is_err());
        assert!(
            plan_card(64 * GIB, 0, &[0]).is_ok(),
            "the only one is the last one"
        );
    }

    /// A card too small is refused with both numbers, not silently shrunk to
    /// something that would not hold what the user asked for.
    #[test]
    fn a_card_too_small_is_refused_by_name() {
        // 512 MB cannot hold a 1.1 GiB boot partition at all.
        let err = plan_card(512 * 1024 * 1024, 0, &[0]).unwrap_err();
        assert!(err.to_string().contains("MB"), "{err}");

        // Nor one whose areas add up to more than there is.
        assert!(plan_card(8 * GIB, 0, &[6 * GIB, 6 * GIB]).is_err());
    }

    /// The sector is a partition table and nothing else: no boot code, and the
    /// signature where every reader looks for it.
    #[test]
    fn the_written_sector_is_a_table_and_a_signature() {
        let sector = write_mbr(&plan_card(32 * GIB, 0, &[0]).unwrap());
        assert_eq!(sector.len(), 512);
        assert_eq!(&sector[510..512], &[0x55, 0xAA]);
        assert!(
            sector[..TABLE_OFFSET].iter().all(|&b| b == 0),
            "boot code is left as zeroes; no machine in this story executes x86"
        );
        // The fourth slot is empty on a two-partition card, and empty means
        // sixteen zero bytes rather than something that looks like a type.
        assert!(
            sector[TABLE_OFFSET + 3 * ENTRY_BYTES..TABLE_OFFSET + 4 * ENTRY_BYTES]
                .iter()
                .all(|&b| b == 0)
        );
    }
}
