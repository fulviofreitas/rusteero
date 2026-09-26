//! HTTP integration tests for `MembersApi` (`src/endpoints/members.rs`, new in v8.0.0) against a
//! local `wiremock` server per the crate's testing conventions.
//!
//! No committed fixture file exists for this domain, so every response body here is a small,
//! obviously-synthetic value built inline with `serde_json::json!`, in the shape `eero-api`'s
//! own tests (`tests/api/test_members.py`) use — no real MACs, serials, IPs or names.

mod common;

use std::sync::Arc;

use rusteero::endpoints::members::MembersApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

/// Builds a [`MembersApi`] pointed at `mock`, wrapping a `Transport` already authenticated with
/// [`TEST_TOKEN`].
fn api(mock: &MockEero) -> MembersApi {
    MembersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_members =====================

#[tokio::test]
async fn get_members_hits_the_members_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body =
        json!({ "meta": { "code": 200 }, "data": { "members": [{"user_name": "example"}] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/members"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).get_members("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_members_prefers_a_parent_supplied_link_over_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "members": [] } });
    Mock::given(method("GET"))
        .and(path("/2.3/networks/network-0001/members"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"members": "/2.3/networks/network-0001/members"}});
    let env = api(&mock)
        .get_members("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_members_falls_back_to_the_bare_id_template_with_no_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "members": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/members"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).get_members("network-0001", None).await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== get_invites =====================

#[tokio::test]
async fn get_invites_hits_the_invites_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 200 }, "data": { "invites": [] } });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/invites"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).get_invites("network-0001").await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== create_invite =====================

#[tokio::test]
async fn create_invite_sends_invite_role_owner() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites"))
        .and(session_cookie())
        .and(body_json(json!({ "invite_role": "owner" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock).create_invite("network-0001", "owner").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_invite_normalises_mixed_case_role_before_sending() -> anyhow::Result<()> {
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

    let env = api(&mock).create_invite("network-0001", "Admin").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn create_invite_rejects_an_unknown_role_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // Registered so a stray request gets an answer instead of hanging; `.expect(0)` fails the
    // test the moment it is ever hit.
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = api(&mock)
        .create_invite("network-0001", "superuser")
        .await
        .expect_err("an unrecognised role must be rejected locally");

    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "role");
    Ok(())
}

// ===================== update_invite =====================

#[tokio::test]
async fn update_invite_sends_invite_nickname() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/invites/invite-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "invite_nickname": "Housemate" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .update_invite("network-0001", "invite-0001", "Housemate")
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== delete_invite =====================

#[tokio::test]
async fn delete_invite_sends_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/invites/invite-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock)
        .delete_invite("network-0001", "invite-0001")
        .await?;
    Ok(())
}

// ===================== respond_to_invite =====================

#[tokio::test]
async fn respond_to_invite_sends_the_invite_id_variant() -> anyhow::Result<()> {
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

    let env = api(&mock)
        .respond_to_invite("network-0001", true, Some("invite-0001"), None)
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn respond_to_invite_sends_the_invite_code_variant() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({ "meta": { "code": 200 }, "data": {} });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites/response"))
        .and(session_cookie())
        .and(body_json(
            json!({ "accept": false, "invite_code": "code-abc" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = api(&mock)
        .respond_to_invite("network-0001", false, None, Some("code-abc"))
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

#[tokio::test]
async fn respond_to_invite_rejects_neither_identifier_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites/response"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = api(&mock)
        .respond_to_invite("network-0001", true, None, None)
        .await
        .expect_err("neither invite_id nor invite_code must be rejected");
    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "invite_id");
    Ok(())
}

#[tokio::test]
async fn respond_to_invite_rejects_both_identifiers_with_zero_requests() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites/response"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;

    let err = api(&mock)
        .respond_to_invite("network-0001", true, Some("invite-0001"), Some("code-abc"))
        .await
        .expect_err("both invite_id and invite_code together must be rejected");
    // Python always reports `invite_id` as the field, even when the actual problem is that
    // `invite_code` was also supplied — reproduced verbatim.
    let Error::Validation { field, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "invite_id");
    Ok(())
}

// ===================== cancel_pending_admin =====================

#[tokio::test]
async fn cancel_pending_admin_sends_the_literal_empty_json_string_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/invites/cancel_pending_admin",
        ))
        .and(session_cookie())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock).cancel_pending_admin("network-0001").await?;
    Ok(())
}

// ===================== promote_member =====================

#[tokio::test]
async fn promote_member_sends_member_id() -> anyhow::Result<()> {
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

    let env = api(&mock)
        .promote_member("network-0001", "member-0001")
        .await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== remove_admin =====================

#[tokio::test]
async fn remove_admin_sends_delete() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/admins/user-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(json!({"data": {}}).to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    api(&mock).remove_admin("network-0001", "user-0001").await?;
    Ok(())
}

// ===================== query_invite =====================

#[tokio::test]
async fn query_invite_posts_to_the_top_level_invite_query_path() -> anyhow::Result<()> {
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

    let env = api(&mock).query_invite("code-abc").await?;
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== domain error surfaces as Error::Api =====================

#[tokio::test]
async fn expired_invite_status_surfaces_as_api_error_with_its_error_code() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": { "code": 409, "error": "error.invite.status.expired" } });
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/invites/response"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(409).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let err = api(&mock)
        .respond_to_invite("network-0001", true, Some("invite-0001"), None)
        .await
        .expect_err("an expired invite must surface as an error");

    let Error::Api {
        status, error_code, ..
    } = &err
    else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 409);
    assert_eq!(error_code.as_deref(), Some("error.invite.status.expired"));
    assert!(!err.is_auth_error());
    Ok(())
}
