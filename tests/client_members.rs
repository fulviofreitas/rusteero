//! `Client` integration suite for `MembersApi` (new in v8.0.0): network-id resolution for every
//! wrapper, and `query_invite`'s no-network-id shape.

mod common;

use serde_json::json;
use wiremock::matchers::{body_json, method, path};
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
async fn get_members_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "members": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/members"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_members(Some("network-0001")).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

/// `get_members` passes the cached network as `parent=` (`+net`, `client.py:2577-2579`): a
/// network that publishes its own `members` link must be preferred over the bare-id template.
#[tokio::test]
async fn get_members_prefers_the_cached_networks_published_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001"))
        .and(session_cookie())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                json!({
                    "meta": { "code": 200 },
                    "data": {
                        "url": "/2.2/networks/network-0001",
                        "resources": { "members": "/2.4/networks/network-0001/members" },
                    },
                })
                .to_string(),
            ),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/members"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.4/networks/network-0001/members"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client.get_network(Some("network-0001"), false).await?;
    client.get_members(Some("network-0001")).await?;
    Ok(())
}

#[tokio::test]
async fn get_members_without_an_explicit_network_id_requires_a_preferred_network()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let client = client(&mock).await;
    let err = client
        .get_members(None)
        .await
        .expect_err("no network id and no preferred network must fail");
    assert!(matches!(err, rusteero::error::Error::MissingNetworkId));
    Ok(())
}

#[tokio::test]
async fn create_invite_forwards_the_role_and_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites"))
        .and(session_cookie())
        .and(body_json(json!({ "invite_role": "admin" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.create_invite("admin", Some("network-0001")).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn get_invites_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "invites": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/invites"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.get_invites(Some("network-0001")).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn update_invite_sends_the_nickname_and_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/invites/invite-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "invite_nickname": "Guest" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .update_invite("invite-0001", "Guest", Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn delete_invite_resolves_the_network_id_and_sends_the_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/invites/invite-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .delete_invite("invite-0001", Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn respond_to_invite_sends_accept_and_the_invite_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites/response"))
        .and(session_cookie())
        .and(body_json(
            json!({ "accept": true, "invite_id": "invite-0001" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .respond_to_invite(true, Some("invite-0001"), None, Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn cancel_pending_admin_resolves_the_network_id_and_sends_the_empty_json_string()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/invites/cancel_pending_admin",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.cancel_pending_admin(Some("network-0001")).await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn promote_member_sends_the_member_id_and_resolves_the_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/member_promotion"))
        .and(session_cookie())
        .and(body_json(json!({ "member_id": "member-0001" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .promote_member("member-0001", Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn remove_admin_resolves_the_network_id_and_sends_the_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/admins/user-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client
        .remove_admin("user-0001", Some("network-0001"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn query_invite_never_resolves_a_network_id() -> anyhow::Result<()> {
    // No network exists on this client at all (no `set_preferred_network`, and `query_invite`
    // never calls `ensure_network_id`); if it accidentally did, this test would fail with
    // `Error::MissingNetworkId` instead of exercising the wire call below.
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": { "invite": {} } });
    Mock::given(method("POST"))
        .and(path("/2.2/inviteQuery"))
        .and(session_cookie())
        .and(body_json(json!({ "invite_code": "code-abc" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let env = client.query_invite("code-abc").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}
