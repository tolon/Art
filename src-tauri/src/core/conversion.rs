//! Image conversion engine — **Phase 5**.
//!
//! Format-aware conversions (DMS → ADF, ADF → ADZ, ADF → HDF, ...). Never
//! performs extension-only conversions.

use crate::core::error::{CoreError, CoreResult};

/// Reserved for Phase 5: convert an image between formats.
pub fn convert(_source: &std::path::Path, _target_format: &str) -> CoreResult<()> {
    Err(CoreError::NotImplemented("conversion::convert".into()))
}
