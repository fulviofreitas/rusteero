//! Thread (smart-home mesh) API: `eero-api`'s `ThreadAPI`, v8.4.1.
//!
//! Ported from `eero-api src/eero/api/thread.py` at v8.4.1. Per that module's own docstring: the
//! read goes through the network's published `thread` link, like every other sub-resource; every
//! write below targets the **literal** `networks/{id}/thread` path directly, never through a
//! published link — `parent` is accepted by every write method here for signature consistency
//! with the rest of this crate's domain modules, but is never consulted (see
//! `src/routes/thread.rs`'s module docs, for why this asymmetry is upstream-declared, not an
//! oversight to "fix").

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `ThreadAPI` (`src/eero/api/thread.py`), v8.4.1.
///
/// Build one with [`ThreadApi::new`], wrapping a [`Transport`] shared with the rest of the
/// [`crate::api::EeroApi`] aggregator — `ThreadApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct ThreadApi {
    transport: Arc<Transport>,
}

impl ThreadApi {
    /// Wraps `transport` as a `ThreadApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets Thread status for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/thread.py:40-68` (`ThreadAPI.get_thread`). GETs the
    /// network's `thread` link, preferring a supplied `parent`'s published link over the bare-id
    /// template.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_thread(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::thread::GET_THREAD,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Enables or disables Thread — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/thread.py:70-99` (`ThreadAPI.set_thread_enabled`), an
    /// alias of [`ThreadApi::update_thread`] with only `thread_enable` set
    /// (`thread.py:99`): it issues a JSON PUT of exactly `{"thread_enable": enabled}` to the
    /// **literal** `networks/{id}/thread` path — never through the `thread` link
    /// [`ThreadApi::get_thread`] prefers — and logs `update_thread`'s uncharacterised-write
    /// warning (`"update thread config for network"`). The `{"enabled": …}` key this method sent
    /// before v8.4.0 is not in the observed spec and is gone. Unverified upstream.
    ///
    /// `parent` is accepted only for signature consistency with the rest of this family — Python
    /// itself documents it "Unused; accepted for signature consistency" (`thread.py:89-90`) and
    /// never consults it; this port keeps the parameter but never reads it, matching that
    /// verbatim rather than "fixing" it into a link-preferring resolve (see this module's own
    /// docs).
    ///
    /// # Errors
    ///
    /// See [`ThreadApi::update_thread`]; the validation error for an empty payload cannot occur,
    /// because `thread_enable` is always supplied.
    pub async fn set_thread_enabled(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.update_thread(network_id, Some(enabled), None, parent)
            .await
    }

    /// Updates Thread configuration — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/thread.py:101-155` (`ThreadAPI.update_thread`). Issues
    /// a JSON PUT to the literal `networks/{id}/thread` path with exactly the keys supplied among
    /// `thread_enable` and `enable_credential_syncing` (`thread.py:143-146`) — an omitted (`None`)
    /// field is left out of the body entirely, never sent as `null`. Unverified upstream.
    ///
    /// `_parent` is unused — see [`ThreadApi::set_thread_enabled`]'s own docs for why.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "thread", .. }` if both `thread_enable` and
    /// `enable_credential_syncing` are `None` (`thread.py:148-151`), before any request is sent.
    /// Otherwise see [`ThreadApi::get_thread`].
    pub async fn update_thread(
        &self,
        network_id: &str,
        thread_enable: Option<bool>,
        enable_credential_syncing: Option<bool>,
        _parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut payload = Map::new();
        if let Some(thread_enable) = thread_enable {
            payload.insert("thread_enable".to_owned(), Value::Bool(thread_enable));
        }
        if let Some(enable_credential_syncing) = enable_credential_syncing {
            payload.insert(
                "enable_credential_syncing".to_owned(),
                Value::Bool(enable_credential_syncing),
            );
        }
        if payload.is_empty() {
            return Err(Error::validation(
                "thread",
                "at least one of thread_enable, enable_credential_syncing is required",
            ));
        }

        let url =
            routes::thread::PUT_THREAD.resolve(self.transport.api_host(), network_id, None)?;
        warn_uncharacterised_write("update thread config for network");
        self.transport
            .request(
                routes::thread::PUT_THREAD.method.clone(),
                url,
                &[],
                RequestBody::Json(Value::Object(payload)),
            )
            .await
    }

    /// Regenerates Thread network credentials — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/thread.py:157-191`
    /// (`ThreadAPI.regenerate_thread_credentials`). Issues a `POST` with the literal two-byte body
    /// `""` to the literal `networks/{id}/thread` path (`RequestEncoding.EMPTY_JSON_STRING`,
    /// mirrored here as [`RequestBody::EmptyJsonString`]). Unverified upstream; the response is
    /// documented to carry a `network` key of unknown shape — this method returns it unmodified,
    /// like every other method in this crate.
    ///
    /// `_parent` is unused — see [`ThreadApi::set_thread_enabled`]'s own docs for why.
    ///
    /// # Errors
    ///
    /// See [`ThreadApi::get_thread`].
    pub async fn regenerate_thread_credentials(
        &self,
        network_id: &str,
        _parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url =
            routes::thread::POST_THREAD.resolve(self.transport.api_host(), network_id, None)?;
        warn_uncharacterised_write("regenerate thread credentials for network");
        self.transport
            .request(
                routes::thread::POST_THREAD.method.clone(),
                url,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }
}
