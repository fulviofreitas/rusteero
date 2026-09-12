//! P5.12 `Client` integration suite: mutating pass-throughs and cache invalidation.
//!
//! Covers, against a local `wiremock` server per `.claude/rules/testing.md`: a representative
//! mutation reaching the correct endpoint for each of the four per-network cache buckets (eeros,
//! devices, profiles, network); the core claim of this phase — that a successful write
//! invalidates exactly the cache entries the behaviour brief's §2 table says it should, no more
//! and no less; the two deliberate improvements over Python this port makes
//! (`set_led_brightness` invalidating `eeros`, and a DNS setter invalidating `network[nid]`); a
//! faithful no-op (a setter with no Python-side invalidation, proven to leave an unrelated cached
//! getter untouched, so the invalidation tests above are shown to be targeted rather than a
//! blanket clear); that `account`/`networks` survive a write that invalidates `network[nid]`; and
//! that a *failed* write never invalidates anything.
//!
//! Every invalidation test asserts on the wiremock `.expect(n)` call count of the underlying
//! `GET`, never just the returned envelope — a caching test that only checks the value is not
//! testing caching at all (`.claude/rules/testing.md`'s "Assertion Patterns").

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL — plenty long enough that none of these tests can flake on a slow
/// CI runner crossing a TTL boundary mid-test.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

// ===================== One representative mutation per bucket =====================
//
// Each of these also doubles as the "reaches the right endpoint" check: method, path and (where
// it matters) session cookie are all asserted via the mock's own matchers, and `.expect(1)`
// verifies the client made exactly the request Python would.

#[tokio::test]
async fn set_led_reaches_the_eero_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_led("eero-0001", true, Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("eero.json"));
    Ok(())
}

#[tokio::test]
async fn set_device_nickname_reaches_the_v2_3_device_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_device_nickname("device-0001", "New Name", Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("device.json"));
    Ok(())
}

#[tokio::test]
async fn pause_profile_reaches_the_profile_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .pause_profile("profile-0001", true, Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("profile.json"));
    Ok(())
}

#[tokio::test]
async fn set_network_name_reaches_the_settings_put_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== Targeted invalidation: the core claim =====================
//
// Each of these primes a cached getter (`.expect(1)` on the underlying GET), performs a write
// that the behaviour brief's §2 table says invalidates that same bucket, then calls the getter
// again and asserts the GET mock's overall call count reached 2 — i.e. the second call could not
// have been served from cache.

#[tokio::test]
async fn reboot_eero_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/eeros/eero-0001/reboot"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .reboot_eero("eero-0001", Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn pause_device_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/device-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("device.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_devices(Some("network-0001"), false).await?;
    client
        .pause_device("device-0001", true, Some("network-0001"))
        .await?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn rename_profile_invalidates_the_profiles_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profiles.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_profiles(Some("network-0001"), false).await?;
    client
        .rename_profile("profile-0001", "New Name", Some("network-0001"))
        .await?;
    client.get_profiles(Some("network-0001"), false).await?;
    Ok(())
}

/// **Broken/restored in place** (see this suite's module docs and the task's red/green
/// requirement): the assertion below was temporarily changed to `.expect(1)` (asserting the
/// second `get_network` call is served from cache), run, and observed to fail — RED — because
/// `set_network_name` really does invalidate `network[nid]` and a second network request really
/// is made. It was then restored to `.expect(2)` — GREEN. See this task's final report for the
/// captured `cargo test` output of both runs.
#[tokio::test]
async fn set_network_name_invalidates_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== The two deliberate improvements over Python =====================

#[tokio::test]
async fn set_led_brightness_invalidates_the_eeros_bucket() -> anyhow::Result<()> {
    // Divergence from eero-api (rust-port-plan.md §3.8, improvement (b)): Python's
    // `set_led_brightness` invalidates nothing at all. This test would pass trivially under a
    // blanket "invalidate everything" policy, so see `backup_setter_does_not_invalidate_the_network_bucket`
    // below for the contrasting faithful no-op that proves invalidation here is targeted.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/eeros"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eeros.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/eeros/eero-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("eero.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_eeros(Some("network-0001"), false).await?;
    client
        .set_led_brightness("eero-0001", 42, Some("network-0001"))
        .await?;
    client.get_eeros(Some("network-0001"), false).await?;
    Ok(())
}

#[tokio::test]
async fn dns_setter_invalidates_the_network_bucket() -> anyhow::Result<()> {
    // Divergence from eero-api (rust-port-plan.md §3.8, improvement (a)): Python's DNS/SQM/
    // security setters invalidate nothing, even though they `PUT` the exact resource
    // `get_network` caches.
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.set_dns_caching(true, Some("network-0001")).await?;
    client.get_network(Some("network-0001"), false).await?;
    Ok(())
}

// ===================== Faithful no-op: proves invalidation is targeted =====================

/// **Broken/restored in place**: the assertion below was temporarily changed to `.expect(2)`
/// (asserting the write *does* invalidate, matching every other test in this file), run, and
/// observed to fail — RED — because `set_backup_network` faithfully invalidates nothing and the
/// second `get_network` call really is served from cache. Restored to `.expect(1)` — GREEN. See
/// this task's final report for the captured output of both runs.
///
/// This is the test that proves `set_led_brightness`/DNS invalidation above is targeted rather
/// than every write clearing every bucket: `set_backup_network` PUTs a completely different
/// resource (`networks/{nid}/backup`, not `networks/{nid}/settings`) that Python never wires into
/// any cache invalidation at all (behaviour brief §2, "Verified to invalidate nothing"), and this
/// port does not add that as a third, uninstructed improvement.
#[tokio::test]
async fn backup_setter_does_not_invalidate_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/backup"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .set_backup_network(true, Some("network-0001"))
        .await?;
    let second = client.get_network(Some("network-0001"), false).await?;

    assert_eq!(second.as_value(), &fixture_json("network.json"));
    Ok(())
}

/// A second faithful no-op, for a domain with no cache bucket of its own at all: a DHCP
/// reservation write cannot invalidate anything, because `reservations` was never one of the six
/// buckets `Cache` knows about (behaviour brief §2.1) — this asserts it does not, in passing,
/// touch the unrelated `network[nid]` entry either.
#[tokio::test]
async fn reservation_setter_does_not_invalidate_the_network_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/reservations"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client
        .create_reservation(
            json!({"mac": "AA:BB:CC:00:00:09", "ip": "192.168.4.109"}),
            Some("network-0001"),
        )
        .await?;
    let second = client.get_network(Some("network-0001"), false).await?;

    assert_eq!(second.as_value(), &fixture_json("network.json"));
    Ok(())
}

// ===================== `account`/`networks` survive a network[nid] invalidation =====================

#[tokio::test]
async fn account_and_networks_survive_a_write_that_invalidates_network_nid() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("account.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("networks.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_account(false).await?;
    client.get_networks(false).await?;
    client
        .set_network_name("New SSID", Some("network-0001"))
        .await?;

    let account_again = client.get_account(false).await?;
    let networks_again = client.get_networks(false).await?;

    assert_eq!(account_again.as_value(), &fixture_json("account.json"));
    assert_eq!(networks_again.as_value(), &fixture_json("networks.json"));
    Ok(())
}

// ===================== A failed write never invalidates =====================

#[tokio::test]
async fn a_failed_write_does_not_invalidate_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;

    let err = client
        .set_network_name("New SSID", Some("network-0001"))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        rusteero::error::Error::Api { status: 500, .. }
    ));

    let second = client.get_network(Some("network-0001"), false).await?;
    assert_eq!(second.as_value(), &fixture_json("network.json"));
    Ok(())
}
