//! Profiles API for Eero: per-network device-grouping "profiles" (e.g. "Kids", "Guests").
//!
//! Ported from `eero-api`'s `src/eero/api/profiles.py`. Covers every `ProfilesAPI` method,
//! read-only and mutating alike.
//!
//! As in Python, every method here returns the raw `{"meta": …, "data": …}` envelope
//! unmodified: `ProfilesApi` never extracts, filters or reshapes anything out of a response —
//! the one deliberate exception is `update_profile_content_filter`'s *outgoing* key whitelist
//! (`profiles.py:201-217`), which is a request-shaping step Python itself performs before the
//! request is even sent, not a response transform. That method also fails closed with
//! `Error::Validation` before sending anything if the whitelist leaves nothing behind — a
//! deliberate divergence from Python (security review finding F4); see that method's own docs.
//!
//! `get_profile`, `get_profile_devices` and `get_blocked_applications` are the *same* wire call
//! (`GET networks/{network_id}/profiles/{profile_id}`, `routes::GET_PROFILE` and its two
//! aliases `routes::GET_PROFILE_DEVICES` / `routes::GET_BLOCKED_APPLICATIONS`) returning the
//! same full profile object. Likewise, `pause_profile`, `set_profile_devices`,
//! `update_profile_content_filter`, `update_profile_block_list`, `set_blocked_applications` and
//! `rename_profile` all `PUT` the same `networks/{network_id}/profiles/{profile_id}` resource
//! (`routes::PUT_PROFILE` and its aliases) with a different JSON key each. The distinct Python
//! names exist purely to document caller intent, not to select a different request or to narrow
//! the response. See each method's own docs for the citation.

use std::sync::Arc;

use serde_json::json;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::Transport;

/// Content-filter keys the Eero cloud API accepts on a profile, verbatim from
/// `eero-api src/eero/api/profiles.py:201-209`'s `valid_filters` set.
///
/// [`ProfilesApi::update_profile_content_filter`] drops any caller-supplied key outside this
/// list *before* building the request body, reproducing Python's client-side whitelist exactly
/// (the server is never given a chance to reject an invalid key, because it is never sent).
const VALID_CONTENT_FILTER_KEYS: &[&str] = &[
    "adblock",
    "adblock_plus",
    "safe_search",
    "block_malware",
    "block_illegal",
    "block_violent",
    "block_adult",
    "youtube_restricted",
];

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

    /// Pauses or unpauses internet access for a profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:77` (`ProfilesAPI.pause_profile`): sends
    /// `PUT` `routes::PAUSE_PROFILE` (alias of `routes::PUT_PROFILE`,
    /// `networks/{network_id}/profiles/{profile_id}`) with body `{"paused": paused}`
    /// (`profiles.py:101`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn pause_profile(
        &self,
        network_id: &str,
        profile_id: &str,
        paused: bool,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::PAUSE_PROFILE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "paused": paused })),
            )
            .await
    }

    /// Sets the devices assigned to a profile — returns the raw Eero API response.
    ///
    /// **Replaces the profile's entire device assignment list.** This is not additive: any
    /// device previously assigned to `profile_id` but absent from `device_urls` is unassigned
    /// by this call, exactly as `eero-api` documents (`profiles.py:138`, "This replaces all
    /// existing device assignments with the provided list").
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:130`
    /// (`ProfilesAPI.set_profile_devices`): sends `PUT` `routes::SET_PROFILE_DEVICES` (alias of
    /// `routes::PUT_PROFILE`) with body `{"devices": [{"url": ...}, ...]}` — each URL wrapped
    /// in its own single-key object, matching Python's `devices_payload = [{"url": url} for
    /// url in device_urls]` (`profiles.py:165`) exactly; `device_urls` is never sent as a bare
    /// array of strings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn set_profile_devices(
        &self,
        network_id: &str,
        profile_id: &str,
        device_urls: &[&str],
    ) -> Result<Envelope, Error> {
        let devices: Vec<_> = device_urls
            .iter()
            .map(|url| json!({ "url": url }))
            .collect();
        self.transport
            .send(
                &routes::SET_PROFILE_DEVICES,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "devices": devices })),
            )
            .await
    }

    /// Updates a profile's content-filtering settings — returns the raw Eero API response.
    ///
    /// `filters` is filtered client-side against `VALID_CONTENT_FILTER_KEYS` *before* the
    /// request body is built: any `(key, _)` pair whose `key` is not in that list is silently
    /// dropped and never reaches the server, reproducing Python's `valid_filters` whitelist
    /// (`profiles.py:201-217`) exactly, including the drop-not-reject behaviour for a *single*
    /// unrecognized key mixed in with valid ones — Python logs a warning and continues rather
    /// than raising, and so does this port (minus the log line; see
    /// `.claude/rules/security-review.md` on this crate's logging discipline). Keys are otherwise
    /// passed through in the order given, and duplicate keys keep `filters`' own last-write-wins
    /// order, matching Python's `dict` iteration.
    ///
    /// **Deliberate divergence from Python (security review finding F4).** Unlike its four
    /// sibling setters in this crate (`set_nightlight`, `configure_security`,
    /// `configure_backup_network`, `set_dns_mode`), this method previously had no guard against
    /// the whitelist leaving *every* key dropped: a caller who only passed misspelled keys (e.g.
    /// `block_adult_content` instead of `block_adult`) got a `200 OK` for `PUT {"content_filter":
    /// {}}}`, silently believing a filter was enabled — if the server replaces rather than merges
    /// that nested object, this would clear every content filter already set on the profile, on
    /// the one endpoint group whose entire purpose is restriction. Python has the same whitelist
    /// but never guards this case either (`profiles.py:201-217` has no empty-payload check); this
    /// port adds one anyway rather than reproducing a fail-open outcome on a parental-control
    /// surface. An individual unrecognized key mixed in with at least one valid one is still
    /// dropped silently, matching Python exactly (see above) — only the *all-dropped* case is
    /// promoted to an error, since that is the one shape a caller cannot possibly have intended.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:179`
    /// (`ProfilesAPI.update_profile_content_filter`): sends `PUT`
    /// `routes::UPDATE_PROFILE_CONTENT_FILTER` (alias of `routes::PUT_PROFILE`) with body
    /// `{"content_filter": {...}}`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "filters", .. }` if `filters` is empty, or every key
    /// in it falls outside `VALID_CONTENT_FILTER_KEYS` — checked *before* any request is sent.
    /// Otherwise, returns [`Error::Authentication`] if no valid session is configured, or
    /// whatever status-mapped [`Error`] the request produces (see [`Transport::send`]).
    pub async fn update_profile_content_filter(
        &self,
        network_id: &str,
        profile_id: &str,
        filters: &[(&str, bool)],
    ) -> Result<Envelope, Error> {
        let mut content_filter = serde_json::Map::new();
        for (key, value) in filters {
            if VALID_CONTENT_FILTER_KEYS.contains(key) {
                content_filter.insert((*key).to_owned(), serde_json::Value::Bool(*value));
            }
        }

        if content_filter.is_empty() {
            return Err(Error::Validation {
                field: "filters".to_owned(),
                message: "must contain at least one recognized content-filter key".to_owned(),
            });
        }

        self.transport
            .send(
                &routes::UPDATE_PROFILE_CONTENT_FILTER,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "content_filter": content_filter })),
            )
            .await
    }

    /// Updates a profile's custom domain block or allow list — returns the raw Eero API
    /// response.
    ///
    /// The JSON key sent depends entirely on `block`: `{"custom_block_list": domains}` when
    /// `block` is `true`, `{"custom_allow_list": domains}` when `block` is `false` — never
    /// both, and never a `block`/`blocked` field of its own. Ported from
    /// `eero-api src/eero/api/profiles.py:227` (`ProfilesAPI.update_profile_block_list`), whose
    /// `list_type = "custom_block_list" if block else "custom_allow_list"` (`profiles.py:253`)
    /// this reproduces verbatim: sends `PUT` `routes::UPDATE_PROFILE_BLOCK_LIST` (alias of
    /// `routes::PUT_PROFILE`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn update_profile_block_list(
        &self,
        network_id: &str,
        profile_id: &str,
        domains: &[&str],
        block: bool,
    ) -> Result<Envelope, Error> {
        let list_key = if block {
            "custom_block_list"
        } else {
            "custom_allow_list"
        };
        self.transport
            .send(
                &routes::UPDATE_PROFILE_BLOCK_LIST,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ (list_key): domains })),
            )
            .await
    }

    /// Sets the blocked applications (Eero Plus feature) for a profile — returns the raw Eero
    /// API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:294`
    /// (`ProfilesAPI.set_blocked_applications`): sends `PUT`
    /// `routes::SET_BLOCKED_APPLICATIONS` (alias of `routes::PUT_PROFILE`) with body
    /// `{"blocked_applications": applications}` (`profiles.py:333`) — the full list replaces
    /// whatever was previously blocked, same replace-not-merge semantics as
    /// [`ProfilesApi::set_profile_devices`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn set_blocked_applications(
        &self,
        network_id: &str,
        profile_id: &str,
        applications: &[&str],
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::SET_BLOCKED_APPLICATIONS,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "blocked_applications": applications })),
            )
            .await
    }

    /// Creates a new profile on a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:336` (`ProfilesAPI.create_profile`):
    /// sends `POST` `routes::CREATE_PROFILE` (`networks/{network_id}/profiles`) with body
    /// `{"name": name}`. Python's docstring notes the response's `data` field "contains the
    /// full profile object including the assigned URL/ID" (`profiles.py:345-346`); this port
    /// returns that envelope untouched, exactly as it does everywhere else.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn create_profile(&self, network_id: &str, name: &str) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::CREATE_PROFILE,
                &[("network_id", network_id)],
                Some(json!({ "name": name })),
            )
            .await
    }

    /// Renames an existing profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:364` (`ProfilesAPI.rename_profile`):
    /// sends `PUT` `routes::RENAME_PROFILE` (alias of `routes::PUT_PROFILE`) with body
    /// `{"name": name}`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn rename_profile(
        &self,
        network_id: &str,
        profile_id: &str,
        name: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::RENAME_PROFILE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                Some(json!({ "name": name })),
            )
            .await
    }

    /// Deletes a profile from a network — returns the raw Eero API response.
    ///
    /// Devices previously assigned to `profile_id` become unassigned (`profiles.py:394`);
    /// the Eero cloud API performs that side effect server-side, nothing here initiates it.
    ///
    /// Ported from `eero-api src/eero/api/profiles.py:391` (`ProfilesAPI.delete_profile`):
    /// sends `DELETE` `routes::DELETE_PROFILE` (`networks/{network_id}/profiles/{profile_id}`),
    /// with no request body.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise (see [`Transport::send`]).
    pub async fn delete_profile(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        self.transport
            .send(
                &routes::DELETE_PROFILE,
                &[("network_id", network_id), ("profile_id", profile_id)],
                None,
            )
            .await
    }
}
