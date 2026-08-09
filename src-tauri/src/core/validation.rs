//! Validation engine — used across phases.
//!
//! Boot block, filesystem, bitmap and directory structure validation.
//! Phase 0 ships only the surface; concrete validators arrive with each
//! format module.

use crate::core::error::{CoreError, CoreResult};

/// Reserved: validate an image's structural consistency.
pub fn validate(_path: &std::path::Path) -> CoreResult<()> {
    Err(CoreError::NotImplemented("validation::validate".into()))
}
