//! Password API: read-only access to a network's Wi-Fi password.
//!
//! Ported from `eero-api src/eero/api/password.py`. Python's `PasswordAPI` has exactly one
//! method (`get_password`) and no mutating counterpart — there is no phase-5 work for this
//! module; a later reader need not look for one.
//!
//! # Sensitive data
//!
//! Unlike every other endpoint in this crate, this module's single response body carries a
//! secret: the network's Wi-Fi password, in cleartext, under `data`. `eero-api` routes this
//! module through a dedicated `SecureLoggerAdapter` (`src/eero/logging.py`) that redacts every
//! log call automatically; this crate has no such adapter (`tracing` plus
//! `secrecy::SecretString` replace it elsewhere for token handling — see
//! the port plan §3.7), so [`PasswordApi::get_password`] deliberately emits
//! no `tracing` call of its own: the only logging this method's request produces is the shared,
//! header/body-free `method + path + status` line `Transport::send` already emits for every
//! request. See that method's own doc comment for the discipline this places on *callers* of
//! this method instead.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// `eero-api`'s `PasswordAPI` (`src/eero/api/password.py`): a single read-only method returning
/// a network's Wi-Fi password.
///
/// Build one with [`PasswordApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the (not-yet-built) `EeroApi` aggregator — `PasswordApi` never constructs or owns a
/// `Transport` itself.
#[derive(Debug)]
pub struct PasswordApi {
    transport: Arc<Transport>,
}

impl PasswordApi {
    /// Wraps `transport` as a `PasswordApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the network's Wi-Fi password — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/password.py:33-54` (`PasswordAPI.get_password`).
    /// Sends `GET` [`crate::routes::GET_PASSWORD`] with `network_id` substituted into the path.
    ///
    /// # Security
    ///
    /// **The returned envelope's `data` carries the network's Wi-Fi password in cleartext.**
    /// This method itself never logs the response (see the module docs for why); the same
    /// obligation passes to the caller — never `Debug`-print, `tracing`-log, or otherwise emit
    /// the returned [`Envelope`]'s contents at any level. A caller that needs to log the
    /// envelope anyway (e.g. while debugging a shape mismatch) should first pass its value
    /// through [`crate::redact::redact_sensitive`], which redacts any key matching `"password"`
    /// (among others) before it ever reaches a log line.
    pub async fn get_password(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_PASSWORD, &[("network_id", network_id)], None)
            .await
    }
}
