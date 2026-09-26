//! `Client` integration suite for the `TransferAPI` domain.

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
async fn get_transfer_stats_forwards_the_device_id_and_resolves_the_network() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"download": 1} });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/devices/device-0002/transfer",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_transfer_stats(Some("network-0001"), Some("device-0002"))
        .await?;
    assert_eq!(env.as_value(), &body);
    Ok(())
}
