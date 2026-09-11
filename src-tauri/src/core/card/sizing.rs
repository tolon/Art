//! How big a card image and its partitions are — the one-button card
//! design's § 4 (`docs/superpowers/specs/2026-09-11-one-button-card-design.md`).
//!
//! Every number here is measured or read from the code of a project that
//! works; the sources are in `docs/superpowers/notes/2026-09-11-card-layout-research.md`.

/// The share of a card's printed capacity an image may use, per mille.
///
/// **95 %, emu68hatcher's rule** — *"95% of decimal GB for SD card safety"*
/// (`rootrootde/emu68hatcher`, MIT, `config/partition_helpers.py:34-36`).
/// Cards carrying one label differ by up to ~2 %: two real "64 GB" cards
/// expose 62 534 975 488 and 63 864 569 856 bytes, and an image has to fit the
/// smaller. MultibootOS's last partition ends at 121.60 × 10⁹ on a 128 GB card
/// — exactly 95 %.
pub const CARD_MARGIN_PER_MILLE: u64 = 950;

/// The image size for a card sold as `card_gb` gigabytes — **decimal**, the
/// way every card maker counts (ART-308: this used to be `card_gb × 2³⁰`, and
/// ART's "64" was 4.7 × 10⁹ bytes past any 64 GB card).
pub fn image_bytes_for_label(card_gb: u32) -> u64 {
    u64::from(card_gb) * 1_000_000 * CARD_MARGIN_PER_MILLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest real card measured for each label (fdisk output in the
    /// threads the research note cites). An image for that label must fit it.
    const SMALLEST_MEASURED: [(u32, u64); 3] = [
        (32, 31_914_983_424),
        (64, 62_534_975_488),
        (128, 127_865_454_592),
    ];

    #[test]
    fn a_card_image_is_ninety_five_percent_of_the_decimal_label() {
        assert_eq!(image_bytes_for_label(16), 15_200_000_000);
        assert_eq!(image_bytes_for_label(32), 30_400_000_000);
        assert_eq!(image_bytes_for_label(64), 60_800_000_000);
        assert_eq!(image_bytes_for_label(128), 121_600_000_000);
    }

    /// **ART-308.** The image for a label fits the smallest card of that label
    /// measured anywhere — and is no longer 64 GiB for "64".
    #[test]
    fn a_card_image_fits_the_smallest_real_card_of_its_label() {
        for (label, smallest) in SMALLEST_MEASURED {
            assert!(
                image_bytes_for_label(label) <= smallest,
                "{label} GB: {} > {smallest}",
                image_bytes_for_label(label)
            );
        }
        assert!(image_bytes_for_label(64) < 64 * 1024 * 1024 * 1024);
    }
}
