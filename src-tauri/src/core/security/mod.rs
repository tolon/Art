//! Security primitives shared across ART's core.
//!
//! Right now this is path-traversal defence for archive extraction. Everything
//! here is platform-independent (no `tauri`, no OS APIs) so it stays testable.

pub mod path;

pub use path::{safe_join, PathTraversalError};
