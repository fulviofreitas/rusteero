//! The `Client` facade: [`EeroApi`] plus a [`Cache`] plus in-memory preferred-network state.
//!
//! Ported from `eero-api`'s `EeroClient` (`src/eero/client.py`). See
//! the client behaviour notes for the full behaviour brief this module implements — every
//! non-obvious decision below cites it, plus the exact `client.py` line range it replaces.
//!
//! # Scope: phases 4 and 5
//!
//! Phase 4 implemented every **read-only** `EeroClient` method: the eight cached getters
//! (`get_account`, `get_networks`, `get_network`, `get_eeros`, `get_devices`, `get_device`,
//! `get_profiles`, `get_profile` — brief §1.4, §6), every other `GET`-shaped pass-through that
//! has a corresponding method already implemented in [`crate::endpoints`], network-id
//! resolution (`_ensure_network_id`, brief §3), the `/account` fallback inside `get_networks`
//! (brief §4), and `clear_cache` plus the state it is wired into.
//!
//! Phase 5 (see the "Mutating pass-throughs" section near the bottom of this file) adds every
//! *mutating* `EeroClient` method (every `set_*`/`run_*`/`reboot_*`/`create_*`/`delete_*`/
//! `pause_*`/`block_*` that has a corresponding endpoint method in [`crate::endpoints`]), plus
//! the cache invalidation the behaviour brief's §2 table specifies for each — including a
//! deliberate improvement over Python `rust-port-plan.md` §3.8 records (`set_led_brightness`,
//! and every DNS/SQM/security setter, invalidating cache entries Python's own setters forget to)
//! and, for the device-blacklist family, [`Client::block_device`]/[`Client::unblock_device`]
//! invalidating both the single-device and the device-list cache entries on success (security
//! finding F2). See each method's own doc comment for the citation. The v8.0.4 rework of the
//! `schedule` domain ([`Client::enable_bedtime`], [`Client::clear_profile_schedule`],
//! [`Client::update_schedule`]) deliberately does **not** invalidate any cache entry — see
//! `src/client/schedule.rs`'s own doc comments for why the earlier (pre-v8) F3 divergence no
//! longer applies. `set_device_priority`/`get_device_priority` and the `get_activity*` family
//! remain unported — see the phase-5 section's own banner comment for why.
//!
//! # No `login`/`verify` on `Client` (architectural divergence from the port plan)
//!
//! `rust-port-plan.md` §3.7 lists `login`/`verify` as "same names on `Client` and `AuthApi`",
//! but [`crate::api::EeroApi`] itself deliberately has neither (see that module's own docs,
//! "`login`/`verify` are deliberately not mirrored"): the interactive handshake lives entirely
//! in [`crate::auth::flow::LoginFlow`]/[`crate::auth::flow::PendingLogin`], a type-state pair
//! that can only ever hand back a verified [`crate::auth::Session`] — there is no
//! `EeroApi`-level method an unverified login token could reach. `Client` sits on top of
//! `EeroApi`, so it inherits this gap: a `Client` is always constructed *already carrying*
//! whatever session it will use ([`ClientBuilder::session`], or one loaded from a configured
//! [`crate::storage::CredentialStore`] — see [`ClientBuilder::build`]), matching
//! the architecture notes' own request-flow sketch, `Client::builder().session(s)
//! .store(st).build()`. The brief's instruction to "wire `clear_cache` into the places Python
//! calls it: after verify, logout, `set_session_token` and `clear_session_token`" is honoured for
//! the three of those four that exist on this `Client` ([`Client::logout`],
//! [`Client::set_session_token`], [`Client::clear_session_token`]); there is no `Client::verify`
//! to wire it into, and no gap this leaves in practice — a freshly built `Client`'s cache is
//! already empty, so there is nothing a post-verify `clear_cache()` could ever have removed.
//!
//! # `get_account` does not share Python's refresh-hook gap (brief gotcha G12)
//!
//! The brief's G12 documents that Python's `get_account()` cannot self-heal a server-driven
//! session refresh, because it calls the bare `AuthAPI.get()` directly rather than a module with
//! `AuthenticatedAPI`'s `_refresh_hook` wired. This is **not** true of this port:
//! [`crate::endpoints::NetworksApi::get_account`] (where the equivalent Rust call lives — see
//! that method's own docs for why) sends its request through the ordinary
//! [`crate::transport::Transport::resource`], the exact same one-shot-refresh-and-retry path
//! every other cached getter in this file uses. That decision was already made at the endpoints layer
//! before this phase started; [`Client::get_account`] simply inherits it. Recorded here, and in
//! `PARITY.md`, as a deliberate behavioural improvement over Python, not an oversight.
//!
//! # The `/account` fallback: two deliberate divergences (brief gotchas G1, G6)
//!
//! [`Client::get_networks`] reproduces Python's `/account` fallback (brief §4) with two
//! decisions the brief explicitly calls out as needing a deliberate choice:
//!
//! - **G1 (synthesised envelope)**: when the fallback succeeds, this crate — like Python —
//!   returns an envelope whose `data` was never actually returned by a single server response
//!   (`meta` comes from the original `/networks` call, `data.networks` comes from `/account`).
//!   This contradicts the crate's usual "never transform the wire payload" rule, but is ported
//!   anyway: downstream tools rely on `get_networks()` returning a non-empty list whenever the
//!   account actually has networks, and Python's own behaviour is the only precedent for what
//!   that synthesised shape should look like. This is the **one** documented exception to "every
//!   endpoint method returns the envelope unmodified" in this crate.
//! - **G6 (silent exception swallowing) — NOT reproduced.** Python wraps the entire fallback
//!   attempt in `except Exception: _LOGGER.debug(...)` (`client.py:462-465`), so an
//!   authentication failure, timeout, or any other error while fetching `/account` is invisible
//!   to the caller — `get_networks()`
//!   just returns the original, still-empty `/networks` envelope. This crate instead propagates
//!   that error with `?`: if the `/networks` list is empty and the `/account` fallback itself
//!   fails, [`Client::get_networks`] returns that `Err` rather than a silently-degraded `Ok`.
//!   A `Result`-returning API hiding a real failure behind a successful-looking empty list is
//!   exactly the kind of thing this port has fixed elsewhere (e.g. `transport.rs`'s refresh-retry
//!   divergence); reproducing it here would be inconsistent with that precedent. Flagged for
//!   `PARITY.md` as a deliberate divergence, not an oversight.
//!
//! # `get_profile_devices` keeps Python's `auto_discover=True` (brief gotcha G5)
//!
//! `rust-port-plan.md` §1.6 claims every method from `get_diagnostics` (`client.py:809`) onward
//! passes `auto_discover=False`. The brief corrects this: `get_profile_devices`
//! (`client.py:1355-1360`) and, in phase 5, `set_profile_devices` (`client.py:1362-1372`) are
//! the two exceptions — both call `_ensure_network_id(network_id)` with no `auto_discover`
//! argument at all, i.e. the default `True`, exactly like `get_network`/`get_eeros`/etc. from
//! earlier in the file. [`Client::get_profile_devices`] below passes `true` explicitly, with a
//! comment at the call site, specifically so a future reader does not "fix" it to match its
//! neighbours.
//!
//! # Explicit session vs. a configured store, at `build()` time
//!
//! Python has no direct precedent here: `EeroClient.__init__` never accepts a pre-built session,
//! only `session`/`cookie_file`/`use_keyring`, and always loads from storage once, in
//! `AuthAPI.__aenter__` (`auth.py:63-65`, brief-adjacent — see the `auth.md` brief). This port's
//! [`ClientBuilder`] additionally allows seeding a session directly
//! ([`ClientBuilder::session`], mirroring [`crate::transport::TransportBuilder::session`]), so a
//! choice has to be made when a caller sets both. [`ClientBuilder::build`] gives the explicit
//! session priority: a store is still installed on the resulting [`Client`] (so
//! [`Client::set_session_token`]/[`Client::clear_session_token`]/[`Client::logout`] still
//! persist through it), but it is only *read* — the equivalent of `AuthAPI._load_credentials`
//! (`auth.py:67-77`) — when [`ClientBuilder::session`] was never called. This is the more
//! predictable behaviour for a caller who explicitly injects a token (e.g. in a test), and does
//! not silently discard it in favour of whatever happens to already be on disk.

// This module holds every `Client` item that either has no single natural domain owner
// (construction, cache/preferred-network plumbing, network-id resolution, auth
// pass-throughs) or is tightly coupled to that plumbing (`get_account`, `get_networks`
// and their private helpers `apply_account_fallback`/`derive_preferred_network_id`).
// One `impl Client` block per remaining domain lives in its own file under this
// directory (`networks.rs`, `devices.rs`, `blacklist.rs`, `eeros.rs`, `profiles.rs`,
// `schedule.rs`, `dns.rs`, `security.rs`, `sqm.rs`, `backup.rs`, `burst_reporters.rs`,
// `data_usage.rs`, `diagnostics.rs`, `forwards.rs`, `insights.rs`, `ouicheck.rs`,
// `reservations.rs`, `routing.rs`, `support.rs`,
// `thread.rs`, `transfer.rs`, `updates.rs`, `ac_compat.rs`, plus the 14 new-in-v8.0.0 modules
// this phase scaffolds empty — `account.rs`, `backup_access_points.rs`, `ddns.rs`, `dhcp.rs`,
// `dns_policies.rs`, `entitlements.rs`, `events.rs`, `members.rs`, `notifications.rs`,
// `permissions.rs`, `power_saving.rs`, `subnets.rs`, `wan.rs`, `wpa3.rs`) — Rust's privacy rules make
// this transparent: a private item defined here (the `Client` fields, `ensure_network_id`,
// the three cache-invalidation helpers below) is visible to every one of those submodules
// without any `pub(crate)`/`pub(super)` change, because they are descendants of this module.
// `crate::client::{Client, ClientBuilder}` keeps resolving exactly as before either way.

use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use serde_json::{Value, json};

use crate::api::EeroApi;
use crate::auth::Session;
use crate::cache::{Cache, CacheKey};
use crate::consts;
use crate::envelope::Envelope;
use crate::error::Error;
use crate::storage::CredentialStore;
use crate::transport::{StorageFailures, Transport};

mod ac_compat;
mod account;
mod backup;
mod backup_access_points;
mod blacklist;
mod burst_reporters;
mod data_usage;
mod ddns;
mod devices;
mod dhcp;
mod diagnostics;
mod dns;
mod dns_policies;
mod eeros;
mod entitlements;
mod events;
mod forwards;
mod insights;
mod members;
mod networks;
mod notifications;
mod ouicheck;
mod permissions;
mod power_saving;
mod profiles;
mod reservations;
mod routing;
mod schedule;
mod security;
mod sqm;
mod subnets;
mod support;
mod thread;
mod transfer;
mod updates;
mod wan;
mod wpa3;

/// `eero-api`'s `EeroClient` (`src/eero/client.py`): [`EeroApi`] plus the client-only cache and
/// preferred-network state.
///
/// Build one with [`Client::builder`]. See the module docs for what is deliberately not
/// mirrored (`login`/`verify`) and for every divergence from Python this type introduces.
///
/// `#[derive(Debug)]` is safe here without a hand-written impl: [`EeroApi`] and [`Cache`] both
/// already hand-write their own redacting `Debug` impls (a cached [`Envelope`] can carry a Wi-Fi
/// password, so `Cache`'s own `Debug` prints only its keys — see that type's docs), and
/// `preferred_network_id` holds nothing more sensitive than a network id.
#[derive(Debug)]
pub struct Client {
    api: EeroApi,
    cache: Cache,
    /// `eero-api`'s `_preferred_network_id` (`client.py:53`): in-memory only, lost on restart,
    /// never synced with any API-layer state (brief §5) — see [`Client::set_preferred_network`].
    preferred_network_id: RwLock<Option<String>>,
}

impl Client {
    /// Starts building a `Client` with [`ClientBuilder`]'s defaults.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Borrows the underlying [`EeroApi`], for a caller that wants to bypass this `Client`'s
    /// cache and network-id resolution for a specific call — the same escape hatch Python
    /// offers by reaching for `self._api` directly.
    #[must_use]
    pub fn api(&self) -> &EeroApi {
        &self.api
    }

    /// Whether this client currently carries a valid, unexpired session.
    ///
    /// Ported from the `is_authenticated` property (`client.py:72-75`): a pure delegation to
    /// [`EeroApi::is_authenticated`], with no caching or client-side state of its own.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.api.is_authenticated()
    }

    /// Returns a snapshot of the currently configured session, if any.
    ///
    /// Not present on `EeroClient` itself, but listed as `Client`'s intended shape in
    /// `rust-port-plan.md` §3.7 (`EeroAPI.auth.get_auth_token()` → `Client::session() ->
    /// Option<Session>`): the token is a [`secrecy::SecretString`] wrapped in [`Session`], not a
    /// bare string, so this exposes the whole session rather than an unwrapped credential.
    #[must_use]
    pub fn session(&self) -> Option<Session> {
        self.api.auth().session()
    }

    /// Removes every cached entry.
    ///
    /// Ported from `clear_cache()` (`client.py:120-126`); see [`Cache::clear`] for the one
    /// documented improvement over Python this shares (clearing drops timestamps too, not just
    /// values).
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Sets the preferred network id used by `Client::ensure_network_id` when no explicit id
    /// is given.
    ///
    /// Ported from `set_preferred_network` (`client.py:791-800`): in-memory only. "For
    /// persistent storage, the CLI application should manage its own configuration file"
    /// (`client.py:794-795`) — lost on process restart, never written to disk, never synced with
    /// any API-layer state (brief §5).
    pub fn set_preferred_network(&self, network_id: impl Into<String>) {
        let mut guard = self
            .preferred_network_id
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        *guard = Some(network_id.into());
    }

    /// Returns the currently preferred network id, if one has been set explicitly (via
    /// [`Client::set_preferred_network`]) or derived automatically (the one-shot side effect of
    /// [`Client::get_networks`] — see that method's docs).
    ///
    /// Ported from the `preferred_network_id` property (`client.py:802-805`).
    #[must_use]
    pub fn preferred_network_id(&self) -> Option<String> {
        self.preferred_network_id
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Resolves a network id: explicit `network_id` → [`Client::preferred_network_id`] → (if
    /// `auto_discover`) the first network from [`Client::get_networks`] → [`Error::MissingNetworkId`].
    ///
    /// Ported from `_ensure_network_id` (`client.py:128-168`, brief §3):
    ///
    /// 1. `network_id`, but only if non-empty — an explicit empty string is treated exactly like
    ///    `None` (brief gotcha G10: Python's `network_id or self._preferred_network_id` is a
    ///    truthiness `or`, not an `is None` check).
    /// 2. [`Client::preferred_network_id`], same non-empty rule.
    /// 3. If `auto_discover`, calls [`Client::get_networks`] with `refresh_cache = false` —
    ///    deliberately, matching Python exactly (brief gotcha G2: a cached "zero networks"
    ///    response can leave auto-discovery failing for a full TTL window; this port reproduces
    ///    that known quirk rather than silently fixing it here) — and takes the first entry's
    ///    `id`, falling back to the trailing path segment of its `url` via
    ///    [`crate::util::id_from_url`] if `id` is absent or empty.
    /// 4. Otherwise, [`Error::MissingNetworkId`] (`client.py:168`'s bare `EeroException`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if none of the above resolves an id. Propagates whatever
    /// [`Client::get_networks`] itself returns when auto-discovery is attempted and fails.
    async fn ensure_network_id(
        &self,
        network_id: Option<&str>,
        auto_discover: bool,
    ) -> Result<String, Error> {
        if let Some(id) = non_empty(network_id) {
            return Ok(id.to_owned());
        }
        let preferred = self.preferred_network_id();
        if let Some(id) = non_empty(preferred.as_deref()) {
            return Ok(id.to_owned());
        }
        if auto_discover {
            let response = self.get_networks(false).await?;
            let networks = extract_networks_list(response.data());
            if let Some(id) = networks.first().and_then(extract_network_id) {
                return Ok(id);
            }
        }
        Err(Error::MissingNetworkId)
    }

    /// Logs the current session out and unconditionally clears this client's cache.
    ///
    /// Ported from `logout()` (`client.py:197-206`, `result = await self._api.logout(); if
    /// result: self.clear_cache()`), but **deliberately diverges** from that `if result:` gate
    /// (security review finding F1): [`EeroApi::logout`] already clears the in-memory session
    /// and the credential store unconditionally, on every outcome — see that method's own docs
    /// — precisely because the session is gone either way, win or lose, on the wire. A `Client`
    /// that only dropped its cache when the network call happened to succeed left every cached
    /// getter (`get_account`, `get_network`, `get_devices`, `get_profiles`, ...) free to keep
    /// serving the previous session's data for the rest of the TTL after a failed logout (a
    /// `429`, a `5xx`, a timeout), with `is_authenticated()` already `false` and no auth check of
    /// its own gating any of those reads. Clearing the cache unconditionally here — before either
    /// return arm — closes that gap and matches [`EeroApi::logout`]'s own contract: the cache and
    /// the session become stale at exactly the same moment, regardless of whether the request
    /// itself succeeded.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`EeroApi::logout`] returns. The cache is cleared regardless.
    pub async fn logout(&self) -> Result<bool, Error> {
        let result = self.api.logout().await;
        self.clear_cache();
        result
    }

    /// Seeds the active session with a pre-existing session token and clears this client's
    /// cache.
    ///
    /// Ported from `set_session_token()` (`client.py:208-225`): the cache is only cleared "if
    /// `auth.set_session_token` did not raise" (brief §1.6's table) — the `?` below reproduces
    /// that ordering exactly, since a `Validation` failure returns before [`Client::clear_cache`]
    /// is ever called.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "token", .. }` if `token` is empty. Returns
    /// `Error::Storage` if a configured credential store failed to persist the new session under
    /// [`StorageFailures::Fatal`] (see [`crate::auth::AuthApi::set_session_token`]'s own docs).
    pub fn set_session_token(&self, token: &str) -> Result<(), Error> {
        self.api.auth().set_session_token(token)?;
        self.clear_cache();
        Ok(())
    }

    /// Clears the active session token (keeping any refresh token) and clears this client's
    /// cache.
    ///
    /// Ported from `clear_session_token()` (`client.py:227-234`), which clears the cache
    /// unconditionally (`AuthAPI.clear_session_token` returns `None`, not a boolean to gate on —
    /// brief §1.6's table notes this is presumably why this call site, unlike `verify`/`logout`,
    /// is unconditional in Python).
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if a configured credential store failed to persist the change
    /// under [`StorageFailures::Fatal`] (see [`crate::auth::AuthApi::clear_session_token`]'s own
    /// docs). The in-memory session is cleared regardless; this client's cache is only cleared
    /// after that call returns `Ok`, mirroring Python's sequencing even though Python's own call
    /// can never itself fail.
    pub fn clear_session_token(&self) -> Result<(), Error> {
        self.api.auth().clear_session_token()?;
        self.clear_cache();
        Ok(())
    }

    // ============================= Cached getters (the eight) =============================
    //
    // Brief §1.4, §6: these are the *only* eight `EeroClient` methods that ever consult the
    // cache. Every one of them follows the exact two-part read guard [`Cache::get`] already
    // implements (TTL plus the falsy-value rule) and writes back unconditionally via
    // [`Cache::put`], regardless of whether the read was skipped by `refresh_cache`.

    /// Gets account information — returns the raw Eero API response.
    ///
    /// Ported from `get_account()` (`client.py:238-256`). Unlike Python (brief gotcha G12 — see
    /// the module docs), the underlying call
    /// ([`crate::endpoints::NetworksApi::get_account`]) already benefits from this crate's
    /// ordinary one-shot refresh-and-retry on a `401 error.session.refresh`.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_account(&self, refresh_cache: bool) -> Result<Envelope, Error> {
        if !refresh_cache && let Some(cached) = self.cache.get(&CacheKey::Account) {
            return Ok(cached);
        }
        let response = self.api.networks().get_account().await?;
        self.cache.put(CacheKey::Account, response.clone());
        Ok(response)
    }

    /// Gets the list of networks on the account — returns the raw Eero API response.
    ///
    /// Ported from `get_networks()` (`client.py:260-331`, brief §4). See the module docs for the
    /// two deliberate divergences in the `/account` fallback this method implements (the
    /// synthesised envelope is reproduced; the fallback's own failure is **not** silently
    /// swallowed) and for why this method's own call to [`Client::get_networks`] — via
    /// `Client::ensure_network_id`'s auto-discovery path — never passes `refresh_cache = true`
    /// (brief gotcha G2, reproduced faithfully).
    ///
    /// The one-shot `_preferred_network_id` side effect (`client.py:313-329`) is reproduced
    /// exactly: it only runs when [`Client::preferred_network_id`] is currently unset, and it is
    /// derived from the *final* (possibly synthesised) response, not the original `/networks`
    /// one.
    ///
    /// # Errors
    ///
    /// Returns whatever status-mapped [`Error`] the initial `/networks` request produces.
    /// Returns whatever [`Client::get_account`] produces if the `/networks` list came back empty
    /// and the `/account` fallback itself fails (see the module docs — this is a deliberate
    /// divergence from Python, which swallows that failure).
    pub async fn get_networks(&self, refresh_cache: bool) -> Result<Envelope, Error> {
        if !refresh_cache && let Some(cached) = self.cache.get(&CacheKey::Networks) {
            return Ok(cached);
        }

        let response = self.api.networks().get_networks().await?;
        let networks = extract_networks_list(response.data());

        let response = if networks.is_empty() {
            self.apply_account_fallback(response).await?
        } else {
            response
        };

        self.cache.put(CacheKey::Networks, response.clone());
        self.derive_preferred_network_id(&response);

        Ok(response)
    }

    /// Implements the `/account` fallback half of [`Client::get_networks`] (`client.py:289-309`).
    ///
    /// Called only when the `/networks` response's own network list came back empty. Forces a
    /// live `/account` fetch (`refresh_cache = true`, matching `client.py:292` exactly — this
    /// also refreshes the `account` cache bucket as a side effect, regardless of whether the
    /// caller needed it). If the account-derived list is non-empty, returns a synthesised
    /// envelope combining `original`'s `meta` with the account-derived `data.networks`
    /// (`client.py:302-307`, brief gotcha G1); otherwise returns `original` unchanged
    /// (`client.py:308-309`'s "no synthesis" path, reached whether the account list is also
    /// empty or — divergence from Python — propagated as an `Err` instead of silently continuing
    /// with `original` on a fetch failure).
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Client::get_account`] returns on failure. See the module docs'
    /// "brief gotchas G1, G6" section for why this is a deliberate divergence from Python, which
    /// never returns an `Err` from this path at all.
    async fn apply_account_fallback(&self, original: Envelope) -> Result<Envelope, Error> {
        let account_response = self.get_account(true).await?;
        let account_networks = extract_account_networks(account_response.data());
        if account_networks.is_empty() {
            return Ok(original);
        }
        let meta = original
            .as_value()
            .get("meta")
            .cloned()
            .unwrap_or_else(|| json!({}));
        Ok(Envelope::from_value(json!({
            "meta": meta,
            "data": {"networks": account_networks},
        })))
    }

    /// Implements the one-shot `_preferred_network_id` side effect of [`Client::get_networks`]
    /// (`client.py:313-329`): a no-op if a preferred network is already set, otherwise derives
    /// one from `response`'s (possibly synthesised) network list, using the same `id`-or-`url`
    /// extraction `Client::ensure_network_id`'s auto-discovery path uses. Silently leaves the
    /// preferred network unset if extraction yields nothing, exactly like Python.
    fn derive_preferred_network_id(&self, response: &Envelope) {
        if self.preferred_network_id().is_some() {
            return;
        }
        let networks = extract_networks_list(response.data());
        if let Some(id) = networks.first().and_then(extract_network_id) {
            self.set_preferred_network(id);
        }
    }

    // ============================= Parent-resolution helpers =============================
    //
    // Ported from `eero-api`'s four read-only, side-effect-free `_..._parent(_kwargs)` helpers
    // (`client.py:230-322`, `.claude/tasks/briefs/v8/client.md` §3.3): each looks up an already
    // cached envelope and hands it back unchanged for use as a domain method's `parent=`
    // argument, so a phase-G domain wrapper can prefer the server's own published `resources`
    // link over a hand-built template without an extra round trip. None of these mutate the
    // cache or trigger a network call; a miss (nothing cached, or a cached entry that is no
    // longer fresh) is simply `None`, mirroring Python's own `{}`-kwargs-omitted shape (this
    // port returns `Option<Value>` instead — the caller passes it straight through as
    // `parent: Option<&Value>`, the shape `crate::routes::Resource::resolve`/`Nested::resolve`
    // already expect).
    //
    // No call site yet within this crate as of this round: every phase-G domain wrapper that
    // will call these is still unwritten (see `.claude/tasks/briefs/v8/g*.md`). Exercised
    // directly by this module's own `#[cfg(test)] mod tests` in the meantime.

    /// Returns the cached network envelope for `network_id`, if a fresh entry exists.
    ///
    /// Ported from `_network_parent` (`eero-api src/eero/client.py:230-241`): looks up
    /// [`CacheKey::network`] and returns the whole cached envelope (`{"meta": .., "data": ..}`,
    /// not just its `data`) exactly when [`Cache::get`] still considers it fresh — never mutates
    /// the cache, never triggers a network call. Python additionally wraps this in
    /// `_network_parent_kwargs`, which turns `Some`/`None` into a `{"parent": ..}`/`{}` kwargs
    /// dict for `**kwargs`-unpacking into a domain call; this port has no keyword-unpacking
    /// equivalent, so callers pass this method's `Option<Value>` straight through as
    /// `parent: Option<&Value>` instead.
    #[allow(dead_code)] // see the "Parent-resolution helpers" banner above
    fn network_parent(&self, network_id: &str) -> Option<Value> {
        self.cache
            .get(&CacheKey::network(network_id))
            .map(Envelope::into_value)
    }

    /// Finds the entry in `envelopes` whose `id` field, or whose `url` field's trailing path
    /// segment, matches `resource_id` exactly.
    ///
    /// Ported from the `@staticmethod` `_find_by_id_or_url`
    /// (`eero-api src/eero/client.py:259-282`). `resource_id` is compared verbatim against each
    /// entry's `id` field first; on a miss, against the trailing path segment of that entry's own
    /// `url` field, extracted via [`crate::util::id_from_url`] — a `url` for which `id_from_url`
    /// itself errors (e.g. empty, or composed entirely of slashes) is simply never a match,
    /// exactly like Python's own `isinstance(url, str)` guard skips a non-string `url`. Entries
    /// that are not JSON objects are skipped, mirroring Python's `isinstance(entry, dict)` guard.
    /// Never mutates `envelopes`; returns a clone of the matching entry, if any.
    #[allow(dead_code)] // see the "Parent-resolution helpers" banner above
    fn find_by_id_or_url(envelopes: &[Value], resource_id: &str) -> Option<Value> {
        envelopes
            .iter()
            .find(|entry| {
                let Some(obj) = entry.as_object() else {
                    return false;
                };
                if obj.get("id").and_then(Value::as_str) == Some(resource_id) {
                    return true;
                }
                obj.get("url")
                    .and_then(Value::as_str)
                    .and_then(|url| crate::util::id_from_url(url).ok())
                    .is_some_and(|tail| tail == resource_id)
            })
            .cloned()
    }

    /// Returns the cached eero entry for `eero_id` within `network_id`'s eeros list, if fresh.
    ///
    /// Ported from `_eero_parent_kwargs` (`eero-api src/eero/client.py:284-305`): looks up
    /// [`CacheKey::eeros`] for `network_id`, and — only when that entry is still fresh — reads
    /// its envelope's `data` field as a JSON array (a non-array `data`, like Python's own
    /// `isinstance(cached, dict)`/list check, yields `None` rather than a match) and searches it
    /// via [`Client::find_by_id_or_url`].
    #[allow(dead_code)] // see the "Parent-resolution helpers" banner above
    fn eero_parent(&self, network_id: &str, eero_id: &str) -> Option<Value> {
        let cached = self.cache.get(&CacheKey::eeros(network_id))?;
        let data = cached.data().as_array()?;
        Self::find_by_id_or_url(data, eero_id)
    }

    /// Returns the cached single-device envelope for `device_id` within `network_id`, if fresh.
    ///
    /// Ported from `_device_parent_kwargs` (`eero-api src/eero/client.py:307-322`): looks up
    /// [`CacheKey::device`] and returns the whole cached envelope — not just its `data` — exactly
    /// when it is still fresh.
    #[allow(dead_code)] // see the "Parent-resolution helpers" banner above
    fn device_parent(&self, network_id: &str, device_id: &str) -> Option<Value> {
        self.cache
            .get(&CacheKey::device(network_id, device_id))
            .map(Envelope::into_value)
    }

    // ============================= Cache-invalidation helpers =============================
    //
    // Mirror Python's own private helpers (`client.py:567-575, 659-667, 669-673`) exactly: each
    // drops precisely the keys Python drops, no more. Where a deliberate `network[nid]`/
    // `eeros[{nid}_eeros]` drop is added on top of Python (the two documented improvements above),
    // that extra call is made directly at the write site rather than folded into a helper here —
    // there is no Python helper for it to mirror.

    /// Drops `network[{nid}]` for one network.
    ///
    /// No single Python helper mirrors this one — `client.py` deletes
    /// `self._cache["network"][network_id]` inline at each setter call site — but it is added
    /// here for the same reason [`Client::invalidate_device_cache`] exists: a single call site
    /// for every phase-G network-scoped setter to invalidate through, instead of each one
    /// repeating `self.cache.invalidate(&CacheKey::network(..))` by hand.
    #[allow(dead_code)] // no call site yet; see the "Parent-resolution helpers" banner above
    fn invalidate_network_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::network(network_id));
    }

    /// Drops `eeros[{nid}_eeros]` for one network.
    ///
    /// No single Python helper mirrors this one either — every `client.py` eeros setter deletes
    /// `self._cache["eeros"][f"{network_id}_eeros"]` inline — added for the same reason as
    /// [`Client::invalidate_network_cache`] just above. [`Client::reboot_eero`]/
    /// [`Client::set_led`]/[`Client::set_led_brightness`]/[`Client::set_nightlight`] etc.
    /// currently call `self.cache.invalidate(&CacheKey::eeros(..))` inline rather than through
    /// this helper; a future cleanup pass may switch them over, but that is out of scope here.
    #[allow(dead_code)] // no call site yet; see the "Parent-resolution helpers" banner above
    fn invalidate_eeros_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::eeros(network_id));
    }

    /// Drops both `devices[{nid}_{did}]` and `devices[{nid}_devices]` for one device.
    ///
    /// Mirrors `_invalidate_device_cache` (`eero-api src/eero/client.py:567-575`).
    fn invalidate_device_cache(&self, network_id: &str, device_id: &str) {
        self.cache
            .invalidate(&CacheKey::device(network_id, device_id));
        self.cache.invalidate(&CacheKey::devices(network_id));
    }

    /// Drops both `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` for one profile.
    ///
    /// Mirrors `_invalidate_profile_cache` (`eero-api src/eero/client.py:659-667`), which already
    /// drops both keys itself — see `Client::rename_profile`'s docs for why call sites that call
    /// both `_invalidate_profile_cache` and `_invalidate_profiles_list_cache` in Python only need
    /// this one helper here.
    fn invalidate_profile_cache(&self, network_id: &str, profile_id: &str) {
        self.cache
            .invalidate(&CacheKey::profile(network_id, profile_id));
        self.cache.invalidate(&CacheKey::profiles(network_id));
    }

    /// Drops only `profiles[{nid}_profiles]` — used by `Client::create_profile`, the one write
    /// with no single-profile key to also drop yet.
    ///
    /// Mirrors `_invalidate_profiles_list_cache` (`eero-api src/eero/client.py:669-673`).
    fn invalidate_profiles_list_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::profiles(network_id));
    }
}

/// Extracts a JSON array of network entries from a `{meta, data}` envelope's `data` value.
///
/// Shared by [`Client::get_networks`] (the initial `/networks` response), `Client::ensure_network_id`'s
/// auto-discovery step, and [`Client::derive_preferred_network_id`] — the same extraction
/// pattern Python duplicates verbatim at three call sites (`client.py:151-157`, `:281-286`,
/// `:316-321`; brief gotcha G11). If `data` is itself a JSON array, it is used directly; if
/// `data` is a JSON object, its `networks` key is preferred when it holds a non-empty array,
/// falling back to its `data` key under the same condition (mirroring Python's `data.get(
/// "networks") or data.get("data") or []`, a truthiness `or` chain — an empty array is treated
/// exactly like a missing key); any other shape yields an empty list.
fn extract_networks_list(data: &Value) -> Vec<Value> {
    match data {
        Value::Array(items) => items.clone(),
        Value::Object(map) => truthy_array(map.get("networks"))
            .or_else(|| truthy_array(map.get("data")))
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Returns `value` as a non-empty JSON array, or `None` for anything else (missing, `null`, a
/// non-array shape, or a present-but-empty array) — the "truthy" half of `extract_networks_list`'s
/// `or`-chain.
fn truthy_array(value: Option<&Value>) -> Option<&Vec<Value>> {
    match value {
        Some(Value::Array(items)) if !items.is_empty() => Some(items),
        _ => None,
    }
}

/// Extracts a network entry's id: prefers a non-empty `id` field, falling back to the trailing
/// path segment of a non-empty `url` field via [`crate::util::id_from_url`].
///
/// Shared by `Client::ensure_network_id`'s auto-discovery step and
/// [`Client::derive_preferred_network_id`] — the same four-line pattern Python duplicates at two
/// call sites (`client.py:161-166`, `:324-329`; brief gotcha G11). Built on
/// [`crate::util::id_from_url`] rather than re-deriving the trailing-segment logic a third time,
/// unlike Python, which has no shared helper for it at all.
///
/// **Security review finding F3.** Both call sites above feed this function's return value
/// straight into a request URL — `ensure_network_id`'s result becomes `{network_id}` in every
/// subsequent mutation this `Client` issues, and `derive_preferred_network_id` latches it as the
/// sticky [`Client::preferred_network_id`] for the rest of this client's lifetime — so a
/// candidate that would trip [`crate::routes::validate_segment`] (empty, an ASCII control
/// character, or a bare `"."`/`".."` once tab/CR/LF are stripped) is rejected here, at the one
/// place a *server-derived* value (a hostile or buggy `/networks` response) can reach a request
/// URL, rather than trusted through to `Route::render`/`Transport::render_url`'s own guard on
/// every call after this one. Applies to both extraction paths, not just the `url` fallback:
/// nothing stops a malicious `id` field from carrying the same value directly.
fn extract_network_id(entry: &Value) -> Option<String> {
    if let Some(id) = entry
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .filter(|id| crate::routes::validate_segment(id).is_ok())
    {
        return Some(id.to_owned());
    }
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    let candidate = crate::util::id_from_url(url).ok()?;
    crate::routes::validate_segment(&candidate).ok()?;
    Some(candidate)
}

/// Extracts the network list from a [`Client::get_account`] envelope's `data.networks` field.
///
/// Distinct from [`extract_networks_list`] (which handles the `/networks` endpoint's own
/// response shape): mirrors `client.py:293-300`'s account-specific extraction exactly —
/// `data.networks` may itself be a JSON object whose own `data` key holds the array, or may be
/// the array directly. No truthiness `or`-chain here; Python uses plain `isinstance` checks with
/// no fallback between the two shapes.
fn extract_account_networks(data: &Value) -> Vec<Value> {
    let Some(account_data) = data.as_object() else {
        return Vec::new();
    };
    match account_data.get("networks") {
        Some(Value::Object(networks_data)) => networks_data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// Returns `value` with empty-string treated as absent, matching Python's truthiness `or`
/// (brief gotcha G10: an explicit `network_id=""` is treated identically to `None`).
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|s| !s.is_empty())
}

/// Builds a [`Client`].
///
/// Every setter takes `self` by value and returns `Self`, so calls chain naturally, matching
/// [`crate::transport::TransportBuilder`]'s own shape — [`ClientBuilder::build`] is the only
/// fallible (and only `async`) step. See each setter's docs for its default when unset, and the
/// module docs for how an explicit [`ClientBuilder::session`] interacts with a configured
/// [`ClientBuilder::store`].
#[derive(Debug)]
pub struct ClientBuilder {
    http: Option<reqwest::ClientBuilder>,
    base_url: Option<String>,
    user_agent: Option<String>,
    accept_language: Option<String>,
    send_legacy_cookie: Option<bool>,
    get_retries: Option<u32>,
    session: Option<Session>,
    store: Option<Arc<dyn CredentialStore>>,
    storage_failures: StorageFailures,
    cache_ttl: Duration,
}

impl Default for ClientBuilder {
    /// `cache_ttl` defaults to [`crate::consts::DEFAULT_CACHE_TTL`] (60 seconds, matching
    /// `client.py:41`'s `cache_timeout: int = 60`) — **not** `Duration::default()` (zero).
    /// [`Duration::ZERO`] disables cache reads (see [`Cache`]'s own docs), so it must stay
    /// distinguishable from "never called [`ClientBuilder::cache_ttl`] at all"; deriving
    /// `Default` here would collapse the two and silently disable caching for every `Client`
    /// built without an explicit `.cache_ttl(..)` call.
    fn default() -> Self {
        Self {
            http: None,
            base_url: None,
            user_agent: None,
            accept_language: None,
            send_legacy_cookie: None,
            get_retries: None,
            session: None,
            store: None,
            storage_failures: StorageFailures::default(),
            cache_ttl: consts::DEFAULT_CACHE_TTL,
        }
    }
}

impl ClientBuilder {
    /// Supplies a `reqwest::ClientBuilder` instead of letting [`ClientBuilder::build`] construct
    /// one from scratch. Forwarded to [`crate::transport::TransportBuilder::http_builder`] — see
    /// that method's docs: the redirect policy is always forced to
    /// `reqwest::redirect::Policy::none()` regardless of what `builder` carries, and this crate's
    /// own timeouts are skipped unless `builder` sets its own.
    #[must_use]
    pub fn http_builder(mut self, builder: reqwest::ClientBuilder) -> Self {
        self.http = Some(builder);
        self
    }

    /// Overrides the base host for both API versions. Forwarded to
    /// [`crate::transport::TransportBuilder::base_url`].
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Sets the `User-Agent` header reqwest sends. Forwarded to
    /// [`crate::transport::TransportBuilder::user_agent`].
    #[must_use]
    pub fn user_agent(mut self, user_agent: Option<String>) -> Self {
        self.user_agent = user_agent;
        self
    }

    /// Sets the `X-Accept-Language` header every request carries. Forwarded to
    /// [`crate::transport::TransportBuilder::accept_language`] — see that method's docs for the
    /// validation it applies (printable ASCII, no CR/LF) and its default
    /// ([`crate::consts::DEFAULT_ACCEPT_LANGUAGE`]) when never called.
    ///
    /// Ported from `EeroClient.__init__`'s `accept_language` keyword-only parameter
    /// (`client.py:49-59`, forwarded to `EeroAPI` at `client.py:80-87`;
    /// `.claude/tasks/briefs/v8/client.md` §1.1/§1.3).
    #[must_use]
    pub fn accept_language(mut self, accept_language: impl Into<String>) -> Self {
        self.accept_language = Some(accept_language.into());
        self
    }

    /// Whether to also send the session as the legacy `Cookie: s=<token>` header alongside the
    /// primary `X-User-Token` header, on every request. Forwarded to
    /// [`crate::transport::TransportBuilder::send_legacy_cookie`] — defaults to `true` when never
    /// called, matching Python's own default.
    ///
    /// Ported from `EeroClient.__init__`'s `send_legacy_cookie` keyword-only parameter
    /// (`client.py:49-59`, forwarded to `EeroAPI` at `client.py:80-87`;
    /// `.claude/tasks/briefs/v8/client.md` §1.1/§1.3).
    #[must_use]
    pub fn send_legacy_cookie(mut self, send_legacy_cookie: bool) -> Self {
        self.send_legacy_cookie = Some(send_legacy_cookie);
        self
    }

    /// Sets how many additional attempts a `GET` request gets on a transport error or a `5xx`
    /// response. Forwarded to [`crate::transport::TransportBuilder::get_retries`] — defaults to
    /// `0` (no retries) when never called, matching Python's own default. Never applies to
    /// writes, and never affects the separate one-shot 401 refresh-and-replay.
    ///
    /// Ported from `EeroClient.__init__`'s `get_retries` keyword-only parameter
    /// (`client.py:49-59`, forwarded to `EeroAPI` at `client.py:80-87`;
    /// `.claude/tasks/briefs/v8/client.md` §1.1/§1.3).
    #[must_use]
    pub fn get_retries(mut self, get_retries: u32) -> Self {
        self.get_retries = Some(get_retries);
        self
    }

    /// Seeds the client with an initial session, bypassing whatever a configured
    /// [`ClientBuilder::store`] would otherwise load — see the module docs' "explicit session
    /// vs. a configured store" section for why this takes priority.
    #[must_use]
    pub fn session(mut self, session: Option<Session>) -> Self {
        self.session = session;
        self
    }

    /// Sets the credential store this client persists session changes through, and — when
    /// [`ClientBuilder::session`] was never called — the store [`ClientBuilder::build`] loads
    /// the initial session from (the equivalent of `AuthAPI._load_credentials`,
    /// `auth.py:67-77`, called once by Python's `__aenter__`, which this crate has no equivalent
    /// of — see the module docs' "no `login`/`verify`" section).
    #[must_use]
    pub fn store(mut self, store: Option<Arc<dyn CredentialStore>>) -> Self {
        self.store = store;
        self
    }

    /// Sets this client's policy for a failed credential-store operation, forwarded to
    /// [`crate::transport::TransportBuilder::storage_failures`] and also applied to the initial
    /// load [`ClientBuilder::build`] performs when a store is configured and no explicit
    /// [`ClientBuilder::session`] was given (decision D-13).
    #[must_use]
    pub fn storage_failures(mut self, storage_failures: StorageFailures) -> Self {
        self.storage_failures = storage_failures;
        self
    }

    /// Sets the cache TTL used by every cached getter (`consts::DEFAULT_CACHE_TTL`, 60 seconds,
    /// by default — matching `client.py:41`'s `cache_timeout: int = 60`).
    /// [`Duration::ZERO`] disables reads without disabling writes; see [`Cache`]'s own docs.
    #[must_use]
    pub fn cache_ttl(mut self, cache_ttl: Duration) -> Self {
        self.cache_ttl = cache_ttl;
        self
    }

    /// Builds the `Client`.
    ///
    /// If [`ClientBuilder::session`] was never called and a [`ClientBuilder::store`] is
    /// configured, this performs the one-time credential load Python's `AuthAPI.__aenter__`
    /// performs (`auth.py:63-77`) — via the same `spawn_blocking`-backed
    /// `storage::load_async` adapter [`crate::transport::Transport`]'s own persistence
    /// paths use — before the underlying [`Transport`] is constructed.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if that initial load fails under
    /// [`StorageFailures::Fatal`] (under the default [`StorageFailures::Warn`], a load failure
    /// is logged at `WARN` and this proceeds with no initial session instead). Returns
    /// `Error::Validation` if `base_url` was set to a string that does not parse as an absolute
    /// URL usable as a path base, or `Error::Network` if constructing the underlying
    /// `reqwest::Client` fails (see [`crate::transport::TransportBuilder::build`]).
    pub async fn build(self) -> Result<Client, Error> {
        let session = match self.session {
            Some(session) => Some(session),
            None => match &self.store {
                Some(store) => match crate::storage::load_async(Arc::clone(store)).await {
                    Ok(loaded) => Some(loaded),
                    Err(err) => match self.storage_failures {
                        StorageFailures::Warn => {
                            tracing::warn!(
                                error = %err,
                                "failed to load initial session from credential store"
                            );
                            None
                        }
                        StorageFailures::Fatal => return Err(Error::Storage(err)),
                    },
                },
                None => None,
            },
        };

        let mut transport_builder = Transport::builder()
            .storage_failures(self.storage_failures)
            .user_agent(self.user_agent)
            .session(session)
            .store(self.store);

        if let Some(http) = self.http {
            transport_builder = transport_builder.http_builder(http);
        }
        if let Some(base_url) = self.base_url {
            transport_builder = transport_builder.base_url(base_url);
        }
        if let Some(accept_language) = self.accept_language {
            transport_builder = transport_builder.accept_language(accept_language);
        }
        if let Some(send_legacy_cookie) = self.send_legacy_cookie {
            transport_builder = transport_builder.send_legacy_cookie(send_legacy_cookie);
        }
        if let Some(get_retries) = self.get_retries {
            transport_builder = transport_builder.get_retries(get_retries);
        }

        let transport = transport_builder.build()?;
        let api = EeroApi::new(transport);

        Ok(Client {
            api,
            cache: Cache::new(self.cache_ttl),
            preferred_network_id: RwLock::new(None),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure, network-free helper functions this module builds on
    //! ([`extract_networks_list`], [`extract_network_id`], [`extract_account_networks`],
    //! [`non_empty`]), plus the four private parent-resolution helpers
    //! ([`Client::network_parent`], [`Client::find_by_id_or_url`], [`Client::eero_parent`],
    //! [`Client::device_parent`]) — private, so (unlike every HTTP-level suite in this crate)
    //! they are exercised here, directly, rather than through `tests/client_core.rs`; see
    //! [`test_client`] for how a bare `Client` with a pre-populated [`Cache`] is built for that
    //! purpose. HTTP-level behaviour (cache hits, the `/account` fallback end to end,
    //! network-id resolution against a live mock, `ClientBuilder::build`'s credential-store load)
    //! is covered by `tests/client_core.rs`, per the crate's testing conventions.

    use super::{
        Cache, CacheKey, Client, EeroApi, Envelope, extract_account_networks, extract_network_id,
        extract_networks_list, non_empty,
    };
    use crate::transport::Transport;
    use serde_json::json;
    use std::sync::RwLock;
    use std::time::Duration;

    /// Builds a bare [`Client`] with no session and no credential store — just enough to exercise
    /// the parent-resolution helpers, which only ever read [`Client::cache`] and never touch the
    /// network. A test populates the cache directly via `client.cache.put(..)` before calling the
    /// helper under test.
    fn test_client() -> Client {
        let transport = Transport::builder()
            .build()
            .expect("default transport configuration (no overrides) always builds successfully");
        Client {
            api: EeroApi::new(transport),
            cache: Cache::new(Duration::from_secs(60)),
            preferred_network_id: RwLock::new(None),
        }
    }

    // ===================== extract_networks_list =====================

    #[test]
    fn extract_networks_list_from_a_bare_array() {
        let data = json!([{"id": "1"}, {"id": "2"}]);
        assert_eq!(
            extract_networks_list(&data),
            vec![json!({"id": "1"}), json!({"id": "2"})]
        );
    }

    #[test]
    fn extract_networks_list_prefers_the_networks_key() {
        let data = json!({"networks": [{"id": "n1"}], "data": [{"id": "should-not-be-used"}]});
        assert_eq!(extract_networks_list(&data), vec![json!({"id": "n1"})]);
    }

    #[test]
    fn extract_networks_list_falls_back_to_the_data_key_when_networks_is_empty() {
        let data = json!({"networks": [], "data": [{"id": "fallback"}]});
        assert_eq!(
            extract_networks_list(&data),
            vec![json!({"id": "fallback"})]
        );
    }

    #[test]
    fn extract_networks_list_falls_back_to_the_data_key_when_networks_is_absent() {
        let data = json!({"data": [{"id": "fallback"}]});
        assert_eq!(
            extract_networks_list(&data),
            vec![json!({"id": "fallback"})]
        );
    }

    #[test]
    fn extract_networks_list_is_empty_when_neither_key_holds_a_non_empty_array() {
        assert!(extract_networks_list(&json!({})).is_empty());
        assert!(extract_networks_list(&json!({"networks": [], "data": []})).is_empty());
        assert!(extract_networks_list(&json!(null)).is_empty());
        assert!(extract_networks_list(&json!("not an object or array")).is_empty());
    }

    // ===================== extract_network_id =====================

    #[test]
    fn extract_network_id_prefers_the_id_field() {
        let entry = json!({"id": "abc123", "url": "/2.2/networks/999"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_network_id_falls_back_to_the_url_tail_when_id_is_absent() {
        let entry = json!({"url": "/2.2/networks/abc123"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_network_id_falls_back_to_the_url_tail_when_id_is_empty() {
        let entry = json!({"id": "", "url": "/2.2/networks/abc123/"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_network_id_is_none_when_neither_field_is_usable() {
        assert!(extract_network_id(&json!({})).is_none());
        assert!(extract_network_id(&json!({"id": "", "url": ""})).is_none());
    }

    // ===================== extract_network_id: security finding F3 =====================
    //
    // A hostile or buggy `/networks` response must not hand back a `".."`/`"."`/empty network
    // id that later collapses a per-item route onto its collection — see this function's own
    // doc comment. Both extraction paths (`id` field, `url`-tail fallback) are covered.

    #[test]
    fn extract_network_id_rejects_a_dot_segment_id_field_and_falls_back_to_url() {
        let entry = json!({"id": "..", "url": "/2.2/networks/abc123"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_network_id_rejects_a_dot_segment_id_field_with_no_usable_url() {
        assert!(extract_network_id(&json!({"id": ".."})).is_none());
        assert!(extract_network_id(&json!({"id": "."})).is_none());
    }

    #[test]
    fn extract_network_id_rejects_a_url_tail_that_resolves_to_a_dot_segment() {
        // `id_from_url` extracts the trailing segment verbatim; a `/networks/..` url resolves
        // to the literal id `".."`, which must be rejected rather than handed back as a usable
        // network id.
        assert!(extract_network_id(&json!({"url": "/2.2/networks/.."})).is_none());
    }

    #[test]
    fn extract_network_id_rejects_a_url_that_resolves_to_an_empty_segment() {
        // `id_from_url("/")` already fails (util's own empty-segment guard), so this exercises
        // this function's `.ok()?` short-circuit on that error, not `validate_segment` itself.
        assert!(extract_network_id(&json!({"url": "/"})).is_none());
    }

    // ===================== extract_account_networks =====================

    #[test]
    fn extract_account_networks_from_a_nested_data_object() {
        let data = json!({"networks": {"data": [{"id": "n1"}]}});
        assert_eq!(extract_account_networks(&data), vec![json!({"id": "n1"})]);
    }

    #[test]
    fn extract_account_networks_from_a_bare_array() {
        let data = json!({"networks": [{"id": "n1"}]});
        assert_eq!(extract_account_networks(&data), vec![json!({"id": "n1"})]);
    }

    #[test]
    fn extract_account_networks_is_empty_for_any_other_shape() {
        assert!(extract_account_networks(&json!({})).is_empty());
        assert!(extract_account_networks(&json!({"networks": null})).is_empty());
        assert!(extract_account_networks(&json!({"networks": {}})).is_empty());
        assert!(extract_account_networks(&json!(null)).is_empty());
    }

    // ===================== non_empty =====================

    #[test]
    fn non_empty_treats_an_empty_string_like_none() {
        assert_eq!(non_empty(Some("")), None);
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("abc")), Some("abc"));
    }

    // ===================== network_parent =====================
    //
    // Ported from `test_client.py::TestClientParentResolutionHelpers` (v8.0.4, ~lines
    // 1264-1345): empty cache -> None; cached + fresh -> Some; cached + expired -> None.

    #[test]
    fn network_parent_is_none_on_an_empty_cache() {
        let client = test_client();
        assert_eq!(client.network_parent("net1"), None);
    }

    #[test]
    fn network_parent_returns_the_whole_cached_envelope_when_fresh() {
        let client = test_client();
        let envelope = json!({"meta": {"code": 200}, "data": {"name": "Home"}});
        client.cache.put(
            CacheKey::network("net1"),
            Envelope::from_value(envelope.clone()),
        );
        assert_eq!(client.network_parent("net1"), Some(envelope));
    }

    #[test]
    fn network_parent_is_none_once_the_entry_has_expired() {
        let client = Client {
            api: EeroApi::new(
                Transport::builder()
                    .build()
                    .expect("default transport configuration always builds successfully"),
            ),
            cache: Cache::new(Duration::ZERO), // ZERO disables reads without disabling writes
            preferred_network_id: RwLock::new(None),
        };
        client.cache.put(
            CacheKey::network("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"name": "Home"}})),
        );
        assert_eq!(client.network_parent("net1"), None);
    }

    #[test]
    fn network_parent_is_scoped_to_its_own_network_id() {
        let client = test_client();
        client.cache.put(
            CacheKey::network("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"name": "net1"}})),
        );
        assert_eq!(client.network_parent("net2"), None);
    }

    // ===================== find_by_id_or_url =====================

    #[test]
    fn find_by_id_or_url_matches_the_bare_id_field() {
        let envelopes = [json!({"id": "e1", "url": "/2.2/eeros/other"})];
        assert_eq!(
            Client::find_by_id_or_url(&envelopes, "e1"),
            Some(envelopes[0].clone())
        );
    }

    #[test]
    fn find_by_id_or_url_falls_back_to_the_url_trailing_segment() {
        let envelopes = [json!({"url": "/2.2/eeros/e1"})];
        assert_eq!(
            Client::find_by_id_or_url(&envelopes, "e1"),
            Some(envelopes[0].clone())
        );
    }

    #[test]
    fn find_by_id_or_url_returns_none_when_nothing_matches() {
        let envelopes = [
            json!({"id": "e1", "url": "/2.2/eeros/e1"}),
            json!({"id": "e2", "url": "/2.2/eeros/e2"}),
        ];
        assert_eq!(Client::find_by_id_or_url(&envelopes, "e3"), None);
    }

    #[test]
    fn find_by_id_or_url_skips_non_object_entries() {
        let envelopes = [json!("not an object"), json!({"id": "e1"})];
        assert_eq!(
            Client::find_by_id_or_url(&envelopes, "e1"),
            Some(json!({"id": "e1"}))
        );
    }

    #[test]
    fn find_by_id_or_url_ignores_a_url_that_id_from_url_itself_rejects() {
        // A `url` field that is empty, or composed entirely of slashes, makes `id_from_url` (and
        // therefore this function's url-tail comparison) fail — such an entry must never match,
        // not even when `resource_id` happens to also be an empty string.
        let envelopes = [json!({"url": "/"})];
        assert_eq!(Client::find_by_id_or_url(&envelopes, ""), None);
    }

    // ===================== eero_parent =====================

    #[test]
    fn eero_parent_is_none_on_an_empty_cache() {
        let client = test_client();
        assert_eq!(client.eero_parent("net1", "eero1"), None);
    }

    #[test]
    fn eero_parent_finds_the_matching_entry_in_the_cached_eeros_list() {
        let client = test_client();
        let eero_entry = json!({"id": "eero1", "url": "/2.2/eeros/eero1"});
        client.cache.put(
            CacheKey::eeros("net1"),
            Envelope::from_value(json!({
                "meta": {"code": 200},
                "data": [eero_entry.clone(), {"id": "eero2"}],
            })),
        );
        assert_eq!(client.eero_parent("net1", "eero1"), Some(eero_entry));
    }

    #[test]
    fn eero_parent_is_none_when_the_id_is_not_in_the_cached_list() {
        let client = test_client();
        client.cache.put(
            CacheKey::eeros("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": [{"id": "eero2"}]})),
        );
        assert_eq!(client.eero_parent("net1", "eero1"), None);
    }

    #[test]
    fn eero_parent_is_none_when_the_cached_datas_shape_is_not_an_array() {
        let client = test_client();
        client.cache.put(
            CacheKey::eeros("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {}})),
        );
        assert_eq!(client.eero_parent("net1", "eero1"), None);
    }

    #[test]
    fn eero_parent_is_none_once_the_entry_has_expired() {
        let client = Client {
            api: EeroApi::new(
                Transport::builder()
                    .build()
                    .expect("default transport configuration always builds successfully"),
            ),
            cache: Cache::new(Duration::ZERO),
            preferred_network_id: RwLock::new(None),
        };
        client.cache.put(
            CacheKey::eeros("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": [{"id": "eero1"}]})),
        );
        assert_eq!(client.eero_parent("net1", "eero1"), None);
    }

    // ===================== device_parent =====================

    #[test]
    fn device_parent_is_none_on_an_empty_cache() {
        let client = test_client();
        assert_eq!(client.device_parent("net1", "dev1"), None);
    }

    #[test]
    fn device_parent_returns_the_whole_cached_envelope_when_fresh() {
        let client = test_client();
        let envelope = json!({"meta": {"code": 200}, "data": {"mac": "dev1"}});
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(envelope.clone()),
        );
        assert_eq!(client.device_parent("net1", "dev1"), Some(envelope));
    }

    #[test]
    fn device_parent_is_scoped_to_its_own_device_id() {
        let client = test_client();
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "dev1"}})),
        );
        assert_eq!(client.device_parent("net1", "dev2"), None);
    }

    #[test]
    fn device_parent_is_none_once_the_entry_has_expired() {
        let client = Client {
            api: EeroApi::new(
                Transport::builder()
                    .build()
                    .expect("default transport configuration always builds successfully"),
            ),
            cache: Cache::new(Duration::ZERO),
            preferred_network_id: RwLock::new(None),
        };
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "dev1"}})),
        );
        assert_eq!(client.device_parent("net1", "dev1"), None);
    }

    // ===================== invalidate_network_cache / invalidate_eeros_cache =====================

    #[test]
    fn invalidate_network_cache_drops_only_the_network_entry() {
        let client = test_client();
        client.cache.put(
            CacheKey::network("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {}})),
        );
        client.invalidate_network_cache("net1");
        assert_eq!(client.cache.get(&CacheKey::network("net1")), None);
    }

    #[test]
    fn invalidate_eeros_cache_drops_only_the_eeros_list_entry() {
        let client = test_client();
        client.cache.put(
            CacheKey::eeros("net1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": []})),
        );
        client.invalidate_eeros_cache("net1");
        assert_eq!(client.cache.get(&CacheKey::eeros("net1")), None);
    }
}
