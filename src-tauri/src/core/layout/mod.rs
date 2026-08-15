//! What goes where (SD-2 · G11).
//!
//! Between the classifier and the card: a pile of files in, a staging tree on
//! the PC out, which the preload screen then copies onto a volume. The staging
//! seam is not a preference — a real PiStorm card is PFS3 and ART cannot write
//! PFS3, so writing straight into the volume works only on FFS, which is not
//! what a finished card uses.

pub mod apply;
pub mod policy;
pub mod scan;

use serde::{Deserialize, Serialize};

/// What ART can justify saying about one thing on disk.
///
/// **There is no `Demo` and there will not be one.** `Detection` carries a
/// category, a format hint and a confidence, and nothing derivable from the
/// bytes separates a demo from a game. The preview is editable instead; §14
/// and §34 say an uncertain classification is offered, never acted on as fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ItemKind {
    /// An archive holding a WHDLoad pack. `name` is what the drawer will be
    /// called, from `core/whdload::analyse`.
    WhdloadArchive {
        name: String,
    },
    /// A folder that *is* a drawer — it directly holds a `.slave`.
    WhdloadDrawer {
        name: String,
    },
    FloppyImage,
    HardDiskImage,
    OpticalImage,
    /// An archive that is not a WHDLoad pack. Could be anything, so it goes to
    /// `Unsorted/` and the user moves it.
    Archive,
    Unknown,
    /// Belongs on the FAT32 boot partition. Refused here.
    Rom,
    /// No business on an Amiga volume at all. Refused here.
    Commodore8Bit,
}
