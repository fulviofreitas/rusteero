//! `Client` integration suite for the `ouicheck` domain (`eero-api src/eero/client.py:1807-1825`).

mod common;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::error::Error;

async fn client(mock: &MockEero) -> Client {
    Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await
        .expect("a MockServer's own URI is always a valid base URL")
}

#[tokio::test]
async fn get_ouicheck_resolves_the_network_and_forwards_serial_and_version() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/ouicheck"))
        .and(query_param("serial", "ABC123"))
        .and(query_param("version", "1"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{\"meta\":{},\"data\":{\"vendor\":\"ExampleVendor\"}}"),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .get_ouicheck("ABC123", "1", Some("network-0001"))
        .await?;
    assert_eq!(env.data()["vendor"].as_str(), Some("ExampleVendor"));
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_requires_a_network_id_without_auto_discovery() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_ouicheck("ABC123", "1", None)
        .await
        .expect_err("no network_id and no preferred network must fail without auto-discovery");
    assert!(matches!(err, Error::MissingNetworkId));
    Ok(())
}

#[tokio::test]
async fn get_ouicheck_rejects_an_empty_serial() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_ouicheck("", "1", Some("network-0001"))
        .await
        .expect_err("an empty serial must be rejected");
    assert!(matches!(err, Error::Validation { field, .. } if field == "serial"));
    Ok(())
}
