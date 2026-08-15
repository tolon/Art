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
