//! `Client` integration suite for the `PermissionsAPI` domain (new in v8.0.0).

mod common;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_permissions_passes_the_cached_network_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network = json!({
        "meta": {"code": 200},
        "data": {"id": "network-0001", "url": "/2.2/networks/network-0001"},
    });
    let permissions = json!({
        "meta": {"code": 200},
        "data": {"permissions": {"network.admin_invites": true}, "role": "OWNER"},
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/permissions"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(permissions.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    let env = client.get_permissions(Some("network-0001")).await?;
    assert_eq!(env.as_value(), &permissions);
    Ok(())
}
