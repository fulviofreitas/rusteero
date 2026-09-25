//! HTTP integration tests for the `EeroApi` aggregator (`src/api.rs`), ported from
//! `eero-api src/eero/api/__init__.py`.
//!
//! Two things matter here that no other suite in this crate exercises:
//!
//! - That each of the 38 accessors (`auth` + 37 domain modules) is wired to *its own* sub-API,
//!   not a copy-pasted neighbour — [`each_domain_accessor_routes_only_to_its_own_wire_endpoint`]
//!   drives one real call through several different accessors against a single mock server and
//!   pins each one's path; [`every_accessor_is_reachable_and_returns_its_own_sub_api_type`]
//!   mechanically touches all 38 and checks each returns a value of its own, distinct type.
//! - That `EeroApi`'s `Debug` output — reachable transitively through 38 fields, each holding an
//!   `Arc<Transport>` (or, for `auth`, owning one directly) — never prints a session token, per
//!   this crate's rule against logging anything that can carry one.

mod common;

use rusteero::EeroApi;
use rusteero::auth::Session;
use rusteero::transport::Transport;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// A network id shared by every fixture/mock in this file — its exact value carries no meaning,
/// it only has to match between a `Mock`'s `path` and the argument passed to the method under
/// test.
const NETWORK_ID: &str = "network-0001";

/// Builds an [`EeroApi`] over a [`Transport`] pointed at `mock` and authenticated with
/// [`TEST_TOKEN`].
fn eero_api(mock: &MockEero) -> EeroApi {
    EeroApi::new(mock.transport_with_token(TEST_TOKEN))
}

/// Builds an [`EeroApi`] over a `Transport` that talks to nothing (no `base_url` override, no
/// mock server): used by the two tests below that never make a network call and so have no need
/// to pay for spinning up a `wiremock::MockServer`.
fn eero_api_offline() -> EeroApi {
    let transport = Transport::builder()
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .expect("default transport configuration (no overrides) always builds successfully");
    EeroApi::new(transport)
}

// ===================== cross-wiring: routes to the right sub-API =====================

/// Drives one real call through four different domain accessors — `networks`, `devices`,
/// `eeros`, `profiles` — against a single mock server, and pins each one's exact wire path.
///
/// This is the test that would catch an accessor (or its backing field in `EeroApi::new`) wired
/// to the wrong sub-API: a copy-paste error across 25 near-identical accessors is easy to make
/// and nothing else in this crate's test suite would notice it, since every other suite tests
/// exactly one domain module's own type directly, never `EeroApi` itself.
///
/// Seen red: `EeroApi::new`'s `devices` field was temporarily changed from
/// `DevicesApi::new(Arc::clone(&transport))` to
/// `DevicesApi::new(Arc::new(detached_auth_transport()))` (the same "wrong, unconfigured
/// `Transport`" shape `auth()` deliberately uses, given to the wrong accessor). With that change
/// in place, `networks` still succeeded, but the `devices` call returned
/// `Error::Authentication("Not authenticated")` before ever reaching wiremock (no session on the
/// detached transport), which propagated out via `?` — wiremock's `MockServer` guard then
/// panicked at drop time reporting the `devices`/`eeros`/`profiles` mocks as never hit (the last
/// two never even ran, since the early return short-circuited the rest of this function). The
/// change was then reverted and this test passes again.
#[tokio::test]
async fn each_domain_accessor_routes_only_to_its_own_wire_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/2.2/networks/{NETWORK_ID}/devices")))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/2.2/networks/{NETWORK_ID}/eeros")))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/2.2/networks/{NETWORK_ID}/profiles")))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eero_api(&mock);

    let networks_env = api.networks().get_networks().await?;
    assert_eq!(networks_env.into_value(), fixture_json("networks.json"));

    let devices_env = api.devices().get_devices(NETWORK_ID).await?;
    assert_eq!(devices_env.into_value(), fixture_json("devices.json"));

    let eeros_env = api.eeros().get_eeros(NETWORK_ID).await?;
    assert_eq!(eeros_env.into_value(), fixture_json("eeros.json"));

    let profiles_env = api.profiles().get_profiles(NETWORK_ID).await?;
    assert_eq!(profiles_env.into_value(), fixture_json("profiles.json"));

    Ok(())
}

// ===================== auth() shares EeroApi::transport, not a detached copy =====================

/// Proves [`EeroApi::auth`] operates on the exact same `Transport` as every domain-module
/// accessor, rather than a separate, unconfigured one nothing else can observe.
///
/// `logout()` is the sharpest probe available: it requires a valid session before it will even
/// attempt a network call ([`crate::transport::Transport::send`]'s own precondition, exercised
/// directly in `src/auth/mod.rs`'s unit tests), so if `auth()`'s `Transport` were a detached,
/// session-less snapshot, this call would fail locally with `Error::Authentication("Not
/// authenticated")` before ever reaching the mock server — and wiremock's `.expect(1)` guard
/// would then panic at drop time reporting the mock as never hit. Requiring the request to
/// actually arrive, carrying the session cookie every domain-module call in this file also uses,
/// is what pins the transport identity: an `EeroApi::auth()` backed by a *different* transport
/// object entirely (even one that happened to carry the same token) could still assemble the
/// correct cookie, so the call succeeding is necessary but the earlier reasoning about the local
/// precondition is what actually rules out a detached transport.
///
/// Seen red (before this crate's shared-transport fix, with [`EeroApi::is_authenticated`] already
/// changed to delegate to `auth()` as it does today): `EeroApi::new` built `auth` from a
/// separate, freshly built `Transport::builder().build()` with no session and no `base_url`
/// override, while every domain module got the caller's real, configured transport. Under that
/// code this test panicked at the very first assertion, `assert!(api.is_authenticated())`, with
/// `assertion failed: api.is_authenticated()` — `EeroApi::is_authenticated` delegated to
/// `auth().is_authenticated()`, and the detached transport backing `auth()` never saw the session
/// this test configures, so it reported `false` before `logout()` (and therefore the mock) was
/// ever reached. Verified directly for this report by re-introducing that exact detached-transport
/// construction, confirming this test fails with the panic above, then reverting it.
#[tokio::test]
async fn auth_shares_the_same_transport_as_the_domain_modules() -> anyhow::Result<()> {
    let mock = MockEero::start().await;

    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":{}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = eero_api(&mock);

    // Before logging out: both views of "authenticated" must agree, since they now read the same
    // transport.
    assert!(api.is_authenticated());
    assert!(api.auth().is_authenticated());
    assert_eq!(api.is_authenticated(), api.auth().is_authenticated());

    // The network call itself only succeeds if `auth()`'s transport actually carries the session
    // configured on `api` — a detached, session-less transport fails this locally (see this
    // test's doc comment) before the mock is ever reached.
    let logged_out = api.auth().logout().await?;
    assert!(logged_out);

    // After logging out: the shared session is cleared, and both views agree again.
    assert!(!api.is_authenticated());
    assert!(!api.auth().is_authenticated());
    assert_eq!(api.is_authenticated(), api.auth().is_authenticated());

    Ok(())
}

// ===================== reachability: all 38 accessors, all distinct =====================

/// Mechanically touches all 38 accessors (`auth` plus the 37 domain modules) and checks each
/// returns a value whose `Debug` output is prefixed by its own, distinct struct name.
///
/// `#[derive(Debug)]` on a named struct always renders as `TypeName { .. }`, so this doubles as a
/// type-identity check: if an accessor's body ever read the wrong field (a mistake that only
/// fails to compile when the two fields' types differ, and every one of these 38 fields already
/// *is* a distinct type — the one case that would NOT be caught at compile time is two same-named
/// return types both reading whichever single field happens to have that type), the prefix would
/// name the wrong struct and the assertion below would fail.
///
/// Seen red: `EeroApi::ac_compat` was temporarily changed from
/// `fn ac_compat(&self) -> &ACCompatApi { &self.ac_compat }` to
/// `fn ac_compat(&self) -> &EerosApi { &self.eeros }` (a self-consistent lie that still compiles,
/// since it now returns the one field whose type matches its own new signature). With that change
/// in place this test failed on the `ac_compat` case with a `"EerosApi { .. }"`-prefixed string
/// instead of the expected `"ACCompatApi"` prefix. The change was then reverted and this test
/// passes again.
#[test]
fn every_accessor_is_reachable_and_returns_its_own_sub_api_type() {
    let api = eero_api_offline();

    let cases: [(&str, String); 38] = [
        ("AuthApi", format!("{:?}", api.auth())),
        ("ACCompatApi", format!("{:?}", api.ac_compat())),
        ("AccountApi", format!("{:?}", api.account())),
        ("BackupApi", format!("{:?}", api.backup())),
        (
            "BackupAccessPointsApi",
            format!("{:?}", api.backup_access_points()),
        ),
        ("BlacklistApi", format!("{:?}", api.blacklist())),
        ("BurstReportersApi", format!("{:?}", api.burst_reporters())),
        ("DataUsageApi", format!("{:?}", api.data_usage())),
        ("DdnsApi", format!("{:?}", api.ddns())),
        ("DevicesApi", format!("{:?}", api.devices())),
        ("DhcpApi", format!("{:?}", api.dhcp())),
        ("DiagnosticsApi", format!("{:?}", api.diagnostics())),
        ("DnsApi", format!("{:?}", api.dns())),
        ("DnsPoliciesApi", format!("{:?}", api.dns_policies())),
        ("EerosApi", format!("{:?}", api.eeros())),
        ("EntitlementsApi", format!("{:?}", api.entitlements())),
        ("EventsApi", format!("{:?}", api.events())),
        ("ForwardsApi", format!("{:?}", api.forwards())),
        ("InsightsApi", format!("{:?}", api.insights())),
        ("MembersApi", format!("{:?}", api.members())),
        ("NetworksApi", format!("{:?}", api.networks())),
        ("NotificationsApi", format!("{:?}", api.notifications())),
        ("OUICheckApi", format!("{:?}", api.ouicheck())),
        ("PermissionsApi", format!("{:?}", api.permissions())),
        ("PowerSavingApi", format!("{:?}", api.power_saving())),
        ("ProfilesApi", format!("{:?}", api.profiles())),
        ("ReservationsApi", format!("{:?}", api.reservations())),
        ("RoutingApi", format!("{:?}", api.routing())),
        ("ScheduleApi", format!("{:?}", api.schedule())),
        ("SecurityApi", format!("{:?}", api.security())),
        ("SqmApi", format!("{:?}", api.sqm())),
        ("SubnetsApi", format!("{:?}", api.subnets())),
        ("SupportApi", format!("{:?}", api.support())),
        ("ThreadApi", format!("{:?}", api.thread())),
        ("TransferApi", format!("{:?}", api.transfer())),
        ("UpdatesApi", format!("{:?}", api.updates())),
        ("WanApi", format!("{:?}", api.wan())),
        ("Wpa3Api", format!("{:?}", api.wpa3())),
    ];

    for (expected_type, debug_output) in &cases {
        assert!(
            debug_output.starts_with(expected_type),
            "expected a {expected_type}-prefixed Debug output, got {debug_output:?}"
        );
    }

    // Every prefix observed above must be distinct — catches a duplicated wiring where two
    // different accessors happen to have been given the same underlying sub-API type.
    let mut seen: Vec<&str> = cases
        .iter()
        .map(|(expected_type, _)| *expected_type)
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        cases.len(),
        "two accessors reported the same sub-API type"
    );
}

// ===================== Debug redaction =====================

/// `format!("{:?}", api)` must never contain the session token, however deeply it is nested
/// (through `transport`, `auth`, and each of the 25 domain modules' own `Arc<Transport>`).
///
/// Seen red: the assertion below was temporarily inverted to `assert!(debug_output.contains(..))`
/// against the real, unmodified `EeroApi`. It failed (as expected, since `Session`'s own
/// hand-written `Debug` impl already redacts the token to `"[REDACTED]"` everywhere it appears in
/// the printed output), which confirms the assertion is not vacuously true. Reverted to the real
/// `!contains` assertion, which passes — this test exists to catch a *future* regression in that
/// redaction chain, not one in `EeroApi` itself today.
#[test]
fn debug_output_never_contains_the_session_token() {
    let api = eero_api_offline();
    let debug_output = format!("{api:?}");
    assert!(
        !debug_output.contains(TEST_TOKEN),
        "EeroApi's Debug output leaked the session token: {debug_output}"
    );
}
