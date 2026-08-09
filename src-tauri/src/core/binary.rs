//! Binary / Amiga executable inspector — **Phase 7**.
//!
//! Detects Amiga executable / Hunk format, WHDLoad slaves, icons, etc.

use crate::core::error::{CoreError, CoreResult};

/// Reserved for Phase 7: inspect a binary file's structure.
pub fn inspect(_path: &std::path::Path) -> CoreResult<()> {
    Err(CoreError::NotImplemented("binary::inspect".into()))
}
