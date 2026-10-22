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

use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::envelope::Envelope;
use rusteero::error::Error;

const NETWORK: &str = "network-0001";

/// How long the held-open read waits before its response is released.
const HOLD: Duration = Duration::from_millis(300);

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

    /// Mounts one answer carrying `marker` for this read, optionally held open for `HOLD`.
    /// Mounts are consumed in order: each one answers a single request, except `unbounded`.
    async fn mount(self, mock: &MockEero, marker: &str, hold: bool, unbounded: bool) {
        let mut response = ResponseTemplate::new(200).set_body_json(self.body(marker));
        if hold {
            response = response.set_delay(HOLD);
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
            read.mount(&mock, "old", false, false).await;
        }
        read.mount(&mock, "stale", true, false).await;
        read.mount(&mock, "fresh", false, true).await;
        write.mount(&mock).await;

        if cached_before {
            let primed = read.run(&client, false).await.expect("priming read");
            assert_eq!(primed.as_value(), &read.body("old"), "{label}");
        }

        // Without a cached entry a plain read goes to the network; with one it must be forced.
        let in_flight = read.run(&client, cached_before);
        let invalidate = async {
            wait_for_gets(&mock, &read.path(), if cached_before { 2 } else { 1 }).await;
            write.run(&client).await
        };
        let (stale, written) = tokio::join!(in_flight, invalidate);

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

/// `get_network` and `get_eeros` store through the same guard once `Client::get_network` and
/// `Client::get_eeros` take the generation before their request. Ignored until those two
/// wrappers (in `src/client/networks.rs` and `src/client/eeros.rs`) make that change.
#[tokio::test]
#[ignore = "needs put_if_current in Client::get_network (src/client/networks.rs)"]
async fn get_network_overtaken_by_set_network_name_is_not_cached() {
    assert_inflight_read_is_not_cached(Read::Network, Write::SetNetworkName).await;
}

#[tokio::test]
#[ignore = "needs put_if_current in Client::get_eeros (src/client/eeros.rs)"]
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
            Read::Devices.mount(&mock, "old", false, false).await;
        }
        Read::Devices.mount(&mock, "stale", true, false).await;
        Read::Devices.mount(&mock, "fresh", false, true).await;

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
