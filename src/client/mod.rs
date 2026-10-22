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
//! .store(st).build()`. Python clears the cache and forgets an auto-discovered network after
//! `verify` and forgets the network after `login` (`client.py:472-503` at `v8.4.1`); a login flow
//! here cannot change any `Client`'s session behind its back, so there is nothing to wire that
//! to. The ways into a new session — a freshly built `Client` (empty cache, no preferred
//! network) or [`Client::set_session_token`] — reset that state themselves. Any future
//! `Client`-level login helper must forget a discovered network on success, and any
//! verification helper must do what [`Client::set_session_token`] does.
//!
//! # Session changes reset the cache and a discovered network
//!
//! [`Client::set_session_token`], [`Client::clear_session_token`] and [`Client::logout`] empty
//! the cache and forget a preferred network that auto-discovery chose (`_reset_session_state`,
//! `client.py:457-470` at `v8.4.1`); a network the caller chose with
//! [`Client::set_preferred_network`] survives. [`Client::clear_auth_data`] drops even that.
//! The next call without a `network_id` then rediscovers the new account's network instead of
//! targeting the previous account's. A read that was already in flight when the session changed
//! is handed to its caller but cannot refill the cache (see the module docs of
//! [`crate::cache`]), and cannot latch the previous account's network as the new preferred one.
//!
//! # `get_account` goes through `AccountApi` (parity with `v8.4.1`)
//!
//! [`Client::get_account`] calls [`crate::endpoints::account::AccountApi::get_account`], as
//! `EeroClient.get_account` calls `self._api.account.get_account()` (`client.py:581-598` at
//! `v8.4.1`). That request takes the ordinary [`crate::transport::Transport::resource`] path, so
//! a `401 error.session.refresh` is refreshed once and replayed, the replayed envelope is the one
//! that gets cached, and a client with no session fails with `Not authenticated` before any
//! request and caches nothing.
//!
//! # The `/account` fallback
//!
//! [`Client::get_networks`] reproduces Python's `/account` fallback (`client.py:602-695` at
//! `v8.4.1`):
//!
//! - **Synthesised envelope**: when the fallback succeeds and finds networks, this crate — like
//!   Python — returns an envelope whose `data` was never actually returned by a single server
//!   response (`meta` comes from the original `/networks` call, `data.networks` comes from
//!   `/account`). This contradicts the crate's usual "never transform the wire payload" rule, but
//!   is ported anyway: downstream tools rely on `get_networks()` returning a non-empty list
//!   whenever the account actually has networks, and Python's own behaviour is the only
//!   precedent for what that synthesised shape should look like. This is the **one** documented
//!   exception to "every endpoint method returns the envelope unmodified" in this crate.
//! - **A failing fallback propagates.** If the `/networks` list is empty and the `/account`
//!   read itself fails (authentication, rate limit, timeout, network or API error),
//!   [`Client::get_networks`] returns that `Err`, caches nothing in either the networks or the
//!   account entry, and selects no preferred network; the next call tries again. Only a
//!   *successful* fallback that finds no networks yields a cacheable empty result. Python
//!   swallowed the failure up to `v8.1.8`.
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

use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
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
    /// `eero-api`'s `_preferred_network_id` and `_preferred_network_explicit`
    /// (`client.py:114-117` at `v8.4.1`): in-memory only, lost on restart, never synced with any
    /// API-layer state — see [`Client::set_preferred_network`].
    preferred: RwLock<PreferredNetwork>,
}

/// The preferred network, where it came from, and how many session changes have happened.
///
/// One lock guards all three so the check-and-set done by auto-discovery cannot interleave with
/// an explicit choice or a session change.
#[derive(Debug, Default)]
struct PreferredNetwork {
    /// The preferred network id, if any.
    id: Option<String>,
    /// `true` only after [`Client::set_preferred_network`]; a value found by auto-discovery
    /// leaves this untouched. Only a discovered network is forgotten when the session changes.
    explicit: bool,
    /// How many times the session has changed under this client. Advanced by
    /// `Client::forget_discovered_network` and `Client::clear_auth_data` — never by cache
    /// invalidation, so an ordinary write cannot stop auto-discovery — so that a
    /// `/networks` read sent under the previous session cannot latch that account's first
    /// network as the new session's preferred one. Wraps on overflow; only inequality matters.
    session_changes: u64,
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
    /// is given, and marks it as an explicit choice.
    ///
    /// Ported from `set_preferred_network` (`client.py:1585-1607` at `v8.4.1`): in-memory only.
    /// "For persistent storage, the CLI application should manage its own configuration file" —
    /// lost on process restart, never written to disk, never synced with any API-layer state.
    /// Python accepts an `int` and stores `str(network_id)`; a Rust caller passes
    /// `id.to_string()`, since the stored type is always a string.
    ///
    /// An explicit choice survives a session change ([`Client::set_session_token`],
    /// [`Client::clear_session_token`], [`Client::logout`]) and also replaces a network found by
    /// auto-discovery: setting it after a discovery makes the value explicit. Only
    /// [`Client::clear_auth_data`] drops it. A network that [`Client::get_networks`] discovers
    /// is forgotten on a session change instead, so the next call without a `network_id`
    /// rediscovers the new account's network.
    pub fn set_preferred_network(&self, network_id: impl Into<String>) {
        let mut guard = self.preferred_write();
        guard.id = Some(network_id.into());
        guard.explicit = true;
    }

    /// Returns the currently preferred network id, if one has been set explicitly (via
    /// [`Client::set_preferred_network`]) or derived automatically (the one-shot side effect of
    /// [`Client::get_networks`] — see that method's docs).
    ///
    /// Ported from the `preferred_network_id` property (`client.py:1609-1612` at `v8.4.1`).
    #[must_use]
    pub fn preferred_network_id(&self) -> Option<String> {
        self.preferred_read().id.clone()
    }

    fn preferred_read(&self) -> RwLockReadGuard<'_, PreferredNetwork> {
        self.preferred
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn preferred_write(&self) -> RwLockWriteGuard<'_, PreferredNetwork> {
        self.preferred
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Drops the preferred network when auto-discovery chose it; a network chosen with
    /// [`Client::set_preferred_network`] is kept. Counts as a session change either way.
    ///
    /// Ported from `_forget_discovered_network` (`client.py:457-465` at `v8.4.1`).
    fn forget_discovered_network(&self) {
        let mut guard = self.preferred_write();
        guard.session_changes = guard.session_changes.wrapping_add(1);
        if !guard.explicit {
            guard.id = None;
        }
    }

    /// Empties the cache and forgets a discovered preferred network, after a session change.
    ///
    /// Ported from `_reset_session_state` (`client.py:467-470` at `v8.4.1`).
    fn reset_session_state(&self) {
        self.clear_cache();
        self.forget_discovered_network();
    }

    /// How many session changes this client has seen; see [`PreferredNetwork::session_changes`].
    fn session_changes(&self) -> u64 {
        self.preferred_read().session_changes
    }

    /// Resolves a network id: explicit `network_id` → [`Client::preferred_network_id`] → (if
    /// `auto_discover`) the first network from [`Client::get_networks`] → [`Error::MissingNetworkId`].
    ///
    /// Ported from `_ensure_network_id` (`client.py:261-304` at `v8.4.1`):
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
    ///    [`crate::util::id_from_url`] if `id` is absent, empty or `0`. A JSON integer `id`
    ///    becomes its decimal string, as Python's `str(net_id)` does for accounts whose API
    ///    returns the id as a number (`client.py:302` at `v8.4.1`).
    /// 4. Otherwise, [`Error::MissingNetworkId`] (`client.py:304`'s bare `EeroException`).
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

    /// Logs the current session out, unconditionally clears this client's cache, and forgets a
    /// preferred network that auto-discovery chose.
    ///
    /// Ported from `logout()` (`client.py:505-514` at `v8.4.1`, `result = await
    /// self._api.logout(); if result: self._reset_session_state()`), but **deliberately diverges**
    /// from that `if result:` gate for the cache (security review finding F1):
    /// [`EeroApi::logout`] already clears the in-memory session and the credential store
    /// unconditionally, on every outcome — see that method's own docs — precisely because the
    /// session is gone either way, win or lose, on the wire. A `Client` that only dropped its
    /// cache when the network call happened to succeed left every cached getter (`get_account`,
    /// `get_network`, `get_devices`, `get_profiles`, ...) free to keep serving the previous
    /// session's data for the rest of the TTL after a failed logout (a `429`, a `5xx`, a
    /// timeout), with `is_authenticated()` already `false` and no auth check of its own gating
    /// any of those reads. Clearing the cache unconditionally here — before either return arm —
    /// closes that gap and matches [`EeroApi::logout`]'s own contract: the cache and the session
    /// become stale at exactly the same moment, regardless of whether the request itself
    /// succeeded.
    ///
    /// The discovered network follows Python: it is forgotten when the session was actually
    /// logged out (`Ok(true)`), and also on `Err`, because the in-memory session is cleared
    /// before the store clear that failed. It is kept on `Ok(false)`, which reports that there
    /// was no session to log out of. A network chosen with [`Client::set_preferred_network`]
    /// always survives.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`EeroApi::logout`] returns. The cache is cleared regardless.
    pub async fn logout(&self) -> Result<bool, Error> {
        let result = self.api.logout().await;
        self.clear_cache();
        if !matches!(result, Ok(false)) {
            self.forget_discovered_network();
        }
        result
    }

    /// Seeds the active session with a pre-existing session token, clears this client's cache
    /// and forgets a preferred network that auto-discovery chose.
    ///
    /// Ported from `set_session_token()` (`client.py:516-534` at `v8.4.1`): the reset runs once
    /// the auth call has accepted the token, so a `Validation` failure — which returns before
    /// anything changes — leaves the cache and the discovered network alone. A network chosen
    /// with [`Client::set_preferred_network`] survives.
    ///
    /// `async` since `eero-api` 8.1.10: the credential-store write runs on the runtime's blocking
    /// pool instead of the calling task's thread. The in-memory session changes first, the cache
    /// is cleared once the store hop has completed, and [`Client::credentials_persisted`] reports
    /// whether the store accepted the write.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "token", .. }` if `token` is empty. Returns
    /// `Error::Storage` if a configured credential store failed to persist the new session under
    /// [`StorageFailures::Fatal`] (see [`crate::auth::AuthApi::set_session_token`]'s own docs).
    /// The new session is already in place when that error is returned, so — unlike Python,
    /// which skips its reset when the auth call raises — the cache and the discovered network are
    /// reset in that case too: nothing cached under the previous session may be served to the
    /// new one.
    pub async fn set_session_token(&self, token: &str) -> Result<(), Error> {
        let result = self.api.auth().set_session_token(token).await;
        if session_changed(&result) {
            self.reset_session_state();
        }
        result
    }

    /// Clears the active session token (keeping any refresh token), clears this client's cache
    /// and forgets a preferred network that auto-discovery chose.
    ///
    /// Ported from `clear_session_token()` (`client.py:536-545` at `v8.4.1`). A network chosen
    /// with [`Client::set_preferred_network`] survives; [`Client::clear_auth_data`] drops it.
    ///
    /// `async` since `eero-api` 8.1.10: the in-memory session is cleared first and the
    /// credential-store clear then runs on the runtime's blocking pool.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if a configured credential store failed to persist the change
    /// under [`StorageFailures::Fatal`] (see [`crate::auth::AuthApi::clear_session_token`]'s own
    /// docs). The in-memory session is cleared regardless, so this client's cache and discovered
    /// network are reset in that case as well.
    pub async fn clear_session_token(&self) -> Result<(), Error> {
        let result = self.api.auth().clear_session_token().await;
        if session_changed(&result) {
            self.reset_session_state();
        }
        result
    }

    /// Removes every stored credential record without contacting the API, then resets this
    /// client's preferred network — **including one set with [`Client::set_preferred_network`]**
    /// — and empties its cache.
    ///
    /// Ported from `EeroClient.clear_auth_data()` (`client.py:563-578` at `v8.4.1`). Unlike
    /// [`Client::clear_session_token`] and [`Client::logout`], which keep an explicit preferred
    /// network, nothing cached or chosen is attributable to a session any more afterwards, so
    /// everything is dropped; [`Client::is_authenticated`] is `false`. The steps run in
    /// Python's order: the auth layer first (see [`crate::auth::AuthApi::clear_auth_data`]),
    /// then the preferred network, then the cache.
    ///
    /// # Errors
    ///
    /// Returns `Error::Storage` if a configured credential store failed to clear its entry under
    /// [`StorageFailures::Fatal`]. Python skips the facade reset when the auth layer raises. The
    /// in-memory session is already gone by then, so this resets the preferred network and the
    /// cache regardless, as [`Client::logout`] does.
    pub async fn clear_auth_data(&self) -> Result<(), Error> {
        let result = self.api.auth().clear_auth_data().await;
        {
            let mut guard = self.preferred_write();
            guard.id = None;
            guard.explicit = false;
            guard.session_changes = guard.session_changes.wrapping_add(1);
        }
        self.clear_cache();
        result
    }

    /// Whether the most recent attempt to persist the session to the configured credential store
    /// succeeded (`client.py:151-160` at `v8.4.1`, the `credentials_persisted` property).
    ///
    /// A pure delegation to [`EeroApi::credentials_persisted`]; see
    /// [`crate::auth::AuthApi::credentials_persisted`] for the exact semantics. `true` for a
    /// client with no credential store.
    #[must_use]
    pub fn credentials_persisted(&self) -> bool {
        self.api.credentials_persisted()
    }

    // ============================= Cached getters (the eight) =============================
    //
    // These are the *only* eight `EeroClient` methods that ever consult the cache. Every one of
    // them follows the same shape: the two-part read guard [`Cache::get`] implements (TTL plus
    // the falsy-value rule), unless `refresh_cache` skips it; then a snapshot of the cache's
    // invalidation counter, taken after the cache check and immediately before the request; then
    // a store through `Cache::put_if_current`, which refuses the response if an invalidation ran
    // while the request was in flight. The caller always gets the response, so a read that was
    // overtaken by a write hands back its (possibly pre-write) answer without caching it for
    // everyone else. A rejected store is not an error.

    /// Gets account information — returns the raw Eero API response.
    ///
    /// Ported from `get_account()` (`client.py:581-598` at `v8.4.1`). The request goes through
    /// [`crate::endpoints::account::AccountApi::get_account`], so a `401 error.session.refresh`
    /// is refreshed and replayed once and the replayed envelope is the one cached, and a client
    /// with no session fails before any request with nothing cached.
    ///
    /// The envelope is stored only if no cache invalidation ran while the request was in flight
    /// (see the module docs of [`crate::cache`]).
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces otherwise.
    pub async fn get_account(&self, refresh_cache: bool) -> Result<Envelope, Error> {
        if !refresh_cache && let Some(cached) = self.cache.get(&CacheKey::Account) {
            return Ok(cached);
        }
        let generation = self.cache.generation();
        let response = self.api.account().get_account().await?;
        self.cache
            .put_if_current(CacheKey::Account, response.clone(), generation);
        Ok(response)
    }

    /// Gets the list of networks on the account — returns the raw Eero API response.
    ///
    /// Ported from `get_networks()` (`client.py:602-675` at `v8.4.1`). See the module docs for
    /// the `/account` fallback this method implements (the synthesised envelope is reproduced;
    /// a failing fallback propagates and caches nothing) and for why this method's own call to
    /// [`Client::get_networks`] — via `Client::ensure_network_id`'s auto-discovery path — never
    /// passes `refresh_cache = true` (a cached "zero networks" response keeps auto-discovery
    /// failing for a full TTL window, as in Python).
    ///
    /// The one-shot `_preferred_network_id` side effect (`client.py:654-673`) is reproduced
    /// exactly: it only runs when [`Client::preferred_network_id`] is currently unset, it is
    /// derived from the *final* (possibly synthesised) response, not the original `/networks`
    /// one, and it never marks the value as an explicit choice (see
    /// [`Client::set_preferred_network`]). A first entry whose `id` is a JSON integer yields that
    /// integer's decimal string.
    ///
    /// The networks entry is stored only if no cache invalidation ran while the request was in
    /// flight, but the preferred network is still selected from the response in that case, as
    /// Python does (`client.py:652-673`). The one exception is a session change: a response that
    /// was requested under the previous session never selects a preferred network, so it cannot
    /// leave the new session targeting the previous account's network.
    ///
    /// # Errors
    ///
    /// Returns whatever status-mapped [`Error`] the initial `/networks` request produces (a 2xx
    /// body that is not a JSON object included, before any `/account` request is made and
    /// before anything is cached). Returns whatever [`Client::get_account`] produces if the
    /// `/networks` list came back empty and the `/account` fallback itself fails.
    pub async fn get_networks(&self, refresh_cache: bool) -> Result<Envelope, Error> {
        if !refresh_cache && let Some(cached) = self.cache.get(&CacheKey::Networks) {
            return Ok(cached);
        }

        let generation = self.cache.generation();
        let session_changes = self.session_changes();
        let response = self.api.networks().get_networks().await?;
        let networks = extract_networks_list(response.data());

        let response = if networks.is_empty() {
            self.apply_account_fallback(response).await?
        } else {
            response
        };

        self.cache
            .put_if_current(CacheKey::Networks, response.clone(), generation);
        // Not gated on the store above: a rejected cache fill still selects the network.
        self.derive_preferred_network_id(&response, session_changes);

        Ok(response)
    }

    /// Implements the `/account` fallback half of [`Client::get_networks`] (`client.py:640-652`
    /// at `v8.4.1`).
    ///
    /// Called only when the `/networks` response's own network list came back empty. Forces a
    /// live `/account` fetch (`refresh_cache = true` — this also refreshes the `account` cache
    /// entry as a side effect, regardless of whether the caller needed it, and with its own
    /// invalidation snapshot). If the account-derived list is non-empty, returns a synthesised
    /// envelope combining `original`'s `meta` with the account-derived `data.networks`;
    /// otherwise returns `original` unchanged, which the caller caches as an empty result.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Client::get_account`] returns on failure: the failure of the
    /// fallback means the account could not be consulted, which is not the same as the account
    /// having no networks.
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
    /// (`client.py:654-673` at `v8.4.1`): a no-op if a preferred network is already set,
    /// otherwise derives one from `response`'s (possibly synthesised) network list, using the
    /// same `id`-or-`url` extraction `Client::ensure_network_id`'s auto-discovery path uses.
    /// Silently leaves the preferred network unset if extraction yields nothing, exactly like
    /// Python.
    ///
    /// The check and the set happen under one lock. The explicit-choice flag is left alone, so a
    /// derived value never counts as an explicit one. Nothing is set if the session changed since
    /// `session_changes` was read, i.e. if `response` was requested under the previous session.
    fn derive_preferred_network_id(&self, response: &Envelope, session_changes: u64) {
        let networks = extract_networks_list(response.data());
        let Some(id) = networks.first().and_then(extract_network_id) else {
            return;
        };
        let mut guard = self.preferred_write();
        if guard.session_changes != session_changes {
            return;
        }
        if guard.id.as_deref().is_none_or(str::is_empty) {
            guard.id = Some(id);
        }
    }

    // ============================= Parent-resolution helpers =============================
    //
    // Ported from `eero-api`'s read-only, side-effect-free `_..._parent(_kwargs)` helpers
    // (`client.py:316-452` at `v8.4.1`): each looks up an already cached envelope and hands it
    // back unchanged for use as a domain method's `parent=` argument, so a wrapper can prefer
    // the server's own published `resources` link over a hand-built template without an extra
    // round trip. None of these mutate the cache or trigger a network call; a miss (nothing
    // cached, or a cached entry that is no longer fresh) is simply `None`, mirroring Python's
    // `{}`-kwargs-omitted shape (this port returns `Option<Value>` instead — the caller passes
    // it straight through as `parent: Option<&Value>`, the shape `crate::routes::Resource::resolve`
    // and `Nested::resolve` already expect). `Client::eero_parent` and `Client::find_by_id_or_url`
    // have no call site yet; every other helper below does, and all of them are exercised
    // directly by this module's own `#[cfg(test)] mod tests`.

    /// Returns the cached network envelope for `network_id`, if a fresh entry exists.
    ///
    /// Ported from `_network_parent` (`eero-api src/eero/client.py:316-328` at `v8.4.1`): looks
    /// up [`CacheKey::network`] and returns the whole cached envelope (`{"meta": .., "data": ..}`,
    /// not just its `data`) exactly when [`Cache::get`] still considers it fresh — never mutates
    /// the cache, never triggers a network call. Python additionally wraps this in
    /// `_network_parent_kwargs`, which turns `Some`/`None` into a `{"parent": ..}`/`{}` kwargs
    /// dict for `**kwargs`-unpacking into a domain call; this port has no keyword-unpacking
    /// equivalent, so callers pass this method's `Option<Value>` straight through as
    /// `parent: Option<&Value>` instead.
    fn network_parent(&self, network_id: &str) -> Option<Value> {
        self.cache
            .get(&CacheKey::network(network_id))
            .map(Envelope::into_value)
    }

    /// Returns the caller's own `parent` envelope when one was supplied, otherwise the cached
    /// network envelope for `network_id` (see [`Client::network_parent`]).
    ///
    /// Ported from `_network_parent_kwargs_or` (`eero-api src/eero/client.py:345-362` at
    /// `v8.4.1`): the caller's envelope is forwarded unchanged and wins over the cache, and an
    /// empty object still counts as supplied. The cache lookup is keyed by the *resolved*
    /// `network_id`, so an explicit network with nothing cached yields no parent.
    #[allow(dead_code)] // the wrappers that need it are added next to it; see the banner above
    fn network_parent_or(&self, network_id: &str, parent: Option<&Value>) -> Option<Value> {
        parent.cloned().or_else(|| self.network_parent(network_id))
    }

    /// Finds the entry in `envelopes` whose `id` field, or whose `url` field's trailing path
    /// segment, matches `resource_id` exactly.
    ///
    /// Ported from the `@staticmethod` `_find_by_id_or_url`
    /// (`eero-api src/eero/client.py:364-387` at `v8.4.1`). `resource_id` is compared verbatim
    /// against each entry's `id` field first; on a miss, against the trailing path segment of that
    /// entry's own `url` field, extracted via [`crate::util::id_from_url`] — a `url` for which
    /// `id_from_url` itself errors (e.g. empty, or composed entirely of slashes) is simply never
    /// a match, exactly like Python's own `isinstance(url, str)` guard skips a non-string `url`.
    /// Entries that are not JSON objects are skipped, mirroring Python's `isinstance(entry,
    /// dict)` guard. Never mutates `envelopes`; returns a clone of the matching entry, if any.
    #[allow(dead_code)] // see the banner above
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
    /// Ported from `_eero_parent_kwargs` (`eero-api src/eero/client.py:389-410` at `v8.4.1`):
    /// looks up [`CacheKey::eeros`] for `network_id`, and — only when that entry is still fresh —
    /// reads its envelope's `data` field as a JSON array (a non-array `data`, like Python's own
    /// `isinstance(cached, dict)`/list check, yields `None` rather than a match) and searches it
    /// via [`Client::find_by_id_or_url`].
    #[allow(dead_code)] // see the banner above
    fn eero_parent(&self, network_id: &str, eero_id: &str) -> Option<Value> {
        let cached = self.cache.get(&CacheKey::eeros(network_id))?;
        let data = cached.data().as_array()?;
        Self::find_by_id_or_url(data, eero_id)
    }

    /// Builds the cache key of one device, identical for every spelling of its id.
    ///
    /// Ported from `_device_cache_key` (`eero-api src/eero/client.py:412-436` at `v8.4.1`). Takes
    /// the last path segment of `device_id` through [`crate::util::id_from_url`], so a bare id,
    /// an API-returned path and an absolute URL agree. When that segment is a 12-hex-digit MAC in
    /// any spelling (colons, dashes, dots or none, any case) it is reduced to lowercase hex
    /// without separators; any other id is kept as written. Only the cache key is canonicalised:
    /// requests still carry the caller's own spelling.
    ///
    /// Unlike Python, whose keys are flat strings (`get_device("devices")` lands on the device
    /// list's `{nid}_devices` key), the key is a structured [`CacheKey::Device`], so a device
    /// can never collide with the list.
    ///
    /// # Errors
    ///
    /// [`Error::Validation`] with `field: "id_or_url"` and `must be a non-empty string` when
    /// `device_id` is empty. A `device_id` made only of slashes is rejected too, with
    /// `resolves to an empty path segment` (see [`crate::util::id_from_url`]); Python derives the
    /// key `{nid}_` for it and fails later in the domain layer.
    fn device_cache_key(network_id: &str, device_id: &str) -> Result<CacheKey, Error> {
        let segment = crate::util::id_from_url(device_id)?;
        let mac: String = segment
            .chars()
            .filter(|c| !matches!(c, ':' | '-' | '.'))
            .collect();
        let canonical = if mac.len() == 12 && mac.bytes().all(|b| b.is_ascii_hexdigit()) {
            mac.to_ascii_lowercase()
        } else {
            segment
        };
        Ok(CacheKey::device(network_id, canonical))
    }

    /// Returns the cached single-device envelope for `device_id` within `network_id`, if fresh,
    /// looked up through the canonical key (see [`Client::device_cache_key`]), so a device cached
    /// under one spelling is found under any other.
    ///
    /// Ported from `_device_parent_kwargs` (`eero-api src/eero/client.py:438-452` at `v8.4.1`):
    /// returns the whole cached envelope — not just its `data` — exactly when it is still fresh.
    ///
    /// # Errors
    ///
    /// As [`Client::device_cache_key`]: an empty `device_id` fails before any lookup.
    fn device_parent(&self, network_id: &str, device_id: &str) -> Result<Option<Value>, Error> {
        let key = Self::device_cache_key(network_id, device_id)?;
        Ok(self.cache.get(&key).map(Envelope::into_value))
    }

    // ============================= Cache-invalidation helpers =============================
    //
    // Mirror Python's own private helpers (`client.py:1173-1185, 1273-1300, 2731-2739` at
    // `v8.4.1`): each drops precisely the keys Python drops, no more. Where a deliberate
    // `network[nid]`/`eeros[{nid}_eeros]` drop is added on top of Python (the two documented
    // improvements in [`crate::cache`]), that extra call is made directly at the write site
    // rather than folded into a helper here — there is no Python helper for it to mirror. Every
    // invalidation, through a helper or not, advances the cache's invalidation counter (see
    // [`crate::cache`]), which is what rejects the store of a read that was in flight.

    /// Drops the cached account after a write to it, and so rejects the store of a
    /// `get_account` that was already in flight.
    ///
    /// Mirrors `_invalidate_account_cache` (`client.py:3441-3448` at `v8.4.1`).
    fn invalidate_account_cache(&self) {
        self.cache.invalidate(&CacheKey::Account);
    }

    /// Drops `network[{nid}]` for one network.
    ///
    /// Mirrors `_invalidate_network_cache` (`client.py:2731-2739` at `v8.4.1`): a single call
    /// site for every network-scoped setter to invalidate through, instead of each one
    /// repeating `self.cache.invalidate(&CacheKey::network(..))` by hand.
    fn invalidate_network_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::network(network_id));
    }

    /// Drops `eeros[{nid}_eeros]` for one network.
    ///
    /// Mirrors `_invalidate_eeros_cache` (`client.py:891-896` at `v8.4.1`) — added for the same
    /// reason as [`Client::invalidate_network_cache`] just above. [`Client::reboot_eero`]/
    /// [`Client::set_led`]/[`Client::set_led_brightness`]/[`Client::set_nightlight`] etc.
    /// currently call `self.cache.invalidate(&CacheKey::eeros(..))` inline rather than through
    /// this helper; either spelling has the same effect.
    #[allow(dead_code)] // the network-wide reboot wrapper that uses it is added next to it
    fn invalidate_eeros_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::eeros(network_id));
    }

    /// Drops `devices[{nid}_{did}]` for every id in `device_ids`, then `devices[{nid}_devices]`.
    ///
    /// Mirrors `_invalidate_device_cache(network_id, *device_ids)` (`eero-api
    /// src/eero/client.py:1173-1185` at `v8.4.1`): zero or more devices, and the network's device
    /// list is always dropped — a write that touches no single device (a network reboot, say)
    /// passes an empty slice. Each id is reduced to its canonical key (see
    /// [`Client::device_cache_key`]), so a write under any spelling drops the entry that was read
    /// under another.
    ///
    /// An id that has no key (an empty id or one made only of slashes) is skipped: such an id
    /// can never have been cached, and the write this runs after has already succeeded. Python
    /// would raise there, after the write, leaving the device list in place.
    fn invalidate_device_cache(&self, network_id: &str, device_ids: &[&str]) {
        for &device_id in device_ids {
            if let Ok(key) = Self::device_cache_key(network_id, device_id) {
                self.cache.invalidate(&key);
            }
        }
        self.cache.invalidate(&CacheKey::devices(network_id));
    }

    /// Drops both `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` for one profile.
    ///
    /// Mirrors `_invalidate_profile_cache` (`eero-api src/eero/client.py:1273-1300` at `v8.4.1`),
    /// which already drops both keys itself — see `Client::rename_profile`'s docs for why call
    /// sites that call both `_invalidate_profile_cache` and `_invalidate_profiles_list_cache` in
    /// Python only need this one helper here.
    fn invalidate_profile_cache(&self, network_id: &str, profile_id: &str) {
        self.cache
            .invalidate(&CacheKey::profile(network_id, profile_id));
        self.cache.invalidate(&CacheKey::profiles(network_id));
    }

    /// Drops only `profiles[{nid}_profiles]` — used by `Client::create_profile`, the one write
    /// with no single-profile key to also drop yet.
    ///
    /// Mirrors `_invalidate_profiles_list_cache` (`eero-api src/eero/client.py:1273-1300` at
    /// `v8.4.1`).
    fn invalidate_profiles_list_cache(&self, network_id: &str) {
        self.cache.invalidate(&CacheKey::profiles(network_id));
    }
}

/// Whether the outcome of a session setter means the in-memory session changed.
///
/// `Ok` obviously does. `Error::Storage` does too: the setters install the new session in memory
/// before they touch the credential store, so a store failure reported under
/// [`StorageFailures::Fatal`] arrives after the change. Anything else (the token-shape
/// `Validation` error) is raised before anything changes.
fn session_changed<T>(result: &Result<T, Error>) -> bool {
    matches!(result, Ok(_) | Err(Error::Storage(_)))
}

/// Extracts a JSON array of network entries from a `{meta, data}` envelope's `data` value.
///
/// Shared by [`Client::get_networks`] (the initial `/networks` response), `Client::ensure_network_id`'s
/// auto-discovery step, and [`Client::derive_preferred_network_id`] — the same extraction
/// pattern Python duplicates verbatim at three call sites (`client.py:285-290`, `:633-638`,
/// `:657-662` at `v8.4.1`). If `data` is itself a JSON array, it is used directly; if
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

/// Extracts a network entry's id: prefers a usable `id` field, falling back to the trailing
/// path segment of a non-empty `url` field via [`crate::util::id_from_url`].
///
/// A usable `id` is a non-empty JSON string, or a non-zero JSON integer rendered as its decimal
/// string. Some accounts' API returns the id as a JSON number, and Python coerces it with
/// `str(net_id)` (`client.py:302` and `:673` at `v8.4.1`). Python's truthiness rule applies:
/// `0` counts as absent, so the `url` is consulted instead. A float, a boolean or a negative
/// number is not an id.
///
/// Shared by `Client::ensure_network_id`'s auto-discovery step and
/// [`Client::derive_preferred_network_id`] — the same four-line pattern Python duplicates at two
/// call sites (`client.py:295-302`, `:666-673` at `v8.4.1`). Built on
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
    let id = match entry.get("id") {
        Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
        Some(Value::Number(number)) => number
            .as_u64()
            .filter(|&id| id != 0)
            .map(|id| id.to_string()),
        _ => None,
    };
    if let Some(id) = id.filter(|id| crate::routes::validate_segment(id).is_ok()) {
        return Some(id);
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
/// response shape): mirrors `_networks_from_account` (`client.py:677-695` at `v8.4.1`)
/// exactly — `data.networks` may itself be a JSON object whose own `data` key holds the array, or
/// may be the array directly. Anything else (a missing key, `null`, a string, a number, a wrapped
/// `data` that is not a list, or an `account` envelope that is not an object at all, for which
/// [`Envelope::data`] is `null`) yields an empty list instead of an error. No truthiness
/// `or`-chain here; Python uses plain `isinstance` checks with no fallback between the two
/// shapes.
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

    /// Sets the `User-Agent` header sent on every request — login, verify and refresh as well
    /// as every domain call, since the whole client shares one transport. `None` (the default)
    /// sends [`crate::consts::DEFAULT_USER_AGENT`]. The Eero cloud reads this string as the
    /// client's app version and decides which capabilities to advertise from it; pass
    /// [`crate::consts::LEGACY_USER_AGENT`] to reproduce the previous, lower-capability
    /// behaviour. Forwarded to [`crate::transport::TransportBuilder::user_agent`], which
    /// validates the value when the client is built.
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
    /// paths use — before the underlying [`Transport`] is constructed. The header values and
    /// `base_url` are validated first, so a bad configuration fails before the store (a keyring
    /// prompt, a file read) is touched at all.
    ///
    /// # Errors
    ///
    /// Before the credential store is consulted at all (a bad configuration never reaches a
    /// keyring prompt or a file read, whatever [`ClientBuilder::storage_failures`] says):
    ///
    /// - `Error::Validation` if `base_url` was set to a string the transport rejects as a base,
    ///   with field `base_url` (see [`crate::transport::TransportBuilder::build`] for the rules);
    /// - `Error::Validation` if a header value is not printable ASCII: the `X-Accept-Language`
    ///   value set with [`ClientBuilder::accept_language`] or the `User-Agent` value set with
    ///   [`ClientBuilder::user_agent`].
    ///
    /// Then, only when a store is configured and no explicit session was given, `Error::Storage`
    /// if the initial load fails under [`StorageFailures::Fatal`] (under the default
    /// [`StorageFailures::Warn`], a load failure is logged at `WARN` and this proceeds with no
    /// initial session instead). Last, `Error::Network` if constructing the underlying
    /// `reqwest::Client` fails; a failure of a caller-supplied [`ClientBuilder::http_builder`]
    /// is only seen at this point, after the store has been read.
    pub async fn build(self) -> Result<Client, Error> {
        if self.session.is_none() && self.store.is_some() {
            self.validate_configuration()?;
        }

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
            preferred: RwLock::new(PreferredNetwork::default()),
        })
    }

    /// Runs the header and `base_url` validation [`Transport`]'s builder applies, on a throwaway
    /// transport that carries no session and no store, so that [`ClientBuilder::build`] can fail
    /// on a bad configuration before it reads the credential store. Using the transport's own
    /// builder keeps the checks, their order and their messages identical to the ones the real
    /// build applies; a caller-supplied `reqwest::ClientBuilder` is not part of the probe (it is
    /// consumed by the real build).
    fn validate_configuration(&self) -> Result<(), Error> {
        let mut probe = Transport::builder().user_agent(self.user_agent.clone());
        if let Some(base_url) = &self.base_url {
            probe = probe.base_url(base_url.clone());
        }
        if let Some(accept_language) = &self.accept_language {
            probe = probe.accept_language(accept_language.clone());
        }
        probe.build().map(drop)
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
        Cache, CacheKey, Client, EeroApi, Envelope, Error, PreferredNetwork,
        extract_account_networks, extract_network_id, extract_networks_list, non_empty,
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
        test_client_with_ttl(Duration::from_secs(60))
    }

    /// As [`test_client`], with an explicit cache TTL (`Duration::ZERO` disables reads without
    /// disabling writes).
    fn test_client_with_ttl(ttl: Duration) -> Client {
        let transport = Transport::builder()
            .build()
            .expect("default transport configuration (no overrides) always builds successfully");
        Client {
            api: EeroApi::new(transport),
            cache: Cache::new(ttl),
            preferred: RwLock::new(PreferredNetwork::default()),
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

    // ===================== extract_network_id: integer ids =====================
    //
    // Some accounts' API returns the network id as a JSON number; Python coerces it with
    // `str(net_id)` and treats `0` as absent.

    #[test]
    fn extract_network_id_coerces_an_integer_id_without_a_url() {
        assert_eq!(
            extract_network_id(&json!({"id": 12_345_678})).as_deref(),
            Some("12345678")
        );
    }

    #[test]
    fn extract_network_id_prefers_an_integer_id_over_a_mismatching_url() {
        let entry = json!({"id": 123, "url": "/2.2/networks/other"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("123"));
    }

    #[test]
    fn extract_network_id_treats_a_zero_id_as_absent() {
        let entry = json!({"id": 0, "url": "/2.2/networks/abc"});
        assert_eq!(extract_network_id(&entry).as_deref(), Some("abc"));
        assert!(extract_network_id(&json!({"id": 0})).is_none());
    }

    #[test]
    fn extract_network_id_does_not_take_floats_bools_or_negative_numbers_as_ids() {
        for id in [
            json!(1.5),
            json!(7.0),
            json!(true),
            json!(-5),
            json!(null),
            json!([1]),
        ] {
            let entry = json!({"id": id, "url": "/2.2/networks/from-url"});
            assert_eq!(
                extract_network_id(&entry).as_deref(),
                Some("from-url"),
                "{id} must not be taken as an id"
            );
            assert!(extract_network_id(&json!({"id": id})).is_none(), "{id}");
        }
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
        assert!(extract_account_networks(&json!({"networks": "unexpected"})).is_empty());
        assert!(extract_account_networks(&json!({"networks": {"data": "unexpected"}})).is_empty());
        assert!(extract_account_networks(&json!({"networks": {"data": null}})).is_empty());
        assert!(extract_account_networks(&json!({"networks": 7})).is_empty());
        assert!(extract_account_networks(&json!([])).is_empty());
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
        let client = test_client_with_ttl(Duration::ZERO);
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
        let client = test_client_with_ttl(Duration::ZERO);
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
        assert_eq!(client.device_parent("net1", "dev1").unwrap(), None);
    }

    #[test]
    fn device_parent_returns_the_whole_cached_envelope_when_fresh() {
        let client = test_client();
        let envelope = json!({"meta": {"code": 200}, "data": {"mac": "dev1"}});
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(envelope.clone()),
        );
        assert_eq!(
            client.device_parent("net1", "dev1").unwrap(),
            Some(envelope)
        );
    }

    #[test]
    fn device_parent_is_scoped_to_its_own_device_id() {
        let client = test_client();
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "dev1"}})),
        );
        assert_eq!(client.device_parent("net1", "dev2").unwrap(), None);
    }

    #[test]
    fn device_parent_is_none_once_the_entry_has_expired() {
        let client = test_client_with_ttl(Duration::ZERO);
        client.cache.put(
            CacheKey::device("net1", "dev1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "dev1"}})),
        );
        assert_eq!(client.device_parent("net1", "dev1").unwrap(), None);
    }

    #[test]
    fn device_parent_finds_an_entry_cached_under_another_spelling() {
        let client = test_client();
        let envelope = json!({"meta": {"code": 200}, "data": {"mac": "aa:bb:cc:dd:ee:ff"}});
        client.cache.put(
            CacheKey::device("net1", "aabbccddeeff"),
            Envelope::from_value(envelope.clone()),
        );
        assert_eq!(
            client.device_parent("net1", "AA:BB:CC:DD:EE:FF").unwrap(),
            Some(envelope)
        );
    }

    #[test]
    fn device_parent_rejects_an_empty_device_id() {
        let client = test_client();
        assert_empty_id_error(client.device_parent("net1", ""));
    }

    // ===================== device_cache_key =====================
    //
    // Mirrors `tests/api/test_client.py::TestDeviceCacheKeys` (`MAC_SPELLINGS`, v8.4.1). The
    // absolute form uses a host that is not the production one.

    const MAC_SPELLINGS: [&str; 7] = [
        "aabbccddeeff",
        "AA:BB:CC:DD:EE:FF",
        "aa-bb-cc-dd-ee-ff",
        "aabb.ccdd.eeff",
        "/2.2/networks/n1/devices/aabbccddeeff",
        "/2.2/networks/n1/devices/AA:BB:CC:DD:EE:FF/",
        "http://mock.test:1234/2.2/networks/n1/devices/aabbccddeeff",
    ];

    fn assert_empty_id_error<T: std::fmt::Debug>(result: Result<T, Error>) {
        match result {
            Err(Error::Validation { field, message, .. }) => {
                assert_eq!(field, "id_or_url");
                assert_eq!(message, "must be a non-empty string");
            }
            other => panic!("expected the id_or_url validation error, got {other:?}"),
        }
    }

    #[test]
    fn device_cache_key_maps_every_mac_spelling_to_one_key() {
        for spelling in MAC_SPELLINGS {
            assert_eq!(
                Client::device_cache_key("n1", spelling).unwrap(),
                CacheKey::device("n1", "aabbccddeeff"),
                "{spelling}"
            );
        }
    }

    #[test]
    fn device_cache_key_keeps_other_ids_as_written() {
        for (spelling, segment) in [
            ("d1", "d1"),
            ("/2.2/networks/n1/devices/D1", "D1"),
            ("ABC-123", "ABC-123"),
            ("zz:bb:cc:dd:ee:ff", "zz:bb:cc:dd:ee:ff"),
            ("aa:bb:cc:dd:ee:ff:00", "aa:bb:cc:dd:ee:ff:00"),
            ("devices", "devices"),
        ] {
            assert_eq!(
                Client::device_cache_key("n1", spelling).unwrap(),
                CacheKey::device("n1", segment),
                "{spelling}"
            );
        }
    }

    #[test]
    fn device_cache_key_is_scoped_to_the_network() {
        assert_ne!(
            Client::device_cache_key("n1", "aabbccddeeff").unwrap(),
            Client::device_cache_key("n2", "aabbccddeeff").unwrap()
        );
    }

    #[test]
    fn device_cache_key_rejects_an_empty_id_with_the_id_or_url_error() {
        assert_empty_id_error(Client::device_cache_key("n1", ""));
    }

    #[test]
    fn device_cache_key_rejects_a_slash_only_id() {
        // Python derives the key `n1_` for `/`; the shared `id_from_url` helper refuses it.
        assert!(matches!(
            Client::device_cache_key("n1", "/"),
            Err(Error::Validation { ref field, .. }) if field == "id_or_url"
        ));
    }

    // ===================== invalidate_device_cache =====================

    fn seed_device_entries(client: &Client) {
        client.cache.put(
            CacheKey::device("n1", "aabbccddeeff"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "x"}})),
        );
        client.cache.put(
            CacheKey::devices("n1"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": [{"mac": "x"}]})),
        );
    }

    #[test]
    fn invalidate_device_cache_drops_the_entry_and_the_list_for_every_spelling() {
        for spelling in MAC_SPELLINGS {
            let client = test_client();
            seed_device_entries(&client);

            client.invalidate_device_cache("n1", &[spelling]);

            assert!(
                client
                    .cache
                    .get(&CacheKey::device("n1", "aabbccddeeff"))
                    .is_none(),
                "the single-device entry survived a write under {spelling}"
            );
            assert!(
                client.cache.get(&CacheKey::devices("n1")).is_none(),
                "the device list survived a write under {spelling}"
            );
        }
    }

    #[test]
    fn invalidate_device_cache_with_no_ids_drops_only_the_list() {
        let client = test_client();
        seed_device_entries(&client);

        client.invalidate_device_cache("n1", &[]);

        assert!(client.cache.get(&CacheKey::devices("n1")).is_none());
        assert!(
            client
                .cache
                .get(&CacheKey::device("n1", "aabbccddeeff"))
                .is_some()
        );
    }

    #[test]
    fn invalidate_device_cache_skips_an_id_without_a_key_but_still_drops_the_list() {
        let client = test_client();
        seed_device_entries(&client);

        client.invalidate_device_cache("n1", &["", "/", "aabbccddeeff"]);

        assert!(client.cache.get(&CacheKey::devices("n1")).is_none());
        assert!(
            client
                .cache
                .get(&CacheKey::device("n1", "aabbccddeeff"))
                .is_none()
        );
    }

    #[test]
    fn invalidate_device_cache_leaves_other_devices_and_networks_alone() {
        let client = test_client();
        seed_device_entries(&client);
        client.cache.put(
            CacheKey::device("n1", "other"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "other"}})),
        );
        client.cache.put(
            CacheKey::device("n2", "aabbccddeeff"),
            Envelope::from_value(json!({"meta": {"code": 200}, "data": {"mac": "y"}})),
        );

        client.invalidate_device_cache("n1", &["AA:BB:CC:DD:EE:FF"]);

        assert!(client.cache.get(&CacheKey::device("n1", "other")).is_some());
        assert!(
            client
                .cache
                .get(&CacheKey::device("n2", "aabbccddeeff"))
                .is_some()
        );
    }

    // ===================== network_parent_or =====================
    //
    // Mirrors `tests/api/test_client.py::TestNetworkParentKwargsOr` (v8.4.1).

    fn seed_network(client: &Client, network_id: &str) -> serde_json::Value {
        let cached = json!({"meta": {"code": 200}, "data": {"name": network_id}});
        client.cache.put(
            CacheKey::network(network_id),
            Envelope::from_value(cached.clone()),
        );
        cached
    }

    #[test]
    fn network_parent_or_prefers_the_caller_parent_over_a_fresh_cache() {
        let client = test_client();
        seed_network(&client, "net1");
        let caller = json!({"meta": {"code": 200}, "data": {"name": "caller"}});

        assert_eq!(
            client.network_parent_or("net1", Some(&caller)),
            Some(caller)
        );
    }

    #[test]
    fn network_parent_or_treats_an_empty_object_as_supplied() {
        let client = test_client();
        seed_network(&client, "net1");

        assert_eq!(
            client.network_parent_or("net1", Some(&json!({}))),
            Some(json!({}))
        );
    }

    #[test]
    fn network_parent_or_falls_back_to_the_fresh_cached_envelope() {
        let client = test_client();
        let cached = seed_network(&client, "net1");

        assert_eq!(client.network_parent_or("net1", None), Some(cached));
    }

    #[test]
    fn network_parent_or_is_none_on_an_empty_cache() {
        assert_eq!(test_client().network_parent_or("net1", None), None);
    }

    #[test]
    fn network_parent_or_is_none_once_the_cached_entry_expired() {
        let client = test_client_with_ttl(Duration::ZERO);
        seed_network(&client, "net1");

        assert_eq!(client.network_parent_or("net1", None), None);
    }

    #[test]
    fn network_parent_or_is_scoped_to_the_given_network_id() {
        let client = test_client();
        seed_network(&client, "network-0001");

        assert_eq!(client.network_parent_or("network-0002", None), None);
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
