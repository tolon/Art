//! Tauri command modules.
//!
//! Commands are thin adapters: they translate frontend requests into calls on
//! the core engine / services and serialise the results back. No business
//! logic lives here.

pub mod adf;
pub mod archive;
pub mod archives;
pub mod card;
pub mod cbm;
pub mod checkout;
pub mod collection;
pub mod distro;
pub mod dragdrop;
pub mod gotek;
pub mod hdf;
pub mod iso;
pub mod jobs;
pub mod lha;
pub mod oplog;
pub mod panel;
pub mod pistorm;
pub mod sources;
pub mod system;
pub mod volume;
pub mod volume_write;
pub mod whdload;
pub mod winuae;
pub mod workflow;
