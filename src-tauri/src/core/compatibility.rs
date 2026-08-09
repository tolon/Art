//! Amiga Compatibility Engine — **Phase 9**.
//!
//! Compares software metadata against machine profiles to produce a
//! compatibility verdict (HIGH / MEDIUM / LOW / UNKNOWN).

use serde::{Deserialize, Serialize};

use crate::core::workflow::types::Confidence;

/// A compatibility verdict produced by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityVerdict {
    pub confidence: Confidence,
    pub recommended_machine: Option<String>,
    pub notes: String,
}

impl Default for CompatibilityVerdict {
    fn default() -> Self {
        Self {
            confidence: Confidence::Unknown,
            recommended_machine: None,
            notes: "Compatibility analysis arrives in a later phase.".to_string(),
        }
    }
}
