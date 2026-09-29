//! `Client` integration suite for `ReservationsApi`'s network-id resolution, parent forwarding,
//! and faithful (Python-parity) non-invalidation.

mod common;

use serde_json::json;
use wiremock::matchers::{method, path, query_param};
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

#[tokio::test]
async fn get_reservations_prefers_the_cached_networks_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let network_with_link = json!({
        "meta": { "code": 200 },
        "data": {
            "id": "network-0001",
            "url": "/2.2/networks/network-0001",
            "resources": { "reservations": "/2.3/networks/network-0001/reservations" },
        },
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(network_with_link.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/reservations"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": []}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_reservations(Some("network-0001")).await?;
    Ok(())
}

/// A faithful no-op, for a domain with no cache bucket of its own at all: a DHCP reservation
/// write cannot invalidate anything, because `reservations` was never one of the six buckets
/// `Cache` knows about (behaviour brief §2.1) — this asserts it does not, in passing, touch the
/// unrelated `network[nid]` entry either.
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

#[tokio::test]
async fn update_reservation_resolves_network_id_and_reaches_the_template_path() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .update_reservation(
            "res-0001",
            json!({"public_static_ip": true}),
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn delete_reservation_forwards_delete_forwards_as_a_query_param() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/reservations/res-0001"))
        .and(session_cookie())
        .and(query_param("delete_forwards", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .delete_reservation("res-0001", Some(true), Some("network-0001"))
        .await?;
    Ok(())
}
