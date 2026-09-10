//! Authentication: the session credential value ([`Session`]) and its on-disk storage
//! representation. The login flow and the network-facing authentication API are implemented in
//! a later phase.
//!
//! Ported from `eero-api`'s `src/eero/api/auth.py`.
mod flow;
pub mod session;

pub use session::Session;
