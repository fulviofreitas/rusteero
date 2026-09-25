//! HTTP integration test for `SettingsApi` (`src/endpoints/settings.rs`) against a local
//! `wiremock` server per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::settings::SettingsApi;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};

/// Builds a [`SettingsApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`] (see [`MockEero::transport_with_token`]).
fn settings_api(mock: &MockEero) -> SettingsApi {
    SettingsApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_settings =====================

#[tokio::test]
async fn get_settings_hits_the_dedicated_settings_subresource_not_the_full_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A single mock registered against the dedicated `networks/{id}/settings` path (not the bare
    // `networks/{id}` path the other three methods in this domain group hit): if `get_settings`
    // ever regressed to fetching the full network object instead, this mock would never match
    // and the call below would fail with a wiremock "no match" error rather than a response.
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/settings"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("network.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = settings_api(&mock);
    let env = api.get_settings("network-0001").await?;
    assert_eq!(env.into_value(), fixture_json("network.json"));
    Ok(())
}
