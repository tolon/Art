//! Workflow Engine
//!
//! The Workflow Engine is the connective tissue of ART: it turns a detected
//! object (`Detection`) into an ordered list of candidate workflows, ranks the
//! recommended ones, and (in later phases) executes them through the standard
//! pipeline `SOURCE → ANALYZE → VALIDATE → PREVIEW → BACKUP → APPLY → VERIFY
//! → REPORT`.
//!
//! The catalogue of registered workflows lives in [`builtin`]; the routing and
//! ranking machinery lives in [`registry`]. Most entries open a studio with the
//! object loaded ([`types::WorkflowKind::Navigate`]); the rest run engine work.

pub mod builtin;
pub mod registry;
pub mod types;

pub use registry::{WorkflowEngine, WorkflowRegistry};
pub use types::Plan;
