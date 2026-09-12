//! The `EeroApi` aggregator: one auth API plus the 25 domain endpoint modules, composed over a
//! single [`Transport`].
//!
//! Ported from `eero-api`'s `EeroAPI` class (`src/eero/api/__init__.py:36-123`). `EeroApi` is
//! the layer the not-yet-built `Client` facade (phase 4) sits on top of, adding a cache and
//! network-id resolution; a consumer who wants neither uses `EeroApi` directly, exactly like the
//! Python split described in the crate's architecture notes §2.
//!
//! # Composition, not endpoint logic
//!
//! This module only wires things together: it never builds a URL, never touches
//! [`crate::routes`], and never inspects or transforms a response payload. All of that lives in
//! [`crate::endpoints`], one file per Python domain module, exactly as `EeroAPI.__init__`
//! (`api/__init__.py:39-78`) does nothing but construct `AuthAPI` and hand it to every other
//! module's constructor.
//!
//! `activity` is not represented here, matching [`crate::endpoints`] itself: every one of its
//! endpoints 404s on both API versions upstream (eero-api #107), so `ActivityAPI`
//! (`api/__init__.py:8,53`) was never ported.
//!
//! # No async context manager
//!
//! Python's `EeroAPI.__aenter__`/`__aexit__` (`api/__init__.py:80-87`) exist only to forward to
//! `aiohttp`'s `ClientSession` context-manager protocol. `reqwest::Client` has no equivalent
//! lifecycle to manage — it needs no explicit close — so `EeroApi` has no context-manager
//! counterpart at all; a value simply goes out of scope like any other.
//!
//! # `login`/`verify` are deliberately not mirrored
//!
//! Python's `EeroAPI.login`/`EeroAPI.verify` (`api/__init__.py:94-114`) forward to
//! `AuthAPI.login`/`AuthAPI.verify`, which stash an *unverified* token before it has been
//! confirmed by a code. This port does not have (or want) that shape: the interactive login
//! handshake lives entirely in `crate::auth::flow`'s `LoginFlow`/`PendingLogin` type-state pair
//! (port plan §3.5), which can only ever hand back a `crate::auth::Session` once
//! `PendingLogin::verify` succeeds — an unverified token can never reach an authenticated
//! endpoint by construction. `EeroApi` therefore has no `login`/`verify` methods; obtain a
//! `Session` via `LoginFlow` (or `Session::from_token`/any `crate::storage::CredentialStore`)
//! and build the `Transport` passed to [`EeroApi::new`] from it instead.
//!
//! [`EeroApi::is_authenticated`] and [`EeroApi::logout`] *are* mirrored, since `AuthApi` already
//! exposes both today (`api/__init__.py:89-92,116-122`) — see each method's own docs.
//!
//! # One transport, shared by every sub-API
//!
//! [`EeroApi::new`] builds exactly one `Arc<Transport>` and hands a clone of it to [`AuthApi`]
//! (via [`AuthApi::from_shared`]) and to every one of the 25 domain modules — the same thing
//! Python's `EeroAPI.__init__` does by passing one `session`/`cookie_jar` pair to `AuthAPI` and
//! every other domain class it constructs (`api/__init__.py:39-78`). There is exactly one
//! allocation backing every accessor this type exposes; a session change made through any one of
//! them (e.g. [`AuthApi::logout`] clearing the session, or a 401 triggering
//! [`crate::transport::Transport`]'s one-shot refresh) is immediately observable through all the
//! others.

use std::sync::Arc;

use crate::auth::AuthApi;
use crate::endpoints::{
    ACCompatApi, BackupApi, BlacklistApi, BurstReportersApi, DataUsageApi, DevicesApi,
    DiagnosticsApi, DnsApi, EerosApi, ForwardsApi, InsightsApi, NetworksApi, OUICheckApi,
    PasswordApi, ProfilesApi, ReservationsApi, RoutingApi, ScheduleApi, SecurityApi, SettingsApi,
    SqmApi, SupportApi, ThreadApi, TransferApi, UpdatesApi,
};
use crate::envelope::Envelope;
use crate::error::Error;
use crate::transport::Transport;

/// `eero-api`'s `EeroAPI` (`src/eero/api/__init__.py:36-123`): one [`AuthApi`] plus the 25 ported
/// domain endpoint modules, each sharing the same [`Transport`].
///
/// Build one with [`EeroApi::new`]. See the module docs for what is deliberately not ported
/// (`activity`, the async context manager, `login`/`verify`) and for how every accessor below
/// shares one `Transport`.
///
/// `#[derive(Debug)]` is safe here without a hand-written impl: every field below is either an
/// `Arc<Transport>` or a struct that owns one, and `Transport` already derives `Debug` safely —
/// its only session-shaped field, `session: RwLock<Option<Session>>`, relies on
/// `crate::auth::Session`'s own hand-written `Debug`, which redacts via `secrecy::SecretString`.
/// No field reachable from `EeroApi` carries a token through any other path.
#[derive(Debug)]
pub struct EeroApi {
    /// The transport shared by every accessor below, including `auth` — see the module docs'
    /// "One transport, shared by every sub-API" section.
    transport: Arc<Transport>,
    /// `AuthApi`, sharing [`EeroApi::transport`]'s exact `Arc` allocation (built with
    /// [`AuthApi::from_shared`]).
    auth: AuthApi,
    ac_compat: ACCompatApi,
    backup: BackupApi,
    blacklist: BlacklistApi,
    burst_reporters: BurstReportersApi,
    data_usage: DataUsageApi,
    devices: DevicesApi,
    diagnostics: DiagnosticsApi,
    dns: DnsApi,
    eeros: EerosApi,
    forwards: ForwardsApi,
    insights: InsightsApi,
    networks: NetworksApi,
    ouicheck: OUICheckApi,
    password: PasswordApi,
    profiles: ProfilesApi,
    reservations: ReservationsApi,
    routing: RoutingApi,
    schedule: ScheduleApi,
    security: SecurityApi,
    settings: SettingsApi,
    sqm: SqmApi,
    support: SupportApi,
    thread: ThreadApi,
    transfer: TransferApi,
    updates: UpdatesApi,
}

impl EeroApi {
    /// Wraps `transport`, eagerly constructing every domain module (`api/__init__.py:52-78`
    /// constructs each sub-API up front in `__init__` rather than lazily; this port does the
    /// same — each domain module is just an `Arc<Transport>` clone plus a marker type, so
    /// laziness would add complexity for no measurable benefit).
    ///
    /// Takes an owned [`Transport`] rather than an `Arc<Transport>` so that this is directly
    /// reachable from a `Transport` a caller already built with no extra wrapping — the
    /// ergonomic shape the not-yet-built `Client` facade (phase 4, which will own exactly one
    /// `Transport`) needs. `transport` is wrapped in a single `Arc` here and a clone of that same
    /// `Arc` is handed to [`AuthApi`] and to every domain module below — see the module docs.
    #[must_use]
    pub fn new(transport: Transport) -> Self {
        let transport = Arc::new(transport);
        Self {
            auth: AuthApi::from_shared(Arc::clone(&transport)),
            ac_compat: ACCompatApi::new(Arc::clone(&transport)),
            backup: BackupApi::new(Arc::clone(&transport)),
            blacklist: BlacklistApi::new(Arc::clone(&transport)),
            burst_reporters: BurstReportersApi::new(Arc::clone(&transport)),
            data_usage: DataUsageApi::new(Arc::clone(&transport)),
            devices: DevicesApi::new(Arc::clone(&transport)),
            diagnostics: DiagnosticsApi::new(Arc::clone(&transport)),
            dns: DnsApi::new(Arc::clone(&transport)),
            eeros: EerosApi::new(Arc::clone(&transport)),
            forwards: ForwardsApi::new(Arc::clone(&transport)),
            insights: InsightsApi::new(Arc::clone(&transport)),
            networks: NetworksApi::new(Arc::clone(&transport)),
            ouicheck: OUICheckApi::new(Arc::clone(&transport)),
            password: PasswordApi::new(Arc::clone(&transport)),
            profiles: ProfilesApi::new(Arc::clone(&transport)),
            reservations: ReservationsApi::new(Arc::clone(&transport)),
            routing: RoutingApi::new(Arc::clone(&transport)),
            schedule: ScheduleApi::new(Arc::clone(&transport)),
            security: SecurityApi::new(Arc::clone(&transport)),
            settings: SettingsApi::new(Arc::clone(&transport)),
            sqm: SqmApi::new(Arc::clone(&transport)),
            support: SupportApi::new(Arc::clone(&transport)),
            thread: ThreadApi::new(Arc::clone(&transport)),
            transfer: TransferApi::new(Arc::clone(&transport)),
            updates: UpdatesApi::new(Arc::clone(&transport)),
            transport,
        }
    }

    /// Borrows the [`Transport`] shared by every accessor below, including [`EeroApi::auth`].
    #[must_use]
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Borrows the [`AuthApi`] (`api/__init__.py:52`, `self.auth = AuthAPI(...)`).
    ///
    /// Shares [`EeroApi::transport`]'s exact `Transport` — a session change made through this
    /// `AuthApi` (e.g. [`AuthApi::logout`], [`AuthApi::set_session_token`]) is immediately
    /// observable through every domain-module accessor below, and vice versa.
    #[must_use]
    pub fn auth(&self) -> &AuthApi {
        &self.auth
    }

    /// Whether [`EeroApi::transport`] currently carries a valid, unexpired session
    /// (`api/__init__.py:89-92`, the `is_authenticated` property: `return self.auth.is_authenticated`).
    ///
    /// Delegates to [`EeroApi::auth`]'s own [`AuthApi::is_authenticated`], matching Python
    /// exactly — safe now that `auth()` shares [`EeroApi::transport`]'s exact `Transport`.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_authenticated()
    }

    /// Logs out (`api/__init__.py:116-122`, `return await self.auth.logout()`).
    ///
    /// Delegates to [`EeroApi::auth`]'s own `logout`, which clears the session
    /// [`EeroApi::transport`] and every domain-module accessor also observe, since they all share
    /// the same `Transport`.
    pub async fn logout(&self) -> Result<Envelope, Error> {
        self.auth.logout().await
    }

    /// AC-compatibility endpoints (`api/__init__.py:76`, `self.ac_compat = ACCompatAPI(...)`).
    #[must_use]
    pub fn ac_compat(&self) -> &ACCompatApi {
        &self.ac_compat
    }

    /// Backup endpoints (`api/__init__.py:54`, `self.backup = BackupAPI(...)`).
    #[must_use]
    pub fn backup(&self) -> &BackupApi {
        &self.backup
    }

    /// Blacklist (device-blocking) endpoints (`api/__init__.py:70`,
    /// `self.blacklist = BlacklistAPI(...)`).
    #[must_use]
    pub fn blacklist(&self) -> &BlacklistApi {
        &self.blacklist
    }

    /// Burst-reporter endpoints (`api/__init__.py:74`,
    /// `self.burst_reporters = BurstReportersAPI(...)`).
    #[must_use]
    pub fn burst_reporters(&self) -> &BurstReportersApi {
        &self.burst_reporters
    }

    /// Data-usage endpoints (`api/__init__.py:75`, `self.data_usage = DataUsageAPI(...)`).
    #[must_use]
    pub fn data_usage(&self) -> &DataUsageApi {
        &self.data_usage
    }

    /// Device endpoints (`api/__init__.py:57`, `self.devices = DevicesAPI(...)`).
    #[must_use]
    pub fn devices(&self) -> &DevicesApi {
        &self.devices
    }

    /// Diagnostics endpoints (`api/__init__.py:63`, `self.diagnostics = DiagnosticsAPI(...)`).
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticsApi {
        &self.diagnostics
    }

    /// DNS endpoints (`api/__init__.py:55`, `self.dns = DnsAPI(...)`).
    #[must_use]
    pub fn dns(&self) -> &DnsApi {
        &self.dns
    }

    /// Eero (hardware unit) endpoints (`api/__init__.py:58`, `self.eeros = EerosAPI(...)`).
    #[must_use]
    pub fn eeros(&self) -> &EerosApi {
        &self.eeros
    }

    /// Port-forwarding endpoints (`api/__init__.py:72`, `self.forwards = ForwardsAPI(...)`).
    #[must_use]
    pub fn forwards(&self) -> &ForwardsApi {
        &self.forwards
    }

    /// Insights endpoints (`api/__init__.py:66`, `self.insights = InsightsAPI(...)`).
    #[must_use]
    pub fn insights(&self) -> &InsightsApi {
        &self.insights
    }

    /// Network endpoints (`api/__init__.py:56`, `self.networks = NetworksAPI(...)`).
    #[must_use]
    pub fn networks(&self) -> &NetworksApi {
        &self.networks
    }

    /// OUI-check endpoints (`api/__init__.py:77`, `self.ouicheck = OUICheckAPI(...)`).
    #[must_use]
    pub fn ouicheck(&self) -> &OUICheckApi {
        &self.ouicheck
    }

    /// Password endpoints (`api/__init__.py:78`, `self.password = PasswordAPI(...)`).
    #[must_use]
    pub fn password(&self) -> &PasswordApi {
        &self.password
    }

    /// Profile endpoints (`api/__init__.py:59`, `self.profiles = ProfilesAPI(...)`).
    #[must_use]
    pub fn profiles(&self) -> &ProfilesApi {
        &self.profiles
    }

    /// Reservation (DHCP) endpoints (`api/__init__.py:71`,
    /// `self.reservations = ReservationsAPI(...)`).
    #[must_use]
    pub fn reservations(&self) -> &ReservationsApi {
        &self.reservations
    }

    /// Routing endpoints (`api/__init__.py:67`, `self.routing = RoutingAPI(...)`).
    #[must_use]
    pub fn routing(&self) -> &RoutingApi {
        &self.routing
    }

    /// Schedule endpoints (`api/__init__.py:60`, `self.schedule = ScheduleAPI(...)`).
    #[must_use]
    pub fn schedule(&self) -> &ScheduleApi {
        &self.schedule
    }

    /// Security endpoints (`api/__init__.py:61`, `self.security = SecurityAPI(...)`).
    #[must_use]
    pub fn security(&self) -> &SecurityApi {
        &self.security
    }

    /// Settings endpoints (`api/__init__.py:64`, `self.settings = SettingsAPI(...)`).
    #[must_use]
    pub fn settings(&self) -> &SettingsApi {
        &self.settings
    }

    /// Smart Queue Management endpoints (`api/__init__.py:62`, `self.sqm = SqmAPI(...)`).
    #[must_use]
    pub fn sqm(&self) -> &SqmApi {
        &self.sqm
    }

    /// Support endpoints (`api/__init__.py:69`, `self.support = SupportAPI(...)`).
    #[must_use]
    pub fn support(&self) -> &SupportApi {
        &self.support
    }

    /// Thread (network protocol) endpoints (`api/__init__.py:68`,
    /// `self.thread = ThreadAPI(...)`).
    #[must_use]
    pub fn thread(&self) -> &ThreadApi {
        &self.thread
    }

    /// Transfer endpoints (`api/__init__.py:73`, `self.transfer = TransferAPI(...)`).
    #[must_use]
    pub fn transfer(&self) -> &TransferApi {
        &self.transfer
    }

    /// Firmware-update endpoints (`api/__init__.py:65`, `self.updates = UpdatesAPI(...)`).
    #[must_use]
    pub fn updates(&self) -> &UpdatesApi {
        &self.updates
    }
}
