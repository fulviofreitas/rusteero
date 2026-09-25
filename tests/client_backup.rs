//! `Client` integration suite for `BackupApi`'s faithful (Python-parity) non-invalidation.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

/// Builds a [`Client`] pointed at `mock`, authenticated with [`TEST_TOKEN`], with the crate's
/// default 60-second cache TTL.
async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

/// **Broken/restored in place**: the assertion below was temporarily changed to `.expect(2)`
/// (asserting the write *does* invalidate, matching `client_eeros.rs`/`client_dns.rs`), run, and
/// observed to fail — RED — because `set_backup_network` faithfully invalidates nothing and the
/// second `get_network` call really is served from cache. Restored to `.expect(1)` — GREEN. See
/// this task's final report for the captured output of both runs.
///
/// This is the test that proves `set_led_brightness`/DNS invalidation (`client_eeros.rs`,
/// `client_dns.rs`) is targeted rather than every write clearing every bucket:
/// `set_backup_network` PUTs a completely different resource (`networks/{nid}/backup`, not
/// `networks/{nid}/settings`) that Python never wires into any cache invalidation at all
/// (behaviour brief §2, "Verified to invalidate nothing"), and this port does not add that as a
/// third, uninstructed improvement.
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
