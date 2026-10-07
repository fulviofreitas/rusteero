//! The `EeroApi` aggregator: one auth API plus the 37 domain endpoint modules, composed over a
//! single [`Transport`].
//!
//! Ported from `eero-api`'s `EeroAPI` class (`src/eero/api/__init__.py:36-123`). `EeroApi` is
//! the layer [`crate::client::Client`] sits on top of, adding a cache and
//! network-id resolution; a consumer who wants neither uses `EeroApi` directly, exactly like the
//! Python split.
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
//! `settings` and `password` (`SettingsAPI`/`PasswordAPI`) are likewise not represented here:
//! both were removed upstream in `eero-api` v8.0.0.
//! The 14 domain modules new in v8.0.0 — `account`, `backup_access_points`, `ddns`, `dhcp`,
//! `dns_policies`, `entitlements`, `events`, `members`, `notifications`, `permissions`,
//! `power_saving`, `subnets`, `wan`, `wpa3` — are wired up below with their own endpoint
//! methods.
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
//! handshake lives entirely in `crate::auth::flow`'s `LoginFlow`/`PendingLogin` type-state pair,
//! which can only ever hand back a `crate::auth::Session` once
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
//! (via [`AuthApi::from_shared`]) and to every one of the 37 domain modules — the same thing
//! Python's `EeroAPI.__init__` does by passing one `session`/`cookie_jar` pair to `AuthAPI` and
//! every other domain class it constructs (`api/__init__.py:39-78`). There is exactly one
//! allocation backing every accessor this type exposes; a session change made through any one of
//! them (e.g. [`AuthApi::logout`] clearing the session, or a 401 triggering
//! [`crate::transport::Transport`]'s one-shot refresh) is immediately observable through all the
//! others.

use std::sync::Arc;

use crate::auth::AuthApi;
use crate::endpoints::{
    ACCompatApi, AccountApi, BackupAccessPointsApi, BackupApi, BlacklistApi, BurstReportersApi,
    DataUsageApi, DdnsApi, DevicesApi, DhcpApi, DiagnosticsApi, DnsApi, DnsPoliciesApi, EerosApi,
    EntitlementsApi, EventsApi, ForwardsApi, InsightsApi, MembersApi, NetworksApi,
    NotificationsApi, OUICheckApi, PermissionsApi, PowerSavingApi, ProfilesApi, ReservationsApi,
    RoutingApi, ScheduleApi, SecurityApi, SqmApi, SubnetsApi, SupportApi, ThreadApi, TransferApi,
    UpdatesApi, WanApi, Wpa3Api,
};
use crate::error::Error;
use crate::transport::Transport;

/// `eero-api`'s `EeroAPI` (`src/eero/api/__init__.py:36-123`): one [`AuthApi`] plus the 37 ported
/// domain endpoint modules, each sharing the same [`Transport`].
///
/// Build one with [`EeroApi::new`]. See the module docs for what is deliberately not ported
/// (`activity`, `settings`, `password`, the async context manager, `login`/`verify`) and for how
/// every accessor below shares one `Transport`.
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
    account: AccountApi,
    backup: BackupApi,
    backup_access_points: BackupAccessPointsApi,
    blacklist: BlacklistApi,
    burst_reporters: BurstReportersApi,
    data_usage: DataUsageApi,
    ddns: DdnsApi,
    devices: DevicesApi,
    dhcp: DhcpApi,
    diagnostics: DiagnosticsApi,
    dns: DnsApi,
    dns_policies: DnsPoliciesApi,
    eeros: EerosApi,
    entitlements: EntitlementsApi,
    events: EventsApi,
    forwards: ForwardsApi,
    insights: InsightsApi,
    members: MembersApi,
    networks: NetworksApi,
    notifications: NotificationsApi,
    ouicheck: OUICheckApi,
    permissions: PermissionsApi,
    power_saving: PowerSavingApi,
    profiles: ProfilesApi,
    reservations: ReservationsApi,
    routing: RoutingApi,
    schedule: ScheduleApi,
    security: SecurityApi,
    sqm: SqmApi,
    subnets: SubnetsApi,
    support: SupportApi,
    thread: ThreadApi,
    transfer: TransferApi,
    updates: UpdatesApi,
    wan: WanApi,
    wpa3: Wpa3Api,
}

impl EeroApi {
    /// Wraps `transport`, eagerly constructing every domain module (`api/__init__.py:52-78`
    /// constructs each sub-API up front in `__init__` rather than lazily; this port does the
    /// same — each domain module is just an `Arc<Transport>` clone plus a marker type, so
    /// laziness would add complexity for no measurable benefit).
    ///
    /// Takes an owned [`Transport`] rather than an `Arc<Transport>` so that this is directly
    /// reachable from a `Transport` a caller already built with no extra wrapping — the
    /// ergonomic shape [`crate::client::Client`] (which owns exactly one `Transport`) needs.
    /// `transport` is wrapped in a single `Arc` here and a clone of that same
    /// `Arc` is handed to [`AuthApi`] and to every domain module below — see the module docs.
    #[must_use]
    pub fn new(transport: Transport) -> Self {
        let transport = Arc::new(transport);
        Self {
            auth: AuthApi::from_shared(Arc::clone(&transport)),
            ac_compat: ACCompatApi::new(Arc::clone(&transport)),
            account: AccountApi::new(Arc::clone(&transport)),
            backup: BackupApi::new(Arc::clone(&transport)),
            backup_access_points: BackupAccessPointsApi::new(Arc::clone(&transport)),
            blacklist: BlacklistApi::new(Arc::clone(&transport)),
            burst_reporters: BurstReportersApi::new(Arc::clone(&transport)),
            data_usage: DataUsageApi::new(Arc::clone(&transport)),
            ddns: DdnsApi::new(Arc::clone(&transport)),
            devices: DevicesApi::new(Arc::clone(&transport)),
            dhcp: DhcpApi::new(Arc::clone(&transport)),
            diagnostics: DiagnosticsApi::new(Arc::clone(&transport)),
            dns: DnsApi::new(Arc::clone(&transport)),
            dns_policies: DnsPoliciesApi::new(Arc::clone(&transport)),
            eeros: EerosApi::new(Arc::clone(&transport)),
            entitlements: EntitlementsApi::new(Arc::clone(&transport)),
            events: EventsApi::new(Arc::clone(&transport)),
            forwards: ForwardsApi::new(Arc::clone(&transport)),
            insights: InsightsApi::new(Arc::clone(&transport)),
            members: MembersApi::new(Arc::clone(&transport)),
            networks: NetworksApi::new(Arc::clone(&transport)),
            notifications: NotificationsApi::new(Arc::clone(&transport)),
            ouicheck: OUICheckApi::new(Arc::clone(&transport)),
            permissions: PermissionsApi::new(Arc::clone(&transport)),
            power_saving: PowerSavingApi::new(Arc::clone(&transport)),
            profiles: ProfilesApi::new(Arc::clone(&transport)),
            reservations: ReservationsApi::new(Arc::clone(&transport)),
            routing: RoutingApi::new(Arc::clone(&transport)),
            schedule: ScheduleApi::new(Arc::clone(&transport)),
            security: SecurityApi::new(Arc::clone(&transport)),
            sqm: SqmApi::new(Arc::clone(&transport)),
            subnets: SubnetsApi::new(Arc::clone(&transport)),
            support: SupportApi::new(Arc::clone(&transport)),
            thread: ThreadApi::new(Arc::clone(&transport)),
            transfer: TransferApi::new(Arc::clone(&transport)),
            updates: UpdatesApi::new(Arc::clone(&transport)),
            wan: WanApi::new(Arc::clone(&transport)),
            wpa3: Wpa3Api::new(Arc::clone(&transport)),
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
    /// the same `Transport`. Returns `false` (with no network call at all) when no valid session
    /// was configured to begin with; see [`crate::auth::AuthApi::logout`]'s own docs for the full
    /// `v8.0.4` contract this method inherits verbatim.
    pub async fn logout(&self) -> Result<bool, Error> {
        self.auth.logout().await
    }

    /// AC-compatibility endpoints (`api/__init__.py:76`, `self.ac_compat = ACCompatAPI(...)`).
    #[must_use]
    pub fn ac_compat(&self) -> &ACCompatApi {
        &self.ac_compat
    }

    /// Account endpoints (new in v8.0.0, `self.account = AccountAPI(...)`).
    #[must_use]
    pub fn account(&self) -> &AccountApi {
        &self.account
    }

    /// Backup endpoints (`api/__init__.py:54`, `self.backup = BackupAPI(...)`).
    #[must_use]
    pub fn backup(&self) -> &BackupApi {
        &self.backup
    }

    /// Backup access-point endpoints (new in v8.0.0, `self.backup_access_points =
    /// BackupAccessPointsAPI(...)`).
    #[must_use]
    pub fn backup_access_points(&self) -> &BackupAccessPointsApi {
        &self.backup_access_points
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

    /// Dynamic-DNS endpoints (new in v8.0.0, `self.ddns = DdnsAPI(...)`).
    #[must_use]
    pub fn ddns(&self) -> &DdnsApi {
        &self.ddns
    }

    /// Device endpoints (`api/__init__.py:57`, `self.devices = DevicesAPI(...)`).
    #[must_use]
    pub fn devices(&self) -> &DevicesApi {
        &self.devices
    }

    /// DHCP endpoints (new in v8.0.0, `self.dhcp = DhcpAPI(...)`).
    #[must_use]
    pub fn dhcp(&self) -> &DhcpApi {
        &self.dhcp
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

    /// DNS-policy (content filtering) endpoints (new in v8.0.0, `self.dns_policies =
    /// DnsPoliciesAPI(...)`).
    #[must_use]
    pub fn dns_policies(&self) -> &DnsPoliciesApi {
        &self.dns_policies
    }

    /// Eero (hardware unit) endpoints (`api/__init__.py:58`, `self.eeros = EerosAPI(...)`).
    #[must_use]
    pub fn eeros(&self) -> &EerosApi {
        &self.eeros
    }

    /// Entitlement endpoints (new in v8.0.0, `self.entitlements = EntitlementsAPI(...)`).
    #[must_use]
    pub fn entitlements(&self) -> &EntitlementsApi {
        &self.entitlements
    }

    /// Event endpoints (new in v8.0.0, `self.events = EventsAPI(...)`).
    #[must_use]
    pub fn events(&self) -> &EventsApi {
        &self.events
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

    /// Network-member endpoints (new in v8.0.0, `self.members = MembersAPI(...)`).
    #[must_use]
    pub fn members(&self) -> &MembersApi {
        &self.members
    }

    /// Network endpoints (`api/__init__.py:56`, `self.networks = NetworksAPI(...)`).
    #[must_use]
    pub fn networks(&self) -> &NetworksApi {
        &self.networks
    }

    /// Notification endpoints (new in v8.0.0, `self.notifications = NotificationsAPI(...)`).
    #[must_use]
    pub fn notifications(&self) -> &NotificationsApi {
        &self.notifications
    }

    /// OUI-check endpoints (`api/__init__.py:77`, `self.ouicheck = OUICheckAPI(...)`).
    #[must_use]
    pub fn ouicheck(&self) -> &OUICheckApi {
        &self.ouicheck
    }

    /// Permission endpoints (new in v8.0.0, `self.permissions = PermissionsAPI(...)`).
    #[must_use]
    pub fn permissions(&self) -> &PermissionsApi {
        &self.permissions
    }

    /// Power-saving endpoints (new in v8.0.0, `self.power_saving = PowerSavingAPI(...)`).
    #[must_use]
    pub fn power_saving(&self) -> &PowerSavingApi {
        &self.power_saving
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

    /// Smart Queue Management endpoints (`api/__init__.py:62`, `self.sqm = SqmAPI(...)`).
    #[must_use]
    pub fn sqm(&self) -> &SqmApi {
        &self.sqm
    }

    /// Subnet endpoints (new in v8.0.0, `self.subnets = SubnetsAPI(...)`).
    #[must_use]
    pub fn subnets(&self) -> &SubnetsApi {
        &self.subnets
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

    /// WAN endpoints (new in v8.0.0, `self.wan = WanAPI(...)`).
    #[must_use]
    pub fn wan(&self) -> &WanApi {
        &self.wan
    }

    /// WPA3 endpoints (new in v8.0.0, `self.wpa3 = Wpa3API(...)`).
    #[must_use]
    pub fn wpa3(&self) -> &Wpa3Api {
        &self.wpa3
    }
}
