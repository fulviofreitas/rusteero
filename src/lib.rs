//! # rusteero
//!
//! Async Rust client for the Eero (Amazon) mesh Wi-Fi cloud API.
//!
//! `rusteero` is a port of the Python [`eero-api`](https://github.com/fulviofreitas/eero-api)
//! library. It keeps that library's contract: every endpoint returns the raw
//! `{"meta": …, "data": …}` JSON envelope from the Eero cloud API, method names are the same,
//! and stored credentials are wire-compatible.
//!
//! **Unofficial project.** This crate uses reverse-engineered APIs and is not affiliated with
//! or endorsed by Eero or Amazon. Accounts that sign in with Amazon cannot use the email/SMS
//! verification flow; see the project wiki for the documented workaround.
//!
//! ## Status
//!
//! Phase 0 (bootstrap). No endpoints are implemented yet. See `PARITY.md` in the repository
//! for the per-method checklist against `eero-api`.

/// Crate version, as compiled from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_semver_like() {
        assert_eq!(super::VERSION.split('.').count(), 3);
    }
}
