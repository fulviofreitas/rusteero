//! `Client` integration suite for the cache's guarantees that cut across domains: a read that is
//! overtaken by an invalidation must not refill the cache, and every spelling of a device id
//! shares one cache entry.
//!
//! The in-flight cases hold a read open with a delayed mock (`ResponseTemplate::set_delay`), run
//! the invalidating call while it is in flight, and then check three things: the in-flight read
//! still returns the answer it was given, the next read goes back to the network, and the one
//! after that is served from the cache. Every case runs with the entry absent and with it
//! already cached. Every wiremock `.expect(n)` count is part of the assertion: a test that only
//! compared envelopes would not be testing caching at all.

mod common;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::envelope::Envelope;
use rusteero::error::Error;

const NETWORK: &str = "network-0001";

/// How long the held-open read waits before its response is released when the invalidating call
/// sends nothing (`clear_cache`, `set_session_token`).
const HOLD: Duration = Duration::from_millis(300);

/// As [`HOLD`], for an invalidating call that makes its own request. That request needs a loopback
/// round trip of its own while the read is held, and on a starved runner it can take longer than
/// 300 ms; the test also checks that the call finished first (see
/// [`assert_inflight_read_is_not_cached_when`]).
const NETWORK_HOLD: Duration = Duration::from_millis(1500);

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

fn ok() -> Value {
    json!({"meta": {"code": 200}, "data": {}})
}

// ===================== The cached read under test =====================

#[derive(Debug, Clone, Copy)]
enum Read {
    Account,
    Networks,
    Network,
    Eeros,
    Devices,
    Device,
    Profiles,
    Profile,
}

impl Read {
    fn path(self) -> String {
        match self {
            Self::Account => "/2.2/account".to_owned(),
            Self::Networks => "/2.2/networks".to_owned(),
            Self::Network => format!("/2.2/networks/{NETWORK}"),
            Self::Eeros => format!("/2.2/networks/{NETWORK}/eeros"),
            Self::Devices => format!("/2.2/networks/{NETWORK}/devices"),
            Self::Device => format!("/2.2/networks/{NETWORK}/devices/device-0001"),
            Self::Profiles => format!("/2.2/networks/{NETWORK}/profiles"),
            Self::Profile => format!("/2.2/networks/{NETWORK}/profiles/profile-0001"),
        }
    }

    /// A response body that carries `marker`, so a test can tell which answer it was given.
    /// `get_networks` needs a non-empty network list, or it would go on to `/account`.
    fn body(self, marker: &str) -> Value {
        match self {
            Self::Networks => json!({
                "meta": {"code": 200},
                "data": {"networks": [{"id": NETWORK}], "marker": marker},
            }),
            _ => json!({"meta": {"code": 200}, "data": {"marker": marker}}),
        }
    }

    async fn run(self, client: &Client, refresh_cache: bool) -> Result<Envelope, Error> {
        match self {
            Self::Account => client.get_account(refresh_cache).await,
            Self::Networks => client.get_networks(refresh_cache).await,
            Self::Network => client.get_network(Some(NETWORK), refresh_cache).await,
            Self::Eeros => client.get_eeros(Some(NETWORK), refresh_cache).await,
            Self::Devices => {
                client
                    .get_devices(Some(NETWORK), refresh_cache, None, None)
                    .await
            }
            Self::Device => {
                client
                    .get_device("device-0001", Some(NETWORK), refresh_cache)
                    .await
            }
            Self::Profiles => client.get_profiles(Some(NETWORK), refresh_cache).await,
            Self::Profile => {
                client
                    .get_profile("profile-0001", Some(NETWORK), refresh_cache)
                    .await
            }
        }
    }

    /// Mounts one answer carrying `marker` for this read, optionally held open for `hold`.
    /// Mounts are consumed in order: each one answers a single request, except `unbounded`.
    async fn mount(self, mock: &MockEero, marker: &str, hold: Option<Duration>, unbounded: bool) {
        let mut response = ResponseTemplate::new(200).set_body_json(self.body(marker));
        if let Some(hold) = hold {
            response = response.set_delay(hold);
        }
        let mut mock_builder = Mock::given(method("GET"))
            .and(path(self.path()))
            .and(user_token_header())
            .and(session_cookie())
            .respond_with(response)
            .expect(1);
        if !unbounded {
            mock_builder = mock_builder.up_to_n_times(1);
        }
        mock_builder.mount(&mock.server).await;
    }
}

// ===================== The invalidating call =====================

#[derive(Debug, Clone, Copy)]
enum Write {
    ClearCache,
    SetAccountName,
    VerifyAccountEmail,
    VerifyAccountPhone,
    SetAccountConsents,
    SetNetworkName,
    RebootEero,
    PauseDevice,
    CreateProfile,
    RenameProfile,
    UpdateDeviceViaLinkWithProfile,
    SetSessionToken,
    Logout,
}

impl Write {
    /// The verb and path the write sends, if it sends anything.
    fn endpoint(self) -> Option<(&'static str, String)> {
        match self {
            Self::ClearCache | Self::SetSessionToken => None,
            Self::SetAccountName => Some(("PUT", "/2.2/account/name".to_owned())),
            Self::VerifyAccountEmail => Some(("POST", "/2.2/account/email/verify".to_owned())),
            Self::VerifyAccountPhone => Some(("POST", "/2.2/account/phone/verify".to_owned())),
            Self::SetAccountConsents => Some(("PUT", "/2.2/account/consents".to_owned())),
            Self::SetNetworkName => Some(("PUT", format!("/2.2/networks/{NETWORK}/settings"))),
            Self::RebootEero => Some(("POST", "/2.2/eeros/eero-0001/reboot".to_owned())),
            Self::PauseDevice => Some((
                "PUT",
                format!("/2.3/networks/{NETWORK}/devices/device-0001"),
            )),
            Self::CreateProfile => Some(("POST", format!("/2.2/networks/{NETWORK}/profiles"))),
            Self::RenameProfile => Some((
                "PUT",
                format!("/2.2/networks/{NETWORK}/profiles/profile-0001"),
            )),
            Self::UpdateDeviceViaLinkWithProfile => Some((
                "PUT",
                format!("/2.2/networks/{NETWORK}/devices/device-0001"),
            )),
            Self::Logout => Some(("POST", "/2.2/logout".to_owned())),
        }
    }

    async fn mount(self, mock: &MockEero) {
        if let Some((verb, endpoint)) = self.endpoint() {
            Mock::given(method(verb))
                .and(path(endpoint))
                .and(user_token_header())
                .and(session_cookie())
                .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
                .expect(1)
                .mount(&mock.server)
                .await;
        }
    }

    async fn run(self, client: &Client) -> Result<(), Error> {
        match self {
            Self::ClearCache => client.clear_cache(),
            Self::SetAccountName => {
                client.set_account_name("New Name").await?;
            }
            Self::VerifyAccountEmail => {
                client.verify_account_email("123456").await?;
            }
            Self::VerifyAccountPhone => {
                client.verify_account_phone("654321").await?;
            }
            Self::SetAccountConsents => {
                client.set_account_consents(true).await?;
            }
            Self::SetNetworkName => {
                client.set_network_name("New SSID", Some(NETWORK)).await?;
            }
            Self::RebootEero => {
                client.reboot_eero("eero-0001", Some(NETWORK)).await?;
            }
            Self::PauseDevice => {
                client
                    .pause_device("device-0001", true, Some(NETWORK))
                    .await?;
            }
            Self::CreateProfile => {
                client
                    .create_profile("Guests", None, None, Some(NETWORK))
                    .await?;
            }
            Self::RenameProfile => {
                client
                    .rename_profile("profile-0001", "New Name", Some(NETWORK))
                    .await?;
            }
            Self::UpdateDeviceViaLinkWithProfile => {
                client
                    .update_device_via_link(
                        "device-0001",
                        None,
                        None,
                        Some("profile-0002"),
                        Some(NETWORK),
                    )
                    .await?;
            }
            Self::SetSessionToken => client.set_session_token(TEST_TOKEN).await?,
            Self::Logout => {
                client.logout().await?;
            }
        }
        Ok(())
    }

    /// Whether the write leaves the client without a session.
    fn drops_the_session(self) -> bool {
        matches!(self, Self::Logout)
    }

    /// How long a read has to be held open for this write to overtake it.
    fn hold(self) -> Duration {
        if self.endpoint().is_some() {
            NETWORK_HOLD
        } else {
            HOLD
        }
    }
}

/// Polls until `count` `GET`s to `target` have reached the server.
async fn wait_for_gets(mock: &MockEero, target: &str, count: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let seen = mock
                .server
                .received_requests()
                .await
                .expect("request recording is enabled by default")
                .iter()
                .filter(|request| request.method.as_str() == "GET" && request.url.path() == target)
                .count();
            if seen >= count {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the held-open read should have reached the server");
}

/// Holds `read` open, runs `write` while it is in flight, and checks the three consequences:
/// the in-flight read returns its own answer, the next read fetches fresh data, and the read
/// after that is a cache hit. Runs with the entry absent and with it already cached.
async fn assert_inflight_read_is_not_cached(read: Read, write: Write) {
    assert_inflight_read_is_not_cached_when(read, write, &[false, true]).await;
}

/// As [`assert_inflight_read_is_not_cached`], for the given starting states only.
async fn assert_inflight_read_is_not_cached_when(read: Read, write: Write, cached_states: &[bool]) {
    for &cached_before in cached_states {
        let label = format!("{read:?} vs {write:?} (cached before: {cached_before})");
        let mock = MockEero::start().await;
        let client = client(&mock).await;

        if cached_before {
            read.mount(&mock, "old", None, false).await;
        }
        read.mount(&mock, "stale", Some(write.hold()), false).await;
        read.mount(&mock, "fresh", None, true).await;
        write.mount(&mock).await;

        if cached_before {
            let primed = read.run(&client, false).await.expect("priming read");
            assert_eq!(primed.as_value(), &read.body("old"), "{label}");
        }

        // Without a cached entry a plain read goes to the network; with one it must be forced.
        let in_flight = async {
            let answer = read.run(&client, cached_before).await;
            (answer, Instant::now())
        };
        let invalidate = async {
            wait_for_gets(&mock, &read.path(), if cached_before { 2 } else { 1 }).await;
            let outcome = write.run(&client).await;
            (outcome, Instant::now())
        };
        let ((stale, read_returned), (written, write_returned)) =
            tokio::join!(in_flight, invalidate);

        // If the write only finished after the held read had returned, it did not overtake it,
        // and the assertions below would pass without testing the generation guard at all.
        assert!(
            write_returned <= read_returned,
            "{label}: the invalidating call finished {:?} after the held read returned, so it \
             did not overtake it",
            write_returned.duration_since(read_returned)
        );
        written.unwrap_or_else(|err| panic!("{label}: the invalidating call failed: {err:?}"));
        let stale = stale.unwrap_or_else(|err| panic!("{label}: the held read failed: {err:?}"));
        assert_eq!(
            stale.as_value(),
            &read.body("stale"),
            "{label}: the in-flight read still returns its own answer"
        );

        if write.drops_the_session() {
            client
                .api()
                .auth()
                .set_session_token(TEST_TOKEN)
                .await
                .expect("restoring the session");
        }

        let fresh = read.run(&client, false).await.expect("fresh read");
        assert_eq!(
            fresh.as_value(),
            &read.body("fresh"),
            "{label}: the stale answer was not cached"
        );
        let cached = read.run(&client, false).await.expect("cached read");
        assert_eq!(
            cached.as_value(),
            &read.body("fresh"),
            "{label}: the fresh answer was cached"
        );
    }
}

// ===================== Reads overtaken by an invalidation =====================

#[tokio::test]
async fn get_account_overtaken_by_clear_cache_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Account, Write::ClearCache).await;
}

#[tokio::test]
async fn get_account_overtaken_by_set_account_name_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Account, Write::SetAccountName).await;
}

#[tokio::test]
async fn get_account_overtaken_by_verify_account_email_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Account, Write::VerifyAccountEmail).await;
}

#[tokio::test]
async fn get_account_overtaken_by_verify_account_phone_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Account, Write::VerifyAccountPhone).await;
}

#[tokio::test]
async fn get_account_overtaken_by_set_account_consents_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Account, Write::SetAccountConsents).await;
}

#[tokio::test]
async fn get_networks_overtaken_by_clear_cache_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Networks, Write::ClearCache).await;
}

#[tokio::test]
async fn get_devices_overtaken_by_pause_device_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Devices, Write::PauseDevice).await;
}

#[tokio::test]
async fn get_device_overtaken_by_pause_device_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Device, Write::PauseDevice).await;
}

#[tokio::test]
async fn get_profiles_overtaken_by_create_profile_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Profiles, Write::CreateProfile).await;
}

#[tokio::test]
async fn get_profile_overtaken_by_rename_profile_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Profile, Write::RenameProfile).await;
}

#[tokio::test]
async fn get_profile_overtaken_by_update_device_via_link_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Profile, Write::UpdateDeviceViaLinkWithProfile).await;
}

/// An invalidation of one resource blocks the fill of an unrelated read in flight: the counter
/// is client-wide, which costs at most one extra fetch and never leaves stale data behind.
/// Only the entry-absent start is meaningful here: the write does not touch a cached profiles
/// entry, so with one cached the old answer simply stays.
#[tokio::test]
async fn an_unrelated_invalidation_also_blocks_the_fill() {
    assert_inflight_read_is_not_cached_when(Read::Profiles, Write::PauseDevice, &[false]).await;
}

/// `get_network` and `get_eeros` store through the same guard: both take the cache generation
/// before their request.
#[tokio::test]
async fn get_network_overtaken_by_set_network_name_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Network, Write::SetNetworkName).await;
}

#[tokio::test]
async fn get_eeros_overtaken_by_reboot_eero_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Eeros, Write::RebootEero).await;
}

// ===================== Session switches =====================

/// A read sent under the previous session is handed to its caller but not cached, so the new
/// session never sees the previous account's data from the cache.
#[tokio::test]
async fn a_read_in_flight_across_set_session_token_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Devices, Write::SetSessionToken).await;
}

#[tokio::test]
async fn a_read_in_flight_across_logout_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Devices, Write::Logout).await;
}

/// `clear_auth_data` is a session switch too.
#[tokio::test]
async fn a_read_in_flight_across_clear_auth_data_is_not_cached() -> anyhow::Result<()> {
    for cached_before in [false, true] {
        let mock = MockEero::start().await;
        let client = client(&mock).await;
        if cached_before {
            Read::Devices.mount(&mock, "old", None, false).await;
        }
        Read::Devices.mount(&mock, "stale", Some(HOLD), false).await;
        Read::Devices.mount(&mock, "fresh", None, true).await;

        if cached_before {
            Read::Devices.run(&client, false).await?;
        }
        let in_flight = Read::Devices.run(&client, cached_before);
        let invalidate = async {
            wait_for_gets(
                &mock,
                &Read::Devices.path(),
                if cached_before { 2 } else { 1 },
            )
            .await;
            client.clear_auth_data().await
        };
        let (stale, cleared) = tokio::join!(in_flight, invalidate);
        cleared?;
        assert_eq!(stale?.as_value(), &Read::Devices.body("stale"));

        client.api().auth().set_session_token(TEST_TOKEN).await?;
        let fresh = Read::Devices.run(&client, false).await?;
        assert_eq!(fresh.as_value(), &Read::Devices.body("fresh"));
        let cached = Read::Devices.run(&client, false).await?;
        assert_eq!(cached.as_value(), &Read::Devices.body("fresh"));
    }
    Ok(())
}

// ===================== get_networks still selects a network when its fill is rejected =====================

/// An unrelated invalidation during an in-flight `get_networks` drops the cache fill but not the
/// auto-selection: the caller and the preferred network behave as if nothing happened.
#[tokio::test]
async fn a_rejected_networks_fill_still_selects_the_preferred_network() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let stale = json!({"meta": {"code": 200}, "data": {"networks": [{"id": "n_old"}]}});
    Mock::given(method("GET"))
        .and(path("/2.2/networks"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&stale)
                .set_delay(HOLD),
        )
        .expect(2)
        .mount(&mock.server)
        .await;
    let client = client(&mock).await;

    let (response, ()) = tokio::join!(client.get_networks(true), async {
        wait_for_gets(&mock, "/2.2/networks", 1).await;
        client.clear_cache();
    });

    assert_eq!(response?.as_value(), &stale);
    assert_eq!(client.preferred_network_id().as_deref(), Some("n_old"));
    // The fill was rejected, so the next call reaches the mock again (hence `.expect(2)`).
    client.get_networks(false).await?;
    Ok(())
}

/// A `get_networks` sent under the previous session must not leave the new session targeting
/// the previous account's network: only a session change withholds the selection.
#[tokio::test]
async fn a_networks_read_overtaken_by_a_session_change_selects_nothing() -> anyhow::Result<()> {
    for use_clear_auth_data in [false, true] {
        let mock = MockEero::start().await;
        let old = json!({"meta": {"code": 200}, "data": {"networks": [{"id": "n_old"}]}});
        let new = json!({"meta": {"code": 200}, "data": {"networks": [{"id": "n_new"}]}});
        Mock::given(method("GET"))
            .and(path("/2.2/networks"))
            .and(user_token_header())
            .and(session_cookie())
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(&old)
                    .set_delay(HOLD),
            )
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks"))
            .and(user_token_header())
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_json(&new))
            .expect(1)
            .mount(&mock.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/2.2/networks/n_new/devices"))
            .and(user_token_header())
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
            .expect(1)
            .mount(&mock.server)
            .await;
        let client = client(&mock).await;

        let (response, switched) = tokio::join!(client.get_networks(true), async {
            wait_for_gets(&mock, "/2.2/networks", 1).await;
            if use_clear_auth_data {
                client.clear_auth_data().await
            } else {
                client.set_session_token(TEST_TOKEN).await
            }
        });
        switched?;

        assert_eq!(
            response?.as_value(),
            &old,
            "the caller still gets its answer"
        );
        assert_eq!(
            client.preferred_network_id(),
            None,
            "clear_auth_data: {use_clear_auth_data}"
        );
        if use_clear_auth_data {
            client.api().auth().set_session_token(TEST_TOKEN).await?;
        }

        // The next call without a network id rediscovers the new account's network.
        client.get_devices(None, false, None, None).await?;
        assert_eq!(client.preferred_network_id().as_deref(), Some("n_new"));
    }
    Ok(())
}

// ===================== Canonical device cache keys =====================

/// Mounts `GET` `target` answering a device-shaped body, expected `times` times.
async fn mount_get(mock: &MockEero, target: &str, body: Value, times: u64) {
    Mock::given(method("GET"))
        .and(path(target))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(times)
        .mount(&mock.server)
        .await;
}

fn device_body() -> Value {
    json!({"meta": {"code": 200}, "data": {"mac": "aa:bb:cc:dd:ee:ff"}})
}

const DEVICE_PATH: &str = "/2.2/networks/network-0001/devices/aabbccddeeff";
const DEVICES_PATH: &str = "/2.2/networks/network-0001/devices";

/// A MAC address read as `aabbccddeeff` and then under every other spelling is one cache entry:
/// one request in total.
#[tokio::test]
async fn get_device_under_every_mac_spelling_is_served_from_one_entry() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, DEVICE_PATH, device_body(), 1).await;
    let client = client(&mock).await;

    let first = client
        .get_device("aabbccddeeff", Some(NETWORK), false)
        .await?;
    let absolute = format!("{}{DEVICE_PATH}", mock.uri());
    for spelling in [
        "aabbccddeeff",
        "AA:BB:CC:DD:EE:FF",
        "aa-bb-cc-dd-ee-ff",
        "aabb.ccdd.eeff",
        DEVICE_PATH,
        "/2.2/networks/network-0001/devices/AA:BB:CC:DD:EE:FF/",
        absolute.as_str(),
    ] {
        let again = client.get_device(spelling, Some(NETWORK), false).await?;
        assert_eq!(again, first, "{spelling}");
    }
    Ok(())
}

/// The cache key is canonical; the request still carries the caller's own spelling.
#[tokio::test]
async fn the_request_keeps_the_callers_spelling() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/AA:BB:CC:DD:EE:FF",
        device_body(),
        1,
    )
    .await;
    let client = client(&mock).await;

    client
        .get_device("AA:BB:CC:DD:EE:FF", Some(NETWORK), false)
        .await?;
    // The lowercase spelling is a hit on the entry just written, so no second request.
    client
        .get_device("aabbccddeeff", Some(NETWORK), false)
        .await?;
    Ok(())
}

/// Ids that are not MACs are kept as written, so differently-cased ids stay separate.
#[tokio::test]
async fn ids_that_are_not_macs_are_keyed_as_written() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/d1",
        device_body(),
        1,
    )
    .await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/D1",
        device_body(),
        1,
    )
    .await;
    let client = client(&mock).await;

    client.get_device("d1", Some(NETWORK), false).await?;
    client.get_device("D1", Some(NETWORK), false).await?;
    client.get_device("d1", Some(NETWORK), false).await?;
    client.get_device("D1", Some(NETWORK), false).await?;
    Ok(())
}

fn assert_empty_id(err: &Error) {
    assert!(
        matches!(
            err,
            Error::Validation { field, message, .. }
                if field == "id_or_url" && message == "must be a non-empty string"
        ),
        "{err:?}"
    );
}

async fn assert_no_requests(mock: &MockEero) {
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "{requests:?}");
}

#[tokio::test]
async fn an_empty_device_id_is_rejected_before_any_request() {
    let mock = MockEero::start().await;
    let client = client(&mock).await;

    let err = client
        .get_device("", Some(NETWORK), false)
        .await
        .expect_err("an empty device id is rejected");
    assert_empty_id(&err);

    let err = client
        .update_device_via_link("", Some("x"), None, None, Some(NETWORK))
        .await
        .expect_err("an empty device id is rejected before the all-None check");
    assert_empty_id(&err);

    let err = client
        .update_device_via_link("", None, None, None, Some(NETWORK))
        .await
        .expect_err("the id check comes before the domain's own check");
    assert_empty_id(&err);

    assert_no_requests(&mock).await;
}

/// `block_device("")` fails before the write, where Python posts `mac=` and only then fails to
/// derive the cache key. The cache is untouched.
#[tokio::test]
async fn block_device_with_an_empty_mac_is_rejected_before_the_write() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, DEVICES_PATH, ok(), 1).await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
        .expect(0)
        .mount(&mock.server)
        .await;
    let client = client(&mock).await;
    client.get_devices(Some(NETWORK), false, None, None).await?;

    let err = client
        .block_device("", Some(NETWORK))
        .await
        .expect_err("an empty MAC is rejected");
    assert_empty_id(&err);

    // Still cached: the mock above is expected exactly once.
    client.get_devices(Some(NETWORK), false, None, None).await?;
    Ok(())
}

// ===================== Every device write invalidates across spellings =====================

#[derive(Debug, Clone, Copy)]
enum DeviceWrite {
    Nickname,
    Pause,
    PauseByPath,
    Block,
    Unblock,
    Type,
    Labels,
    UpdateViaLink,
    SecondaryWan,
}

impl DeviceWrite {
    const ALL: [Self; 9] = [
        Self::Nickname,
        Self::Pause,
        Self::PauseByPath,
        Self::Block,
        Self::Unblock,
        Self::Type,
        Self::Labels,
        Self::UpdateViaLink,
        Self::SecondaryWan,
    ];

    const MAC: &'static str = "AA:BB:CC:DD:EE:FF";

    fn endpoint(self) -> (&'static str, String) {
        let v22 = "/2.2/networks/network-0001";
        let v23 = "/2.3/networks/network-0001";
        let mac = Self::MAC;
        match self {
            Self::Nickname | Self::Pause | Self::PauseByPath | Self::SecondaryWan => {
                ("PUT", format!("{v23}/devices/{mac}"))
            }
            Self::Block => ("POST", format!("{v22}/blacklist")),
            Self::Unblock => ("DELETE", format!("{v22}/blacklist/{mac}")),
            Self::Type | Self::UpdateViaLink => ("PUT", format!("{v22}/devices/{mac}")),
            Self::Labels => ("PUT", format!("{v22}/devices/{mac}/labels")),
        }
    }

    async fn run(self, client: &Client) -> Result<(), Error> {
        let mac = Self::MAC;
        match self {
            Self::Nickname => {
                client
                    .set_device_nickname(mac, "Phone", Some(NETWORK))
                    .await?;
            }
            Self::Pause => {
                client.pause_device(mac, true, Some(NETWORK)).await?;
            }
            Self::PauseByPath => {
                client
                    .pause_device(
                        "/2.2/networks/network-0001/devices/AA:BB:CC:DD:EE:FF",
                        true,
                        Some(NETWORK),
                    )
                    .await?;
            }
            Self::Block => {
                client.block_device(mac, Some(NETWORK)).await?;
            }
            Self::Unblock => {
                client.unblock_device(mac, Some(NETWORK)).await?;
            }
            Self::Type => {
                client
                    .set_device_type(mac, "computer", Some(NETWORK))
                    .await?;
            }
            Self::Labels => {
                client
                    .set_device_labels(mac, Some("Acme"), None, None, None, Some(NETWORK))
                    .await?;
            }
            Self::UpdateViaLink => {
                client
                    .update_device_via_link(mac, Some("Phone"), None, None, Some(NETWORK))
                    .await?;
            }
            Self::SecondaryWan => {
                client
                    .set_device_secondary_wan_access(mac, true, Some(NETWORK))
                    .await?;
            }
        }
        Ok(())
    }
}

/// Issue #168's scenario: the device is read as `aabbccddeeff`, the write names it
/// `AA:BB:CC:DD:EE:FF`. The write drops the entry read under the other spelling and the list.
#[tokio::test]
async fn every_device_write_drops_the_entry_read_under_another_spelling() -> anyhow::Result<()> {
    for write in DeviceWrite::ALL {
        let mock = MockEero::start().await;
        mount_get(&mock, DEVICE_PATH, device_body(), 2).await;
        mount_get(&mock, DEVICES_PATH, ok(), 2).await;
        let (verb, target) = write.endpoint();
        Mock::given(method(verb))
            .and(path(target))
            .and(user_token_header())
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
            .expect(1)
            .mount(&mock.server)
            .await;
        let client = client(&mock).await;

        client
            .get_device("aabbccddeeff", Some(NETWORK), false)
            .await?;
        client.get_devices(Some(NETWORK), false, None, None).await?;

        write.run(&client).await?;

        // Both reads go back to the network (`.expect(2)` above).
        client
            .get_device("aabbccddeeff", Some(NETWORK), false)
            .await
            .unwrap_or_else(|err| panic!("{write:?}: {err:?}"));
        client
            .get_devices(Some(NETWORK), false, None, None)
            .await
            .unwrap_or_else(|err| panic!("{write:?}: {err:?}"));
    }
    Ok(())
}

/// A write for one device leaves another device's entry alone.
#[tokio::test]
async fn a_device_write_leaves_other_devices_cached() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_get(&mock, DEVICE_PATH, device_body(), 2).await;
    mount_get(
        &mock,
        "/2.2/networks/network-0001/devices/112233445566",
        device_body(),
        1,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/AA:BB:CC:DD:EE:FF"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let client = client(&mock).await;

    client
        .get_device("aabbccddeeff", Some(NETWORK), false)
        .await?;
    client
        .get_device("112233445566", Some(NETWORK), false)
        .await?;
    client
        .pause_device("AA:BB:CC:DD:EE:FF", true, Some(NETWORK))
        .await?;

    client
        .get_device("aabbccddeeff", Some(NETWORK), false)
        .await?;
    client
        .get_device("112233445566", Some(NETWORK), false)
        .await?;
    Ok(())
}

/// A device cached under one spelling is reused as `parent=` under another, so the update goes
/// to the device's own published url instead of the template.
#[tokio::test]
async fn update_device_via_link_reuses_a_device_cached_under_another_spelling() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    mount_get(
        &mock,
        DEVICE_PATH,
        json!({
            "meta": {"code": 200},
            "data": {
                "url": "/2.3/networks/network-0001/devices/aabbccddeeff",
                "mac": "aa:bb:cc:dd:ee:ff"
            },
        }),
        1,
    )
    .await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/devices/AA:BB:CC:DD:EE:FF"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/2.3/networks/network-0001/devices/aabbccddeeff"))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok()))
        .expect(1)
        .mount(&mock.server)
        .await;
    let client = client(&mock).await;

    client
        .get_device("aabbccddeeff", Some(NETWORK), false)
        .await?;
    client
        .update_device_via_link("AA:BB:CC:DD:EE:FF", Some("x"), None, None, Some(NETWORK))
        .await?;
    Ok(())
}

// ===================== set_profile_devices also drops the named devices =====================

const PROFILE_PATH: &str = "/2.2/networks/network-0001/profiles/profile-0001";
const PROFILES_PATH: &str = "/2.2/networks/network-0001/profiles";
const OTHER_DEVICE_PATH: &str = "/2.2/networks/network-0001/devices/other";
const D1_PATH: &str = "/2.2/networks/network-0001/devices/d1";

/// Primes the device entries `aabbccddeeff`, `d1` and `other`, the device list, a profile and the
/// profile list. `refetched` lists which of the six are expected to be fetched twice.
async fn prime_for_set_profile_devices(mock: &MockEero, client: &Client, refetched: &[&str]) {
    let times = |name: &str| if refetched.contains(&name) { 2 } else { 1 };
    mount_get(mock, DEVICE_PATH, device_body(), times("mac")).await;
    mount_get(mock, D1_PATH, device_body(), times("d1")).await;
    mount_get(mock, OTHER_DEVICE_PATH, device_body(), times("other")).await;
    mount_get(mock, DEVICES_PATH, ok(), times("list")).await;
    mount_get(mock, PROFILE_PATH, ok(), times("profile")).await;
    mount_get(mock, PROFILES_PATH, ok(), times("profiles")).await;
    read_for_set_profile_devices(client).await;
}

async fn read_for_set_profile_devices(client: &Client) {
    let network = Some(NETWORK);
    client
        .get_device("aabbccddeeff", network, false)
        .await
        .expect("mac device");
    client.get_device("d1", network, false).await.expect("d1");
    client
        .get_device("other", network, false)
        .await
        .expect("other");
    client
        .get_devices(network, false, None, None)
        .await
        .expect("device list");
    client
        .get_profile("profile-0001", network, false)
        .await
        .expect("profile");
    client.get_profiles(network, false).await.expect("profiles");
}

async fn mount_profile_put(mock: &MockEero, status: u16) {
    Mock::given(method("PUT"))
        .and(path(PROFILE_PATH))
        .and(user_token_header())
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(status).set_body_json(ok()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn set_profile_devices_drops_the_named_devices_the_list_and_both_profile_keys()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    prime_for_set_profile_devices(
        &mock,
        &client,
        &["mac", "d1", "list", "profile", "profiles"],
    )
    .await;
    mount_profile_put(&mock, 200).await;

    client
        .set_profile_devices(
            "profile-0001",
            &[
                "/2.2/networks/network-0001/devices/AA:BB:CC:DD:EE:FF",
                "/2.2/networks/network-0001/devices/d1",
            ],
            Some(NETWORK),
        )
        .await?;

    // Everything but `other` is fetched again; `other` stays cached.
    read_for_set_profile_devices(&client).await;
    Ok(())
}

#[tokio::test]
async fn set_profile_devices_with_no_devices_still_drops_the_device_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    prime_for_set_profile_devices(&mock, &client, &["list", "profile", "profiles"]).await;
    mount_profile_put(&mock, 200).await;

    client
        .set_profile_devices("profile-0001", &[], Some(NETWORK))
        .await?;

    read_for_set_profile_devices(&client).await;
    Ok(())
}

/// An unusable entry never fails a write the server has already accepted.
#[tokio::test]
async fn set_profile_devices_skips_empty_entries() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    prime_for_set_profile_devices(&mock, &client, &["d1", "list", "profile", "profiles"]).await;
    mount_profile_put(&mock, 200).await;

    client
        .set_profile_devices(
            "profile-0001",
            &["", "/2.2/networks/network-0001/devices/d1"],
            Some(NETWORK),
        )
        .await?;

    read_for_set_profile_devices(&client).await;
    Ok(())
}

#[tokio::test]
async fn a_failed_set_profile_devices_invalidates_nothing() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    prime_for_set_profile_devices(&mock, &client, &[]).await;
    mount_profile_put(&mock, 500).await;

    client
        .set_profile_devices(
            "profile-0001",
            &["/2.2/networks/network-0001/devices/d1"],
            Some(NETWORK),
        )
        .await
        .expect_err("the write fails");

    // Every read is still a cache hit: each mock is expected exactly once.
    read_for_set_profile_devices(&client).await;
    Ok(())
}
