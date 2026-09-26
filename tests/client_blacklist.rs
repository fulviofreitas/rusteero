//! `Client` integration suite for `BlacklistApi`'s cache invalidation at `v8.0.4`.
//!
//! `Client::add_to_blacklist`/`Client::remove_from_blacklist` were removed at v8.0.4 (no
//! `client.py` precedent — see `src/client/blacklist.rs`'s module docs); their coverage moved to
//! `tests/client_devices.rs`'s `block_device_invalidates_the_devices_bucket`/
//! `unblock_device_invalidates_the_devices_bucket`, which exercise the same underlying
//! `BlacklistApi::add_to_blacklist`/`remove_from_blacklist` calls through
//! `Client::block_device`/`Client::unblock_device`.

mod common;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
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

#[tokio::test]
async fn get_blacklist_hits_the_blacklist_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/blacklist"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_blacklist(Some("network-0001")).await?;
    Ok(())
}
