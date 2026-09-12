//! The `Client` facade: [`EeroApi`] plus a [`Cache`] plus in-memory preferred-network state.
//!
//! Ported from `eero-api`'s `EeroClient` (`src/eero/client.py`). See
//! `.claude/tasks/briefs/client.md` for the full behaviour brief this module implements — every
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
//! the cache invalidation the behaviour brief's §2 table specifies for each — including the two
//! deliberate improvements over Python `rust-port-plan.md` §3.8 records (`set_led_brightness`,
//! and every DNS/SQM/security setter, invalidating cache entries Python's own setters forget to).
//! `set_device_priority`/`get_device_priority` and the `get_activity*` family remain unported —
//! see the phase-5 section's own banner comment for why.
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
//! `.claude/docs/architecture.md`'s own request-flow sketch, `Client::builder().session(s)
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
//! [`crate::transport::Transport::send`], the exact same one-shot-refresh-and-retry path every
//! other cached getter in this file uses. That decision was already made at the endpoints layer
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
//!   attempt in `except Exception: _LOGGER.debug(...)`, so an authentication failure, timeout,
//!   or any other error while fetching `/account` is invisible to the caller — `get_networks()`
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

    /// Logs the current session out and, on success, clears this client's cache.
    ///
    /// Ported from `logout()` (`client.py:197-206`): `result = await self._api.logout(); if
    /// result: self.clear_cache()`. This port's [`EeroApi::logout`] returns
    /// `Result<Envelope, Error>` rather than a `bool`, so "success" is `Ok(_)` — the direct
    /// analogue of Python's truthy `result`.
    ///
    /// # Errors
    ///
    /// Propagates whatever [`EeroApi::logout`] returns; the cache is left untouched on `Err`,
    /// matching Python's `if result:` gate exactly.
    pub async fn logout(&self) -> Result<Envelope, Error> {
        let result = self.api.logout().await;
        if result.is_ok() {
            self.clear_cache();
        }
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

    /// Gets a network's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_network()` (`client.py:333-357`). Resolves `network_id` with
    /// `auto_discover = true` (Python's default, since this call omits the argument), matching
    /// every other cached getter that takes a network id.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no network can be resolved
    /// (see `Client::ensure_network_id`). Otherwise, whatever status-mapped [`Error`] the
    /// request produces.
    pub async fn get_network(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::network(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.networks().get_network(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets the list of Eero devices (mesh nodes) on a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_eeros()` (`client.py:361-386`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_eeros(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::eeros(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.eeros().get_eeros(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets the list of connected devices — returns the raw Eero API response.
    ///
    /// Ported from `get_devices()` (`client.py:436-461`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_devices(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::devices(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.devices().get_devices(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single device's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_device()` (`client.py:463-492`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note. Note the parameter order: `device_id` (required)
    /// comes first, `network_id` (optional) second — matching every cached getter that takes an
    /// item id (brief §6).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_device(
        &self,
        device_id: &str,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::device(network_id.as_str(), device_id);
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self
            .api
            .devices()
            .get_device(&network_id, device_id)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets the list of profiles on a network — returns the raw Eero API response.
    ///
    /// Ported from `get_profiles()` (`client.py:579-604`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_profiles(
        &self,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::profiles(network_id.as_str());
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self.api.profiles().get_profiles(&network_id).await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    /// Gets a single profile's full object — returns the raw Eero API response.
    ///
    /// Ported from `get_profile()` (`client.py:606-634`); see [`Client::get_network`] for the
    /// shared `auto_discover = true` note. Note the parameter order: `profile_id` (required)
    /// comes first, `network_id` (optional) second, matching [`Client::get_device`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_profile(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
        refresh_cache: bool,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let key = CacheKey::profile(network_id.as_str(), profile_id);
        if !refresh_cache && let Some(cached) = self.cache.get(&key) {
            return Ok(cached);
        }
        let response = self
            .api
            .profiles()
            .get_profile(&network_id, profile_id)
            .await?;
        self.cache.put(key, response.clone());
        Ok(response)
    }

    // =========================== Uncached read-only pass-throughs ===========================
    //
    // Every method below resolves a network id (never touching the cache) and forwards straight
    // to the matching `EeroApi` domain accessor — no method in this section reads or writes
    // `self.cache` at all (brief §2.1: only the eight methods above ever do). Grouped in the
    // same order and under the same `# ====` banners `client.py` itself uses, so a diff against
    // the Python source stays easy to follow.

    // ==================== Diagnostics & Settings ====================

    /// Gets network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `get_diagnostics()` (`client.py:809-812`). Resolves `network_id` with
    /// `auto_discover = false`, like every method in this section (`client.py:809` through
    /// `client.py:1350` — see the module docs' "G5" note for the two exceptions at the very end
    /// of the file).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted here. Otherwise, whatever status-mapped [`Error`] the
    /// request produces.
    pub async fn get_diagnostics(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.diagnostics().get_diagnostics(&network_id).await
    }

    /// Gets network settings — returns the raw Eero API response.
    ///
    /// Ported from `get_settings()` (`client.py:819-822`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.settings().get_settings(&network_id).await
    }

    /// Queries insights time-series data — returns the raw Eero API response.
    ///
    /// Ported from `get_insights()` (`client.py:824-857`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. Unlike Python, `cadence` has no SDK-supplied default: the
    /// underlying [`crate::endpoints::InsightsApi::get_insights`] already made that call (see its
    /// own docs) and this method inherits it.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_insights(
        &self,
        network_id: Option<&str>,
        start: &str,
        end: &str,
        insight_type: &str,
        cadence: &str,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .insights()
            .get_insights(&network_id, start, end, insight_type, cadence)
            .await
    }

    /// Gets network routing information — returns the raw Eero API response.
    ///
    /// Ported from `get_routing()` (`client.py:859-862`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_routing(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.routing().get_routing(&network_id).await
    }

    /// Gets Thread protocol status — returns the raw Eero API response.
    ///
    /// Ported from `get_thread()` (`client.py:864-867`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_thread(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.thread().get_thread(&network_id).await
    }

    /// Gets support information — returns the raw Eero API response.
    ///
    /// Ported from `get_support()` (`client.py:869-872`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_support(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.support().get_support(&network_id).await
    }

    /// Gets the device blacklist — returns the raw Eero API response.
    ///
    /// Ported from `get_blacklist()` (`client.py:874-877`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blacklist(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.blacklist().get_blacklist(&network_id).await
    }

    /// Gets DHCP reservations — returns the raw Eero API response.
    ///
    /// Ported from `get_reservations()` (`client.py:879-882`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_reservations(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.reservations().get_reservations(&network_id).await
    }

    /// Gets port forwards — returns the raw Eero API response.
    ///
    /// Ported from `get_forwards()` (`client.py:910-913`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_forwards(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.forwards().get_forwards(&network_id).await
    }

    /// Gets transfer statistics for a network, or a single device on it — returns the raw Eero
    /// API response.
    ///
    /// Ported from `get_transfer_stats()` (`client.py:929-934`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_transfer_stats(
        &self,
        network_id: Option<&str>,
        device_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .transfer()
            .get_transfer_stats(&network_id, device_id)
            .await
    }

    /// Gets data usage statistics — returns the raw Eero API response.
    ///
    /// Ported from `get_data_usage()` (`client.py:936-944`), including `payload or {}`
    /// (`client.py:944`): a `None` payload is normalised to an empty JSON object before being
    /// attached as the (unusual, but intentional — see
    /// [`crate::endpoints::DataUsageApi::get_data_usage`]) `GET` request body. `auto_discover =
    /// false` — see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_data_usage(
        &self,
        network_id: Option<&str>,
        payload: Option<Value>,
        resource: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .data_usage()
            .get_data_usage(&network_id, payload.unwrap_or_else(|| json!({})), resource)
            .await
    }

    /// Gets burst reporters — returns the raw Eero API response.
    ///
    /// Ported from `get_burst_reporters()` (`client.py:946-949`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_burst_reporters(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .burst_reporters()
            .get_burst_reporters(&network_id)
            .await
    }

    /// Gets AC compatibility information — returns the raw Eero API response.
    ///
    /// Ported from `get_ac_compat()` (`client.py:951-954`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_ac_compat(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.ac_compat().get_ac_compat(&network_id).await
    }

    /// Gets OUI-check information — returns the raw Eero API response.
    ///
    /// Ported from `get_ouicheck()` (`client.py:956-959`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_ouicheck(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.ouicheck().get_ouicheck(&network_id).await
    }

    /// Gets password information — returns the raw Eero API response.
    ///
    /// Ported from `get_password()` (`client.py:961-964`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_password(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.password().get_password(&network_id).await
    }

    /// Gets firmware-update information — returns the raw Eero API response.
    ///
    /// Ported from `get_updates()` (`client.py:966-969`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_updates(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.updates().get_updates(&network_id).await
    }

    /// Gets Eero Plus/Eero Secure subscription status — returns the raw Eero API response.
    ///
    /// Ported from `get_premium_status()` (`client.py:1015-1018`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_premium_status(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.networks().get_premium_status(&network_id).await
    }

    // ==================== LED & Nightlight ====================

    /// Gets LED status for an Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_led_status()` (`client.py:1032-1037`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]. `network_id` is still resolved and validated even though it
    /// is never forwarded to the underlying request: [`crate::endpoints::EerosApi::get_led_status`]
    /// drops it entirely, since Python's own `network_id` parameter here is unused (see that
    /// method's own docs) — this method keeps resolving it anyway, purely for call-signature and
    /// validation parity with `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_led_status(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, false).await?;
        self.api.eeros().get_led_status(eero_id).await
    }

    /// Gets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `get_nightlight()` (`client.py:1059-1064`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`]; see [`Client::get_led_status`] for why `network_id` is
    /// resolved but not forwarded.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_nightlight(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, false).await?;
        self.api.eeros().get_nightlight(eero_id).await
    }

    // ==================== Backup Network ====================

    /// Gets backup-network configuration — returns the raw Eero API response.
    ///
    /// Ported from `get_backup_network()` (`client.py:1098-1101`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_backup_network(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.backup().get_backup_network(&network_id).await
    }

    /// Gets backup-network status — returns the raw Eero API response.
    ///
    /// Ported from `get_backup_status()` (`client.py:1103-1106`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_backup_status(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.backup().get_backup_status(&network_id).await
    }

    // ==================== Schedule ====================

    /// Gets a profile's schedule — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_schedule()` (`client.py:1129-1134`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`]. Note the parameter order: `profile_id` first,
    /// `network_id` second, matching `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_profile_schedule(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .get_profile_schedule(&network_id, profile_id)
            .await
    }

    // ==================== DNS ====================

    /// Gets DNS settings — returns the raw Eero API response.
    ///
    /// Ported from `get_dns_settings()` (`client.py:1173-1176`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_dns_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.dns().get_dns_settings(&network_id).await
    }

    // ==================== SQM ====================

    /// Gets Smart Queue Management settings — returns the raw Eero API response.
    ///
    /// Ported from `get_sqm_settings()` (`client.py:1204-1207`). `auto_discover = false` — see
    /// [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_sqm_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.sqm().get_sqm_settings(&network_id).await
    }

    // ==================== Security ====================

    /// Gets security settings — returns the raw Eero API response.
    ///
    /// Ported from `get_security_settings()` (`client.py:1276-1279`). `auto_discover = false` —
    /// see [`Client::get_diagnostics`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_security_settings(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.security().get_security_settings(&network_id).await
    }

    // ==================== Blocked Applications ====================

    /// Gets a profile's blocked applications (Eero Plus feature) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_blocked_applications()` (`client.py:1332-1337`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. Note the parameter order: `profile_id` first,
    /// `network_id` second, matching `client.py`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_blocked_applications(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .profiles()
            .get_blocked_applications(&network_id, profile_id)
            .await
    }

    // ==================== Profile Devices ====================

    /// Gets a profile's data "including devices" — returns the raw Eero API response.
    ///
    /// Ported from `get_profile_devices()` (`client.py:1355-1360`). **`auto_discover = true`**
    /// — the module docs' "brief gotcha G5" exception: unlike every other method in this section
    /// (`get_diagnostics` through `get_blocked_applications`, all `auto_discover = false`), this
    /// method and, in phase 5, `set_profile_devices` (`client.py:1362-1372`) call
    /// `_ensure_network_id(network_id)` with no `auto_discover` argument at all, i.e. Python's
    /// default `True`. Do not "fix" this to `false` to match its neighbours — it would be a
    /// behavioural change, not a cleanup.
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent, no preferred network is set, *and*
    /// auto-discovery via [`Client::get_networks`] finds nothing. Otherwise, whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn get_profile_devices(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        // Brief gotcha G5 (see this method's doc comment): `true`, not `false`, deliberately.
        let network_id = self.ensure_network_id(network_id, true).await?;
        self.api
            .profiles()
            .get_profile_devices(&network_id, profile_id)
            .await
    }

    // ==================== Eeros (uncached pass-through) ====================

    /// Gets information about a specific Eero device — returns the raw Eero API response.
    ///
    /// Ported from `get_eero()` (`client.py:388-408`). Unlike [`Client::get_eeros`] (the list),
    /// this single-item getter is **not** cached (brief §2.1/G9's asymmetry note is about
    /// `get_device_priority` specifically, but the same "list is cached, single item is not"
    /// shape applies here by construction — `get_eero` was never one of the eight cached
    /// getters in the first place). `auto_discover = true` (Python omits the argument, matching
    /// `get_network`/`get_eeros`/etc., since this method appears before the `client.py:809`
    /// section boundary). `network_id` is resolved and validated but never forwarded to the
    /// underlying request — see [`Client::get_led_status`] for why.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`].
    pub async fn get_eero(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let _network_id = self.ensure_network_id(network_id, true).await?;
        self.api.eeros().get_eero(eero_id).await
    }

    // ============================= Mutating pass-throughs (phase 5) =============================
    //
    // Every method below resolves a network id (and, where relevant, an item id) exactly like the
    // read-only pass-throughs above, delegates to the matching `EeroApi` domain accessor, and —
    // on success only, never before the request completes and never on an `Err` — invalidates
    // whatever cache entries the write can leave stale. Grouped in `client.py`'s own file order,
    // so a diff against the Python source stays easy to follow.
    //
    // A handful of methods below have no `client.py` counterpart at all: `src/endpoints/` already
    // ports every mutating method its equivalent Python `api/*.py` module exposes, but
    // `client.py` itself never grew a wrapper for a few of them (`NetworksApi::reboot_network`,
    // `DnsApi::set_ipv6_dns`/`clear_custom_dns`, `SqmApi::set_sqm_bandwidth`/`set_sqm_auto`,
    // `EerosApi::set_nightlight_brightness`/`set_nightlight_schedule`,
    // `ProfilesApi::update_profile_content_filter`/`update_profile_block_list`,
    // `ScheduleApi::set_weekday_bedtime`/`set_weekend_bedtime`,
    // `InsightsApi::run_insights`, `SupportApi::request_support`,
    // `BlacklistApi::add_to_blacklist`/`remove_from_blacklist`,
    // `BurstReportersApi::create_burst_reporter`, `OUICheckApi::run_ouicheck`). Each is called
    // out at its own doc comment below with the reasoning for the invalidation choice made in the
    // absence of a Python precedent to port — every one of them follows the policy already
    // established for its nearest sibling (same underlying wire resource, the same delegated
    // call, or — for the five domains with no cache bucket at all — the same "invalidate nothing"
    // policy every other bucket-less domain already uses), never a freshly invented policy.
    //
    // `set_device_priority`/`get_device_priority` and every `get_activity*` method are **not**
    // added here: `src/endpoints/devices.rs` deliberately never ported `set_device_priority` (a
    // confirmed server no-op, eero-api #111) and explicitly says it should not be added later "for
    // parity" — there is no endpoint method to delegate to. `get_activity*` is unported for the
    // same reason `crate::endpoints` never grew an `ActivityApi`: every one of those routes 404s
    // upstream on both API versions (eero-api #107).
    //
    // Auth-adjacent (already implemented above, not part of this section): `logout`,
    // `set_session_token`, `clear_session_token` — wired to `clear_cache()`.

    // ==================== Networks ====================

    /// Enables, disables or reconfigures the guest network — returns the raw Eero API response.
    ///
    /// Ported from `set_guest_network` (`eero-api src/eero/client.py:740-766`). Resolves
    /// `network_id` with `auto_discover = true` (Python's default; this call sits before the
    /// `client.py:809` "Diagnostics & Settings" boundary — see `Client::get_diagnostics`'s docs
    /// for that boundary and every method after it). On success, invalidates `network[nid]`
    /// (`client.py:763-764`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no network can be resolved.
    /// Otherwise, whatever status-mapped [`Error`] the request produces. The cache is left
    /// untouched on any `Err`.
    pub async fn set_guest_network(
        &self,
        enabled: bool,
        name: Option<&str>,
        password: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .networks()
            .set_guest_network(&network_id, enabled, name, password)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Runs a speed test on the network — returns the raw Eero API response.
    ///
    /// Ported from `run_speed_test` (`eero-api src/eero/client.py:770-787`). `auto_discover =
    /// true` — see [`Client::set_guest_network`]. On success, invalidates `network[nid]`
    /// (`client.py:784-785`).
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn run_speed_test(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self.api.networks().run_speed_test(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Reboots every Eero node on the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` has never wrapped `NetworksAPI.reboot_network`
    /// (`api/networks.py:134`) on `EeroClient` at all, so there is nothing to port faithfully
    /// here — this method exists only because [`crate::endpoints::NetworksApi::reboot_network`]
    /// does. `auto_discover = true`, matching its two closest siblings,
    /// [`Client::set_guest_network`] and [`Client::run_speed_test`] — both plain, id-only network
    /// actions resolved the same way. On success, invalidates `network[nid]`, for the same reason
    /// `run_speed_test` does even though neither is a settings write in the DNS/SQM/security
    /// sense: a reboot is a network-wide action indistinguishable in kind from its two siblings,
    /// both of which drop this same key. This is a judgement call, not a brief citation — there is
    /// no Python behaviour here to diverge from or match.
    ///
    /// # Errors
    ///
    /// See [`Client::set_guest_network`].
    pub async fn reboot_network(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self.api.networks().reboot_network(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets the network name (SSID) — returns the raw Eero API response.
    ///
    /// Ported from `set_network_name` (`eero-api src/eero/client.py:1020-1028`). Unlike its three
    /// Networks-domain siblings above, this call is **after** the `client.py:809` boundary and so
    /// resolves `network_id` with `auto_discover = false` — see [`Client::get_diagnostics`]. On
    /// success, invalidates `network[nid]` (`client.py:1025-1026`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent and no preferred network is set —
    /// auto-discovery is not attempted. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_network_name(
        &self,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .networks()
            .set_network_name(&network_id, name)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    // ==================== Eeros (mutations) ====================

    /// Reboots a single Eero device — returns the raw Eero API response.
    ///
    /// Ported from `reboot_eero` (`eero-api src/eero/client.py:410-432`). `auto_discover = true`
    /// (before the `client.py:809` boundary, matching [`Client::get_eero`] just above it in this
    /// file). `network_id` is resolved only to build the `eeros[{nid}_eeros]` cache key below —
    /// it is never forwarded to [`crate::endpoints::EerosApi::reboot_eero`], which drops it for
    /// the same reason [`Client::get_led_status`] does (see that method's docs). On success,
    /// invalidates `eeros[{nid}_eeros]` (`client.py:427-430`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn reboot_eero(
        &self,
        eero_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self.api.eeros().reboot_eero(eero_id).await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Turns an Eero device's status LED on or off — returns the raw Eero API response.
    ///
    /// Ported from `set_led` (`eero-api src/eero/client.py:1039-1050`). `auto_discover = false`
    /// — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded; see
    /// [`Client::reboot_eero`]. On success, invalidates `eeros[{nid}_eeros]`
    /// (`client.py:1046-1048`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_led(
        &self,
        eero_id: &str,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.eeros().set_led(eero_id, enabled).await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets an Eero device's status LED brightness — returns the raw Eero API response.
    ///
    /// Ported from `set_led_brightness` (`eero-api src/eero/client.py:1052-1057`). `auto_discover
    /// = false` — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded;
    /// see [`Client::reboot_eero`].
    ///
    /// Divergence from eero-api: Python invalidates nothing here, even though its sibling
    /// [`Client::set_led`] invalidates `eeros[{nid}_eeros]` for the exact same underlying node
    /// object — the behaviour brief calls this out as a likely oversight (gotcha G3), not a
    /// deliberate design choice. This port does not reproduce the gap: on success, this method
    /// also invalidates `eeros[{nid}_eeros]`, exactly like [`Client::set_led`] and
    /// [`Client::set_nightlight`] do. `rust-port-plan.md` §3.8 and `cache.rs`'s own module docs
    /// ("what's new" (b)) name this exact method as one of the two intentional improvements over
    /// Python this crate makes — do not remove this call later thinking it restores parity; it
    /// would reintroduce a bug, not fix one.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_led_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_led_brightness(eero_id, brightness)
            .await?;
        // Divergence from eero-api (rust-port-plan.md §3.8, improvement (b)): Python never
        // invalidates `eeros` after a brightness-only write; this port does, since the field it
        // just wrote lives in the same cached node object `set_led` already invalidates for.
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets nightlight settings for an Eero Beacon device — returns the raw Eero API response.
    ///
    /// Ported from `set_nightlight` (`eero-api src/eero/client.py:1066-1094`). Every setting is
    /// optional and independent, exactly like
    /// [`crate::endpoints::EerosApi::set_nightlight`], which this delegates to unchanged — see
    /// that method's own docs for the brightness clamp and the request body shape. `auto_discover
    /// = false` — see [`Client::get_diagnostics`]. `network_id` is resolved but not forwarded;
    /// see [`Client::reboot_eero`]. On success, invalidates `eeros[{nid}_eeros]`
    /// (`client.py:1090-1092`).
    ///
    /// Takes eight parameters (including the receiver), one more than
    /// [`crate::endpoints::EerosApi::set_nightlight`]'s seven, to additionally mirror
    /// `client.py:1066-1075`'s own `network_id` parameter — the same reasoning that method's own
    /// docs give for not splitting its six independent, self-describing scalar settings into a
    /// params struct: doing so would break the direct correspondence with the Python source
    /// without making any call site clearer.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "nightlight", .. }` if every one of `enabled`,
    /// `brightness`, `schedule_enabled`, `schedule_on`, `schedule_off` and
    /// `ambient_light_enabled` is `None` — see
    /// [`crate::endpoints::EerosApi::set_nightlight`]'s own docs for why this port refuses rather
    /// than fabricating Python's local `400` envelope. Otherwise see [`Client::get_diagnostics`].
    /// The cache is left untouched on any `Err`.
    #[allow(clippy::too_many_arguments)] // mirrors client.py:1066-1075's own 8-parameter signature, like EerosApi::set_nightlight's identical allow
    pub async fn set_nightlight(
        &self,
        eero_id: &str,
        enabled: Option<bool>,
        brightness: Option<i32>,
        schedule_enabled: Option<bool>,
        schedule_on: Option<&str>,
        schedule_off: Option<&str>,
        ambient_light_enabled: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight(
                eero_id,
                enabled,
                brightness,
                schedule_enabled,
                schedule_on,
                schedule_off,
                ambient_light_enabled,
            )
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets only the nightlight brightness for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `EerosAPI.set_nightlight_brightness`
    /// (`api/eeros.py:282`) on `EeroClient`, even though it wraps its sibling
    /// [`Client::set_nightlight`]. [`crate::endpoints::EerosApi::set_nightlight_brightness`]
    /// itself is a pure delegator to `EerosApi::set_nightlight` — the exact same wire call — so
    /// this method's cache behaviour simply follows suit: `auto_discover = false`, and on success
    /// invalidates `eeros[{nid}_eeros]`, identically to [`Client::set_nightlight`]. This is not a
    /// third invented improvement over Python; it is the same wire call `set_nightlight` already
    /// makes, which already invalidates that bucket.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. Never returns `Error::Validation`: a `brightness` value
    /// is always supplied here. The cache is left untouched on any `Err`.
    pub async fn set_nightlight_brightness(
        &self,
        eero_id: &str,
        brightness: i32,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight_brightness(eero_id, brightness)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    /// Sets only the nightlight schedule for an Eero Beacon device — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent — see [`Client::set_nightlight_brightness`]'s docs, which apply
    /// identically here: [`crate::endpoints::EerosApi::set_nightlight_schedule`] delegates to the
    /// same `EerosApi::set_nightlight` call [`Client::set_nightlight`] itself uses, so this method
    /// follows the same cache behaviour: `auto_discover = false`, invalidates `eeros[{nid}_eeros]`
    /// on success.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. Never returns `Error::Validation`: `enabled` is always
    /// supplied here. The cache is left untouched on any `Err`.
    pub async fn set_nightlight_schedule(
        &self,
        eero_id: &str,
        enabled: bool,
        on_time: Option<&str>,
        off_time: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .eeros()
            .set_nightlight_schedule(eero_id, enabled, on_time, off_time)
            .await?;
        self.cache.invalidate(&CacheKey::eeros(network_id.as_str()));
        Ok(response)
    }

    // ==================== Devices (mutations) ====================

    /// Sets a nickname for a device — returns the raw Eero API response.
    ///
    /// Ported from `set_device_nickname` (`eero-api src/eero/client.py:494-514`).
    /// `auto_discover = true` (before the `client.py:809` boundary). On success, invalidates
    /// both `devices[{nid}_{did}]` and `devices[{nid}_devices]` via `Client::invalidate_device_cache`
    /// — the same two-key drop Python's `_invalidate_device_cache` performs
    /// (`client.py:567-575`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn set_device_nickname(
        &self,
        device_id: &str,
        nickname: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .set_device_nickname(&network_id, device_id, nickname)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Blocks or unblocks a device via the `/blacklist` resource — returns the raw Eero API
    /// response.
    ///
    /// Ported from `block_device` (`eero-api src/eero/client.py:516-543`). `auto_discover =
    /// true`. [`crate::endpoints::DevicesApi::block_device`] itself performs up to two round
    /// trips (a `GET` to resolve the MAC, then the blacklist `POST`, when `blocked == true`);
    /// this method simply awaits that single call and invalidates once it succeeds overall. On
    /// success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`
    /// (`client.py:540-541`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. Also returns `Error::Api { status: 502, .. }` if `blocked ==
    /// true` and the device's MAC cannot be resolved — see
    /// [`crate::endpoints::DevicesApi::block_device`]'s own docs. The cache is left untouched on
    /// any `Err`.
    pub async fn block_device(
        &self,
        device_id: &str,
        blocked: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .block_device(&network_id, device_id, blocked)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    /// Pauses or unpauses internet access for a device — returns the raw Eero API response.
    ///
    /// Ported from `pause_device` (`eero-api src/eero/client.py:545-565`). `auto_discover =
    /// true`. On success, invalidates both `devices[{nid}_{did}]` and `devices[{nid}_devices]`
    /// (`client.py:562-563`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn pause_device(
        &self,
        device_id: &str,
        paused: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .devices()
            .pause_device(&network_id, device_id, paused)
            .await?;
        self.invalidate_device_cache(network_id.as_str(), device_id);
        Ok(response)
    }

    // ==================== Profiles (mutations) ====================

    /// Creates a new profile on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_profile` (`eero-api src/eero/client.py:675-691`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_profiles]` only — no single-profile key
    /// exists yet for whatever id the server just assigned (`client.py:689`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn create_profile(
        &self,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .create_profile(&network_id, name)
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Renames an existing profile — returns the raw Eero API response.
    ///
    /// Ported from `rename_profile` (`eero-api src/eero/client.py:693-713`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` via
    /// `Client::invalidate_profile_cache` — Python calls both `_invalidate_profile_cache` (which
    /// already drops both keys itself) *and* `_invalidate_profiles_list_cache` at this call site
    /// (`client.py:710-711`), a redundant double-drop of the list key the behaviour brief calls
    /// out as harmless; this port calls the combined helper once, for the identical end state.
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn rename_profile(
        &self,
        profile_id: &str,
        name: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .rename_profile(&network_id, profile_id, name)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Deletes a profile from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_profile` (`eero-api src/eero/client.py:715-736`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]` —
    /// see [`Client::rename_profile`]'s docs for the same redundant-double-drop note
    /// (`client.py:733-734`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn delete_profile(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .delete_profile(&network_id, profile_id)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Pauses or unpauses internet access for a profile — returns the raw Eero API response.
    ///
    /// Ported from `pause_profile` (`eero-api src/eero/client.py:637-657`). `auto_discover =
    /// true`. On success, invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`
    /// (`client.py:654-655`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_network`]. The cache is left untouched on any `Err`.
    pub async fn pause_profile(
        &self,
        profile_id: &str,
        paused: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .pause_profile(&network_id, profile_id, paused)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    // ==================== Diagnostics & Settings (mutations) ====================

    /// Runs network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `run_diagnostics` (`eero-api src/eero/client.py:814-817`). `auto_discover =
    /// false` — see [`Client::get_diagnostics`]. Invalidates nothing: `diagnostics` has no cache
    /// bucket at all (behaviour brief §2.1), and Python's own method body has no
    /// `del self._cache[...]` call to reproduce.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_diagnostics(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.diagnostics().run_diagnostics(&network_id).await
    }

    // ==================== Reservations ====================

    /// Creates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_reservation` (`eero-api src/eero/client.py:884-889`). `auto_discover =
    /// false`. Invalidates nothing: there is no `reservations` cache bucket (behaviour brief
    /// §2.1).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn create_reservation(
        &self,
        reservation_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .create_reservation(&network_id, reservation_data)
            .await
    }

    /// Updates a DHCP reservation on the network — returns the raw Eero API response.
    ///
    /// Ported from `update_reservation` (`eero-api src/eero/client.py:891-901`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_reservation`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn update_reservation(
        &self,
        reservation_id: &str,
        reservation_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .update_reservation(&network_id, reservation_id, reservation_data)
            .await
    }

    /// Deletes a DHCP reservation from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_reservation` (`eero-api src/eero/client.py:903-908`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_reservation`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_reservation(
        &self,
        reservation_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .reservations()
            .delete_reservation(&network_id, reservation_id)
            .await
    }

    // ==================== Forwards ====================

    /// Creates a port forward on the network — returns the raw Eero API response.
    ///
    /// Ported from `create_forward` (`eero-api src/eero/client.py:915-920`). `auto_discover =
    /// false`. Invalidates nothing: there is no `forwards` cache bucket (behaviour brief §2.1).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn create_forward(
        &self,
        forward_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .forwards()
            .create_forward(&network_id, forward_data)
            .await
    }

    /// Deletes a port forward from the network — returns the raw Eero API response.
    ///
    /// Ported from `delete_forward` (`eero-api src/eero/client.py:922-927`). `auto_discover =
    /// false`. Invalidates nothing — see [`Client::create_forward`].
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn delete_forward(
        &self,
        forward_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .forwards()
            .delete_forward(&network_id, forward_id)
            .await
    }

    // ==================== Insights, Support, Blacklist, Burst Reporters, OUICheck ====================
    //
    // None of these five has a `client.py` counterpart — `eero-api` never wrapped
    // `InsightsAPI.run_insights` (`api/insights.py:96`), `SupportAPI.request_support`
    // (`api/support.py:61`), `BlacklistAPI.add_to_blacklist`/`remove_from_blacklist`
    // (`api/blacklist.py:56,81`), `BurstReportersAPI.create_burst_reporter`
    // (`api/burst_reporters.py:56`) or `OUICheckAPI.run_ouicheck` (`api/ouicheck.py:56`) on
    // `EeroClient`, confirmed absent by grepping the committed `client.py` for each name — see
    // this file's own module docs for why they are added here anyway (item 1 of this phase's
    // task: every mutating endpoint method under `src/endpoints/` gets a `Client` wrapper, with
    // or without a `client.py` precedent). `auto_discover = false`, matching every sibling `GET`
    // in this same region of `client.py` (`get_insights`, `get_support`, `get_blacklist`,
    // `get_burst_reporters`, `get_ouicheck`, all between the `client.py:809` boundary and
    // `get_premium_status`). None of these five domains has a cache bucket at all (behaviour
    // brief §2.1), so every method below invalidates nothing — the same "no bucket to keep
    // consistent" reasoning as [`Client::create_reservation`]/[`Client::create_forward`] above,
    // not a judgement call specific to any one of them.

    /// Runs an on-demand insights computation — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_insights(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.insights().run_insights(&network_id).await
    }

    /// Submits a support request for the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. `request_data` is
    /// forwarded verbatim as the request body, exactly like
    /// [`crate::endpoints::SupportApi::request_support`] itself. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn request_support(
        &self,
        request_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .support()
            .request_support(&network_id, request_data)
            .await
    }

    /// Adds a device (by MAC) to the blacklist — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. Distinct from
    /// [`Client::block_device`], which routes through this same `/blacklist` resource but also
    /// resolves the device's MAC first and invalidates the device cache; this method is the bare
    /// [`crate::endpoints::BlacklistApi::add_to_blacklist`] pass-through, `mac` unchanged and
    /// unresolved. Invalidates nothing — there is no `blacklist` cache bucket (behaviour brief
    /// §2.1), and `get_blacklist` is not one of the eight cached getters either.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn add_to_blacklist(
        &self,
        mac: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .blacklist()
            .add_to_blacklist(&network_id, mac)
            .await
    }

    /// Removes a device from the blacklist — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::add_to_blacklist`]'s docs, which apply
    /// identically here. `mac_or_device_id` is forwarded unchanged, exactly like
    /// [`crate::endpoints::BlacklistApi::remove_from_blacklist`] itself. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn remove_from_blacklist(
        &self,
        mac_or_device_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .blacklist()
            .remove_from_blacklist(&network_id, mac_or_device_id)
            .await
    }

    /// Creates a burst reporter on the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. `reporter_data` is
    /// forwarded verbatim as the request body, exactly like
    /// [`crate::endpoints::BurstReportersApi::create_burst_reporter`] itself. Invalidates
    /// nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn create_burst_reporter(
        &self,
        reporter_data: Value,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .burst_reporters()
            .create_burst_reporter(&network_id, reporter_data)
            .await
    }

    /// Runs an OUI (MAC vendor) check on the network — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see this group's banner comment above. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn run_ouicheck(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api.ouicheck().run_ouicheck(&network_id).await
    }

    // ==================== Backup Network (mutations) ====================

    /// Enables or disables the backup network — returns the raw Eero API response.
    ///
    /// Ported from `set_backup_network` (`eero-api src/eero/client.py:1108-1113`).
    /// `auto_discover = false`. Invalidates nothing: Python's method body has no
    /// `del self._cache[...]` call (behaviour brief §2, "Verified to invalidate nothing at
    /// all"), and there is no `backup` cache bucket to keep consistent with
    /// `get_backup_network`/`get_backup_status` either way, since neither of those is one of the
    /// eight cached getters.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_backup_network(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .set_backup_network(&network_id, enabled)
            .await
    }

    /// Configures the backup network (enable/disable and/or a phone number) — returns the raw
    /// Eero API response.
    ///
    /// Ported from `configure_backup_network` (`eero-api src/eero/client.py:1115-1125`).
    /// `auto_discover = false`. Invalidates nothing — see [`Client::set_backup_network`].
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if both `enabled` and `phone_number` are `None` — see
    /// [`crate::endpoints::BackupApi::configure_backup_network`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`].
    pub async fn configure_backup_network(
        &self,
        enabled: Option<bool>,
        phone_number: Option<&str>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .backup()
            .configure_backup_network(&network_id, enabled, phone_number)
            .await
    }

    // ==================== Schedule (mutations) ====================

    /// Sets a profile's internet-access schedule — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_schedule` (`eero-api src/eero/client.py:1136-1148`).
    /// `auto_discover = false`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1147`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_profile_schedule(
        &self,
        profile_id: &str,
        time_blocks: &[Value],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .schedule()
            .set_profile_schedule(&network_id, profile_id, time_blocks)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Enables bedtime mode for a profile — returns the raw Eero API response.
    ///
    /// Ported from `enable_bedtime` (`eero-api src/eero/client.py:1150-1162`). `auto_discover =
    /// false`.
    ///
    /// Faithful no-op: invalidates nothing, even though this delegates to
    /// [`crate::endpoints::ScheduleApi::enable_bedtime`], which itself calls the same
    /// `set_profile_schedule` wire endpoint [`Client::set_profile_schedule`] uses (and which
    /// *does* invalidate the profile cache when called directly). This asymmetry is a real,
    /// pre-existing Python inconsistency (behaviour brief §2: `enable_bedtime` invalidates
    /// "nothing, despite `set_profile_schedule` ... invalidating the profile cache"), **not** one
    /// of this port's two deliberate improvements — do not "fix" this later thinking it was
    /// missed. A profile fetched via [`Client::get_profile`] right after this call can return
    /// stale schedule data until the TTL naturally expires, exactly as in Python.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn enable_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        days: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .enable_bedtime(&network_id, profile_id, start_time, end_time, days)
            .await
    }

    /// Clears every schedule on a profile — returns the raw Eero API response.
    ///
    /// Ported from `clear_profile_schedule` (`eero-api src/eero/client.py:1164-1169`).
    /// `auto_discover = false`. Faithful no-op: invalidates nothing, for the same reason
    /// [`Client::enable_bedtime`] does not — see that method's docs.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn clear_profile_schedule(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .clear_profile_schedule(&network_id, profile_id)
            .await
    }

    /// Sets bedtime for weekdays only (Monday through Friday) — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `ScheduleAPI.set_weekday_bedtime`
    /// (`api/schedule.py:169`) on `EeroClient`.
    /// [`crate::endpoints::ScheduleApi::set_weekday_bedtime`] is a pure delegator to
    /// `ScheduleApi::enable_bedtime`, so this method's cache behaviour matches
    /// [`Client::enable_bedtime`] exactly: `auto_discover = false`, invalidates nothing — a
    /// faithful no-op for the same bedtime-family reason.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_weekday_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .set_weekday_bedtime(&network_id, profile_id, start_time, end_time)
            .await
    }

    /// Sets bedtime for weekends only (Saturday and Sunday) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::set_weekday_bedtime`]'s docs, which apply
    /// identically here. `auto_discover = false`. Invalidates nothing.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn set_weekend_bedtime(
        &self,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .schedule()
            .set_weekend_bedtime(&network_id, profile_id, start_time, end_time)
            .await
    }

    // ==================== DNS (mutations) ====================
    //
    // Divergence from eero-api: every setter in this group invalidates `network[nid]`, even
    // though not one of them does in Python (behaviour brief §2/§2.1, gotcha G3). `DnsApi`'s six
    // setters all `PUT networks/{nid}/settings` through the shared `put_network_settings` call
    // site (`src/endpoints/networks.rs`) — the exact same resource `Client::get_network` caches —
    // so a `get_network()` call served from cache right after any of these would echo back
    // pre-write data for as long as the TTL lasts. `rust-port-plan.md` §3.8 and `cache.rs`'s own
    // module docs ("what's new" (b)) name this as one of this port's two deliberate improvements
    // over Python; it applies to every DNS setter uniformly, including the two below with no
    // `client.py` precedent at all — not just the three Python happens to wrap.

    /// Enables or disables DNS caching — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_caching` (`eero-api src/eero/client.py:1178-1183`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_dns_caching(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().set_dns_caching(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets custom DNS servers — returns the raw Eero API response.
    ///
    /// Ported from `set_custom_dns` (`eero-api src/eero/client.py:1185-1190`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_custom_dns(
        &self,
        dns_servers: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .dns()
            .set_custom_dns(&network_id, dns_servers)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Clears custom DNS servers (reverts to automatic DNS) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped `DnsAPI.clear_custom_dns`
    /// (`api/dns.py:126`) on `EeroClient`. [`crate::endpoints::DnsApi::clear_custom_dns`] itself
    /// delegates to `DnsApi::set_custom_dns([])` — the same resource [`Client::set_custom_dns`]
    /// mutates — so this method follows the same cache behaviour: `auto_discover = false`,
    /// invalidates `network[nid]` on success.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn clear_custom_dns(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().clear_custom_dns(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets DNS mode to a preset or a custom server list — returns the raw Eero API response.
    ///
    /// Ported from `set_dns_mode` (`eero-api src/eero/client.py:1192-1200`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "mode", .. }` for an unrecognised `mode` — see
    /// [`crate::endpoints::DnsApi::set_dns_mode`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_dns_mode(
        &self,
        mode: &str,
        custom_servers: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .dns()
            .set_dns_mode(&network_id, mode, custom_servers)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables IPv6 DNS (upstream only) — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs; the same reasoning
    /// applies here. `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_ipv6_dns(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.dns().set_ipv6_dns(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    // ==================== SQM (mutations) ====================
    //
    // Divergence from eero-api: every setter in this group invalidates `network[nid]`, for the
    // exact same reason as the DNS group above — `SqmApi`'s four setters all `PUT
    // networks/{nid}/settings` through the same shared call site `get_network` reads. See the DNS
    // group's banner comment for the full rationale; it applies here unchanged.

    /// Enables or disables SQM (Smart Queue Management) — returns the raw Eero API response.
    ///
    /// Ported from `set_sqm_enabled` (`eero-api src/eero/client.py:1209-1214`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_enabled(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.sqm().set_sqm_enabled(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets SQM upload/download bandwidth limits — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs for the reasoning
    /// pattern this follows. `auto_discover = false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_bandwidth(
        &self,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .sqm()
            .set_sqm_bandwidth(&network_id, upload_mbps, download_mbps)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Configures SQM (enable/disable plus optional bandwidth limits) in one call — returns the
    /// raw Eero API response.
    ///
    /// Ported from `configure_sqm` (`eero-api src/eero/client.py:1216-1225`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn configure_sqm(
        &self,
        enabled: bool,
        upload_mbps: Option<u32>,
        download_mbps: Option<u32>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .sqm()
            .configure_sqm(&network_id, enabled, upload_mbps, download_mbps)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Sets SQM to automatic mode — returns the raw Eero API response.
    ///
    /// No `client.py` precedent — see [`Client::clear_custom_dns`]'s docs. `auto_discover =
    /// false`. On success, invalidates `network[nid]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_sqm_auto(&self, network_id: Option<&str>) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.sqm().set_sqm_auto(&network_id).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    // ==================== Security (mutations) ====================
    //
    // Divergence from eero-api: every setter in this group invalidates `network[nid]`, for the
    // same reason as DNS/SQM above — `SecurityApi`'s six setters all `PUT
    // networks/{nid}/settings` through the same shared call site `get_network` reads. See the DNS
    // group's banner comment for the full rationale.

    /// Enables or disables WPA3 encryption — returns the raw Eero API response.
    ///
    /// Ported from `set_wpa3` (`eero-api src/eero/client.py:1281-1284`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_wpa3(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_wpa3(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables band steering — returns the raw Eero API response.
    ///
    /// Ported from `set_band_steering` (`eero-api src/eero/client.py:1286-1291`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_band_steering(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .security()
            .set_band_steering(&network_id, enabled)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables `UPnP` — returns the raw Eero API response.
    ///
    /// Ported from `set_upnp` (`eero-api src/eero/client.py:1293-1296`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_upnp(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_upnp(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables IPv6 (both upstream and downstream) — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_ipv6` (`eero-api src/eero/client.py:1298-1301`). `auto_discover =
    /// false`. On success, invalidates `network[nid]` — see this group's banner comment above.
    /// Contrast with [`Client::set_ipv6_dns`], which sets only the upstream flag.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_ipv6(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_ipv6(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Enables or disables Thread — returns the raw Eero API response.
    ///
    /// Ported from `set_thread_enabled` (`eero-api src/eero/client.py:1303-1308`), which
    /// delegates to [`crate::endpoints::SecurityApi::set_thread`] — the client-level name keeps
    /// Python's own `set_thread_enabled`, not `set_thread`, verbatim (`client.py:1303`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_thread_enabled(
        &self,
        enabled: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self.api.security().set_thread(&network_id, enabled).await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    /// Configures multiple security settings in one call — returns the raw Eero API response.
    ///
    /// Ported from `configure_security` (`eero-api src/eero/client.py:1310-1328`).
    /// `auto_discover = false`. On success, invalidates `network[nid]` — see this group's banner
    /// comment above.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation` if every argument is `None` — see
    /// [`crate::endpoints::SecurityApi::configure_security`]'s own docs. Otherwise see
    /// [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn configure_security(
        &self,
        wpa3: Option<bool>,
        band_steering: Option<bool>,
        upnp: Option<bool>,
        ipv6: Option<bool>,
        thread: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .security()
            .configure_security(&network_id, wpa3, band_steering, upnp, ipv6, thread)
            .await?;
        self.cache
            .invalidate(&CacheKey::network(network_id.as_str()));
        Ok(response)
    }

    // ==================== Blocked Applications & Profile Content (mutations) ====================

    /// Sets the blocked applications (Eero Plus feature) for a profile — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_blocked_applications` (`eero-api src/eero/client.py:1339-1351`).
    /// `auto_discover = false`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1350`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_blocked_applications(
        &self,
        profile_id: &str,
        applications: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .set_blocked_applications(&network_id, profile_id, applications)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Updates a profile's content-filtering settings — returns the raw Eero API response.
    ///
    /// No `client.py` precedent: `eero-api` never wrapped
    /// `ProfilesAPI.update_profile_content_filter` (`api/profiles.py:179`) on `EeroClient`.
    /// [`crate::endpoints::ProfilesApi::update_profile_content_filter`] `PUT`s the exact same
    /// `networks/{nid}/profiles/{pid}` resource [`Client::set_blocked_applications`],
    /// [`Client::rename_profile`], [`Client::pause_profile`] and [`Client::set_profile_devices`]
    /// all mutate — every one of which invalidates the profile cache in Python — so this method
    /// follows the policy already established for that resource rather than inventing a new one:
    /// `auto_discover = false` (grouped here with [`Client::set_blocked_applications`], not with
    /// the `auto_discover = true` "Profile Devices" section at the very end of `client.py`), and
    /// on success invalidates `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn update_profile_content_filter(
        &self,
        profile_id: &str,
        filters: &[(&str, bool)],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .update_profile_content_filter(&network_id, profile_id, filters)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Updates a profile's custom domain block or allow list — returns the raw Eero API
    /// response.
    ///
    /// No `client.py` precedent — see [`Client::update_profile_content_filter`]'s docs, which
    /// apply identically here: [`crate::endpoints::ProfilesApi::update_profile_block_list`]
    /// `PUT`s the same profile resource. `auto_discover = false`. On success, invalidates
    /// `profiles[{nid}_{pid}]` and `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn update_profile_block_list(
        &self,
        profile_id: &str,
        domains: &[&str],
        block: bool,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .profiles()
            .update_profile_block_list(&network_id, profile_id, domains, block)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    // ==================== Profile Devices ====================

    /// Sets the devices assigned to a profile — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_devices` (`eero-api src/eero/client.py:1362-1372`).
    /// **`auto_discover = true`** — the same brief gotcha G5 exception
    /// [`Client::get_profile_devices`]'s docs describe: this is one of the two methods at the
    /// very end of `client.py` that call `_ensure_network_id(network_id)` with no
    /// `auto_discover` argument at all, unlike every other method from `get_diagnostics` through
    /// `set_blocked_applications`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]` (`client.py:1371`).
    ///
    /// # Errors
    ///
    /// [`Error::MissingNetworkId`] if `network_id` is absent, no preferred network is set, *and*
    /// auto-discovery finds nothing. Otherwise, whatever status-mapped [`Error`] the request
    /// produces. The cache is left untouched on any `Err`.
    pub async fn set_profile_devices(
        &self,
        profile_id: &str,
        device_urls: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        // Brief gotcha G5 (see this method's doc comment): `true`, not `false`, deliberately —
        // matching `Client::get_profile_devices` above.
        let network_id = self.ensure_network_id(network_id, true).await?;
        let response = self
            .api
            .profiles()
            .set_profile_devices(&network_id, profile_id, device_urls)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    // ============================= Cache-invalidation helpers =============================
    //
    // Mirror Python's own private helpers (`client.py:567-575, 659-667, 669-673`) exactly: each
    // drops precisely the keys Python drops, no more. Where a deliberate `network[nid]`/
    // `eeros[{nid}_eeros]` drop is added on top of Python (the two documented improvements above),
    // that extra call is made directly at the write site rather than folded into a helper here —
    // there is no Python helper for it to mirror.

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
fn extract_network_id(entry: &Value) -> Option<String> {
    if let Some(id) = entry
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return Some(id.to_owned());
    }
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    crate::util::id_from_url(url).ok()
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
    http: Option<reqwest::Client>,
    base_url: Option<String>,
    user_agent: Option<String>,
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
            session: None,
            store: None,
            storage_failures: StorageFailures::default(),
            cache_ttl: consts::DEFAULT_CACHE_TTL,
        }
    }
}

impl ClientBuilder {
    /// Supplies a fully-configured `reqwest::Client` instead of letting [`ClientBuilder::build`]
    /// construct one. Forwarded to [`crate::transport::TransportBuilder::http`] — see that
    /// method's docs for the safety guarantees a caller-supplied client discards.
    #[must_use]
    pub fn http(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
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
            transport_builder = transport_builder.http(http);
        }
        if let Some(base_url) = self.base_url {
            transport_builder = transport_builder.base_url(base_url);
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
    //! [`non_empty`]). HTTP-level behaviour (cache hits, the `/account` fallback end to end,
    //! network-id resolution against a live mock, `ClientBuilder::build`'s credential-store load)
    //! is covered by `tests/client.rs`, per `.claude/rules/testing.md`.

    use super::{extract_account_networks, extract_network_id, extract_networks_list, non_empty};
    use serde_json::json;

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
}
