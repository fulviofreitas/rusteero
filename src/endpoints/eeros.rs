//! `EerosApi` — Eero (Amazon mesh node) endpoints, ported from `eero-api`'s `EerosAPI`
//! (`src/eero/api/eeros.py:18-331`).
//!
//! This phase implements only the four read (`GET`) methods: [`EerosApi::get_eeros`],
//! [`EerosApi::get_eero`], [`EerosApi::get_led_status`] and [`EerosApi::get_nightlight`]. The
//! six mutation methods (`reboot_eero`, `set_led`, `set_led_brightness`, `set_nightlight`,
//! `set_nightlight_brightness`, `set_nightlight_schedule`) are phase 5 — see the marker comment
//! at the bottom of the `impl` block for what still needs to land there and the two behavioural
//! gotchas already known before that work starts.
//!
//! ## The `network_id` parameter Python ignores
//!
//! Three of the four Python methods ported here — `get_eero`, `get_led_status` and
//! `get_nightlight` — declare a `network_id: str` parameter that is never read; each method's
//! own docstring says so explicitly (`"... (unused, kept for API compatibility)"`,
//! `eeros.py:57,101,192`). All three make the exact same wire call, `GET eeros/{eero_id}` (not
//! nested under `networks/`), regardless of what `network_id` holds or whether it names a
//! network the Eero even belongs to.
//!
//! `rusteero` drops the parameter from all three signatures here rather than keeping a dead one:
//!
//! - It carries no information the request ever uses, on either side of the wire — the wire path
//!   is [`crate::routes::GET_EERO`] in every case, which has no `{network_id}` placeholder to
//!   fill.
//! - Keeping it would force every caller to supply a value that is silently discarded, with
//!   nothing at the type level warning them that it does nothing.
//! - `get_eeros`, the one method that *does* nest under `networks/{network_id}/eeros`, keeps its
//!   `network_id` — the parameter is dropped only where Python itself never uses it, not across
//!   the whole module.
//!
//! This is a deliberate signature divergence from the Python source; see `PARITY.md` for where
//! it is recorded.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// Eero (mesh node) endpoints. Ported from `EerosAPI` (`eero-api src/eero/api/eeros.py:18-31`).
///
/// Build one with [`EerosApi::new`], wrapping an already-configured [`Transport`] — typically
/// the same `Transport` shared with every other endpoint module behind the not-yet-built
/// `EeroApi` aggregator (phase 3/4).
#[derive(Debug)]
pub struct EerosApi {
    transport: Arc<Transport>,
}

impl EerosApi {
    /// Wraps `transport` as an `EerosApi`.
    ///
    /// Ported from `EerosAPI.__init__` (`eero-api src/eero/api/eeros.py:25-31`), which stores an
    /// `AuthAPI` handle rather than a `Transport` — `rusteero` centralises the authenticated
    /// request path in [`Transport`] itself (see `transport.rs`'s module docs), so every
    /// endpoint module holds one of those instead of a Python-shaped auth handle.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `EerosAPI.get_eeros` (`eero-api src/eero/api/eeros.py:33-51`). Sends
    /// `GET` [`crate::routes::GET_EEROS`] with `network_id` filled into the
    /// `networks/{network_id}/eeros` path template, through [`Transport::send`] — which already
    /// implements the "not authenticated" precondition Python re-checks at the top of this
    /// method (`eeros.py:46-48`), so no separate guard is needed here.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_eeros(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_EEROS, &[("network_id", network_id)], None)
            .await
    }

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `EerosAPI.get_eero` (`eero-api src/eero/api/eeros.py:53-72`); see the module
    /// docs for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_EERO`] with `eero_id` filled into the `eeros/{eero_id}` path
    /// template — **not** nested under `networks/` — through [`Transport::send`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_eero(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_EERO, &[("eero_id", eero_id)], None)
            .await
    }

    /// Gets LED status (`led_on`, `led_brightness`) for an Eero device — returns the raw Eero
    /// API response.
    ///
    /// Ported from `EerosAPI.get_led_status` (`eero-api src/eero/api/eeros.py:95-116`); see the
    /// module docs for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_LED_STATUS`], an alias of [`crate::routes::GET_EERO`] — the exact
    /// same wire call as [`EerosApi::get_eero`]; the LED fields simply live in the same node
    /// object Python reads them out of client-side.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_led_status(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_LED_STATUS, &[("eero_id", eero_id)], None)
            .await
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Nightlight is only available on Eero Beacon devices; the raw response includes a
    /// `nightlight` object in `data` only when the device supports it. Ported from
    /// `EerosAPI.get_nightlight` (`eero-api src/eero/api/eeros.py:185-207`); see the module docs
    /// for why this port drops Python's unused `network_id` parameter. Sends `GET`
    /// [`crate::routes::GET_NIGHTLIGHT`], an alias of [`crate::routes::GET_EERO`] — the exact
    /// same wire call as [`EerosApi::get_eero`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any network call is made. Returns whatever status-mapped error the request
    /// produces otherwise (see [`Error`]'s own docs).
    pub async fn get_nightlight(&self, eero_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_NIGHTLIGHT, &[("eero_id", eero_id)], None)
            .await
    }

    // =================================================================================
    // Phase 5 (not implemented here): the six `EerosAPI` mutation methods.
    //
    // - `reboot_eero`        (`eeros.py:74`)  — POST `crate::routes::REBOOT_EERO`.
    // - `set_led`            (`eeros.py:118`) — PUT `crate::routes::SET_LED`,
    //   `{"led_on": bool}`.
    // - `set_led_brightness` (`eeros.py:150`) — PUT `crate::routes::SET_LED_BRIGHTNESS`
    //   (alias of `SET_LED`), `{"led_brightness": 0..=100}`. Python clamps the caller's value
    //   into range with `max(0, min(100, brightness))` rather than rejecting an out-of-range
    //   input; the Rust port must reproduce that clamp, not add validation Python does not have.
    // - `set_nightlight`     (`eeros.py:209`) — PUT `crate::routes::SET_NIGHTLIGHT` (alias of
    //   `SET_LED`), `{"nightlight": {...}}` built from whichever optional fields are `Some`.
    //   Python brightness fields are clamped the same way as `set_led_brightness`. When *no*
    //   optional field is provided, Python never calls the network at all — it logs a warning
    //   and fabricates a *local* `{"meta": {"code": 400}, "data": {}}` envelope (`eeros.py:269-272`).
    //   That fabricated envelope would violate this crate's "the raw envelope is the contract,
    //   never transform or invent it" rule (see this crate's `CLAUDE.md`), so the Rust port must
    //   diverge here: an empty call becomes `Error::Validation` instead, before any request is
    //   built. Document that divergence at the call site when it lands.
    // - `set_nightlight_brightness` (`eeros.py:282`) — delegates to `set_nightlight`, no
    //   separate route.
    // - `set_nightlight_schedule`   (`eeros.py:302`) — delegates to `set_nightlight`, no
    //   separate route.
    // =================================================================================
}
