//! Network access — the one part of the Software Sources Engine that cannot
//! live in `core/`.
//!
//! `core/sources` declares [`MirrorClient`](crate::core::sources::mirror::MirrorClient)
//! and every rule about *where* ART may fetch from; this module is the
//! transport that carries it out. Keeping the split that way round means the
//! security decisions are unit-testable without a socket, and a second
//! transport (a CLI shell, a proxy) cannot loosen them.
//!
//! Nothing else in ART may open a connection.

pub mod http_mirror;

#[cfg(test)]
mod live_aminet;
