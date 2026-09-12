//! P5.12 `Client` integration suite: mutating pass-throughs and cache invalidation.
//!
//! Covers, against a local `wiremock` server per the crate's testing conventions: a representative
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
//! testing caching at all (the crate's testing conventions' "Assertion Patterns").

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

// ===================== Security review findings (F1-F5) =====================
//
// One test per finding from the security merge review. Each was written first, run against the
// unfixed code and observed RED, then GREEN again once the corresponding fix landed — see the
// task's final report for the captured `cargo test` output of both runs (F1, F4, F5).

/// **F1**: `AuthApi::logout` clears the in-memory session and credential store
/// unconditionally, on every outcome (see `src/auth/mod.rs`'s own docs) — but the previous
/// `Client::logout` only cleared *this client's cache* when the network call itself succeeded.
/// That let a cached `get_network`/`get_account`/`get_devices`/`get_profiles` keep serving
/// pre-logout data for the rest of the TTL even though `is_authenticated()` had already flipped
/// to `false`. This primes the `network` bucket, forces `logout()` to fail on the wire (500),
/// and asserts the cache can no longer serve the second `get_network` call: with both the cache
/// and the session gone, it must fail with `Error::Authentication`, not silently return the
/// stale envelope.
#[tokio::test]
async fn logout_failure_still_clears_the_cache() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/logout"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;

    let err = client.logout().await.unwrap_err();
    assert!(matches!(
        err,
        rusteero::error::Error::Api { status: 500, .. }
    ));
    assert!(!client.is_authenticated());

    // Before the fix this served the pre-logout envelope straight from cache (`Ok`), with no
    // auth check at all. After the fix the cache is empty and the session is gone, so this must
    // fail closed rather than leak the earlier authenticated response.
    let after = client.get_network(Some("network-0001"), false).await;
    assert!(matches!(
        after,
        Err(rusteero::error::Error::Authentication(_))
    ));
    Ok(())
}

/// **F2**: `add_to_blacklist` issues the identical `POST .../blacklist` call
/// `block_device(blocked: true)` makes, and `block_device` already invalidates the `devices`
/// bucket on success. Before the fix, `add_to_blacklist` invalidated nothing, so a `get_devices`
/// call right after would keep reporting the device as unblocked for the rest of the TTL.
#[tokio::test]
async fn add_to_blacklist_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_devices(Some("network-0001"), false).await?;
    client
        .add_to_blacklist("AA:BB:CC:00:00:01", Some("network-0001"))
        .await?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}

/// **F2**: `remove_from_blacklist` must invalidate the `devices` bucket too, for the same reason
/// as `add_to_blacklist` above.
#[tokio::test]
async fn remove_from_blacklist_invalidates_the_devices_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/devices"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("devices.json")))
        .expect(2)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/blacklist/AA:BB:CC:00:00:01",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_devices(Some("network-0001"), false).await?;
    client
        .remove_from_blacklist("AA:BB:CC:00:00:01", Some("network-0001"))
        .await?;
    client.get_devices(Some("network-0001"), false).await?;
    Ok(())
}

/// **F3**: `enable_bedtime` delegates to `ScheduleApi::set_profile_schedule` — the same `PUT
/// .../profiles/{pid}` that `Client::set_profile_schedule` invalidates the profile cache for.
/// Before the fix this was a faithful-to-Python no-op; the fix makes it match its own delegate.
#[tokio::test]
async fn enable_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
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
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .enable_bedtime("profile-0001", "21:00", "07:00", None, Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `clear_profile_schedule` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn clear_profile_schedule_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
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
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .clear_profile_schedule("profile-0001", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `set_weekday_bedtime` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn set_weekday_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
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
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .set_weekday_bedtime("profile-0001", "21:00", "07:00", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F3**: `set_weekend_bedtime` must invalidate the profile cache too — see
/// `enable_bedtime_invalidates_the_profile_bucket` above.
#[tokio::test]
async fn set_weekend_bedtime_invalidates_the_profile_bucket() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
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
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    client
        .set_weekend_bedtime("profile-0001", "21:00", "07:00", Some("network-0001"))
        .await?;
    client
        .get_profile("profile-0001", Some("network-0001"), false)
        .await?;
    Ok(())
}

/// **F4**: every caller-supplied key outside `VALID_CONTENT_FILTER_KEYS` is dropped client-side
/// before the request body is built (parity with Python). Before the fix, nothing guarded
/// against the resulting map being empty: a caller who only passed a misspelled key (e.g.
/// `block_adult_content` instead of `block_adult`) got a `200 OK` for `PUT {"content_filter":
/// {}}}`, which — if the server replaces rather than merges that nested object — silently clears
/// every content filter on the profile. This asserts the call now fails closed with
/// `Error::Validation` *before* any request is sent (`.expect(0)` on the PUT mock).
#[tokio::test]
async fn update_profile_content_filter_with_only_unknown_keys_is_a_validation_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .update_profile_content_filter(
            "profile-0001",
            &[("block_adult_content", true)],
            Some("network-0001"),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        rusteero::error::Error::Validation { ref field, .. } if field == "filters"
    ));
    Ok(())
}

/// **F5**: `sanitize_body_for_error` redacts a parseable JSON error body by *key* — a value that
/// looks like a credential but sits under an unrelated key (e.g. a guest Wi-Fi password echoed
/// back verbatim in a `400`'s `error` field) previously passed through untouched, because the
/// raw-text `looks_sensitive` fallback was only reachable for bodies that failed to parse as
/// JSON at all. This asserts the resulting `Error`'s `Display` never contains the submitted
/// password.
#[tokio::test]
async fn a_password_echoed_under_a_non_sensitive_key_is_suppressed_from_the_error()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/guestnetwork"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"meta":{"code":400,"error":"Invalid password: hunter2"}}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let err = client
        .set_guest_network(true, None, Some("hunter2"), Some("network-0001"))
        .await
        .unwrap_err();
    let rendered = err.to_string();
    assert!(!rendered.contains("hunter2"));
    Ok(())
}
