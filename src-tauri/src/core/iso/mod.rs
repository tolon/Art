//! Reading ISO9660 optical discs.
//!
//! This commit lays down the two format primitives: the volume descriptor
//! set, and the directory records inside a directory's extent. The image
//! reader that puts them together arrives next.
//!
//! # Every number here came from a file ART did not write
//!
//! Extents, lengths and record sizes are all attacker-controlled in the
//! general case, and the release profile aborts on panic. So the descriptor
//! scan is capped, the entries in a directory are capped, and a zero-length
//! record advances to the next sector instead of looping.

pub mod descriptor;
pub mod directory;
