//! Profiles API for Eero: per-network device-grouping "profiles" (e.g. "Kids", "Guests").
//!
//! Ported from `eero-api`'s `src/eero/api/profiles.py`. This phase covers only the read-only
//! methods (`get_profiles`, `get_profile`, `get_profile_devices`, `get_blocked_applications`);
//! the mutation methods (`pause_profile`, `set_profile_devices`,
//! `update_profile_content_filter`, `update_profile_block_list`, `set_blocked_applications`,
//! `create_profile`, `rename_profile`, `delete_profile`) are phase 5 — see the marker comment
//! at the bottom of this file for where they go.
//!
//! As in Python, every method here returns the raw `{"meta": …, "data": …}` envelope
//! unmodified: `ProfilesApi` never extracts, filters or reshapes anything out of a response.
//! This matters most for `get_profile`, `get_profile_devices` and `get_blocked_applications`:
//! all three are the *same* wire call (`GET networks/{network_id}/profiles/{profile_id}`,
//! `routes::GET_PROFILE` and its two aliases `routes::GET_PROFILE_DEVICES` /
//! `routes::GET_BLOCKED_APPLICATIONS`) returning the same full profile object. The distinct
//! Python names exist purely to document caller intent — "I want this profile's `devices`
//! field" vs. "I want this profile's `blocked_applications` field" — not to select a different
//! request or to narrow the response. See each method's own docs for the citation.

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// The profiles API for a single Eero account, built on a shared [`Transport`].
///
/// Holds its `Transport` behind an `Arc` so it can be constructed independently and cloned
/// cheaply alongside sibling endpoint modules that share the same underlying connection and
/// session state (the not-yet-built `EeroApi` aggregator, phase 3).
#[derive(Debug)]
pub struct ProfilesApi {
    transport: Arc<Transport>,
}

impl ProfilesApi {
    /// Wraps `transport` as a `ProfilesApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Lists every profile on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:33`
    /// (`ProfilesAPI.get_profiles`): sends `GET` `routes::GET_PROFILES`
    /// (`networks/{network_id}/profiles`). The "not authenticated" precondition Python checks
    /// up front (`profiles.py:46-48`) is enforced by `Transport::send` itself, not duplicated
    /// here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_profiles(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(&routes::GET_PROFILES, &[("network_id", network_id)], None)
            .await
    }

    /// Gets a single profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:53` (`ProfilesAPI.get_profile`): sends
    /// `GET` `routes::GET_PROFILE` (`networks/{network_id}/profiles/{profile_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_profile(&self, network_id: &str, profile_id: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_PROFILE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                None,
            )
            .await
    }

    /// Gets a profile's data "including devices" — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:104`
    /// (`ProfilesAPI.get_profile_devices`), which sends the exact same request as
    /// [`ProfilesApi::get_profile`] (`routes::GET_PROFILE_DEVICES` is an alias of
    /// `routes::GET_PROFILE`): `GET networks/{network_id}/profiles/{profile_id}`. Python's
    /// docstring notes "The devices are in the `devices` field of the response data"
    /// (`profiles.py:107`), but neither Python nor this port extracts that field — the full,
    /// untouched profile object is returned, exactly as from `get_profile`. This method exists
    /// only so a caller searching for "how do I get a profile's devices" finds a name that says
    /// so; it does not change the request or the response shape in any way.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_profile_devices(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_PROFILE_DEVICES,
                &[("network_id", network_id), ("profile_id", profile_id)],
                None,
            )
            .await
    }

    /// Gets a profile's blocked applications (Eero Plus feature) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:267`
    /// (`ProfilesAPI.get_blocked_applications`), which sends the exact same request as
    /// [`ProfilesApi::get_profile`] (`routes::GET_BLOCKED_APPLICATIONS` is an alias of
    /// `routes::GET_PROFILE`): `GET networks/{network_id}/profiles/{profile_id}`. Python's
    /// docstring notes the blocked applications are in the `blocked_applications` or
    /// `premium_dns.blocked_applications` field of the response data (`profiles.py:270-271`),
    /// but neither Python nor this port extracts either field — the full, untouched profile
    /// object is returned, exactly as from `get_profile`. This method exists only so a caller
    /// searching for "how do I get a profile's blocked applications" finds a name that says so;
    /// it does not change the request or the response shape in any way.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn get_blocked_applications(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::GET_BLOCKED_APPLICATIONS,
                &[("network_id", network_id), ("profile_id", profile_id)],
                None,
            )
            .await
    }

    // Phase 5 (mutations, not yet ported) go here, in the same order as
    // `eero-api src/eero/api/profiles.py`:
    //   - pause_profile (profiles.py:77) — PUT routes::PAUSE_PROFILE, {"paused": bool}
    //   - set_profile_devices (profiles.py:130) — PUT routes::SET_PROFILE_DEVICES,
    //     {"devices": [{"url": ...}, ...]}
    //   - update_profile_content_filter (profiles.py:179) — PUT
    //     routes::UPDATE_PROFILE_CONTENT_FILTER, {"content_filter": {...}}. Python applies a
    //     server-side key whitelist *client-side first* (profiles.py:201-217: only
    //     `adblock`, `adblock_plus`, `safe_search`, `block_malware`, `block_illegal`,
    //     `block_violent`, `block_adult`, `youtube_restricted` survive; anything else is
    //     dropped with a warning) before building the request body — this filtering must be
    //     reproduced here, not delegated to the server.
    //   - update_profile_block_list (profiles.py:227) — PUT routes::UPDATE_PROFILE_BLOCK_LIST,
    //     {"custom_block_list": [..]} or {"custom_allow_list": [..]}
    //   - set_blocked_applications (profiles.py:294) — PUT routes::SET_BLOCKED_APPLICATIONS,
    //     {"blocked_applications": [..]}
    //   - create_profile (profiles.py:336) — POST routes::CREATE_PROFILE, {"name": str}
    //   - rename_profile (profiles.py:364) — PUT routes::RENAME_PROFILE, {"name": str}
    //   - delete_profile (profiles.py:391) — DELETE routes::DELETE_PROFILE
}
