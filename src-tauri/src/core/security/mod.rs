//! Security primitives shared across ART's core.
//!
//! Two hazards live here: path-traversal defence for archive extraction
//! ([`path`]), and what may never reach an AmigaDOS command line ART generates
//! ([`amigados`]). Everything here is platform-independent (no `tauri`, no OS
//! APIs) so it stays testable.

pub mod amigados;
pub mod path;

pub use amigados::refuse_shell_metacharacters;
pub use path::{safe_join, PathTraversalError};
