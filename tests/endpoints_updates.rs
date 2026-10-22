//! HTTP integration tests for `UpdatesApi` (`src/endpoints/updates.rs`).
//!
//! `UpdatesApi` has no fixture under `tests/fixtures/`, so every response body here is a small,
//! obviously-synthetic `{"meta": …, "data": …}` value built inline with `serde_json::json!`,
//! matching the shape exercised by the corresponding `eero-api` unit test
//! (`tests/api/test_updates.py`). No real MACs, serials, IPs or names appear in any fixture here.

mod common;

use std::sync::Arc;

use rusteero::endpoints::updates::UpdatesApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, capture_logs, session_cookie, user_token_header};

fn updates_api(mock: &MockEero) -> UpdatesApi {
    UpdatesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_updates =====================

#[tokio::test]
async fn get_updates_bare_id_falls_back_to_the_template() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({
        "meta": {"code": 200},
        "data": {"available": true, "current_version": "6.15.0"},
    });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = updates_api(&mock).get_updates("network-0001", None).await?;

    assert_eq!(env.into_value(), body);
    Ok(())
}

#[tokio::test]
async fn get_updates_prefers_the_parents_updates_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let body = json!({ "meta": {"code": 200}, "data": {"available": false} });
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/updates"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/custom-updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"updates": "/2.2/networks/network-0001/custom-updates"}});
    let env = updates_api(&mock)
        .get_updates("network-0001", Some(&parent))
        .await?;
    assert_eq!(env.into_value(), body);
    Ok(())
}

// ===================== apply_update =====================

#[tokio::test]
async fn apply_update_posts_the_empty_json_string_to_the_updates_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/updates"))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_string("\"\""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "meta": { "code": 200 }, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let env = updates_api(&mock)
        .apply_update("network-0001", None)
        .await?;
    assert_eq!(
        env.into_value(),
        json!({ "meta": { "code": 200 }, "data": {} })
    );
    Ok(())
}

#[tokio::test]
async fn apply_update_prefers_the_parents_updates_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/updates"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/custom-updates"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "meta": { "code": 200 }, "data": {} })),
        )
        .expect(1)
        .mount(&mock.server)
        .await;

    let parent = json!({"resources": {"updates": "/2.2/networks/network-0001/custom-updates"}});
    let env = updates_api(&mock)
        .apply_update("network-0001", Some(&parent))
        .await?;
    assert_eq!(
        env.into_value(),
        json!({ "meta": { "code": 200 }, "data": {} })
    );
    Ok(())
}

// ===================== set_preferred_update_hour =====================

const HOUR_PATH: &str = "/2.2/networks/network-0001/updates/preferred_update_hour";

fn ok_envelope() -> serde_json::Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

/// Mounts `POST HOUR_PATH` expecting exactly one JSON `{"preferred_update_hour": hour}` request
/// with both credentials.
async fn mount_hour_post(mock: &MockEero, hour: i32) {
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .and(session_cookie())
        .and(user_token_header())
        .and(header("content-type", "application/json"))
        .and(body_json(json!({ "preferred_update_hour": hour })))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// Mounts a `POST HOUR_PATH` mock that must never be reached.
async fn mount_hour_post_never(mock: &MockEero) {
    Mock::given(method("POST"))
        .and(path(HOUR_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(0)
        .mount(&mock.server)
        .await;
}

async fn assert_no_requests(mock: &MockEero) {
    let requests = mock
        .server
        .received_requests()
        .await
        .expect("request recording is enabled by default");
    assert!(requests.is_empty(), "expected zero requests: {requests:?}");
}

#[tokio::test]
async fn set_preferred_update_hour_posts_the_hour_as_json() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post(&mock, 3).await;

    let env = updates_api(&mock)
        .set_preferred_update_hour("network-0001", 3)
        .await?;
    assert_eq!(env.into_value(), ok_envelope());
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_sends_both_boundary_hours() -> anyhow::Result<()> {
    for hour in [0, 23] {
        let mock = MockEero::start().await;
        mount_hour_post(&mock, hour).await;

        updates_api(&mock)
            .set_preferred_update_hour("network-0001", hour)
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_accepts_an_api_network_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    // A `/2.2/networks/{id}` API path resolves to the same single, undoubled path. Hour 0 is
    // valid and is sent as the integer 0, not dropped as falsy.
    mount_hour_post(&mock, 0).await;

    updates_api(&mock)
        .set_preferred_update_hour("/2.2/networks/network-0001", 0)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_accepts_an_absolute_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post(&mock, 23).await;
    let url = format!("{}/2.2/networks/network-0001", mock.uri());

    updates_api(&mock)
        .set_preferred_update_hour(&url, 23)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_rejects_out_of_range_hours_without_a_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post_never(&mock).await;
    let api = updates_api(&mock);

    for hour in [-1, 24, i32::MIN, i32::MAX] {
        let err = api
            .set_preferred_update_hour("network-0001", hour)
            .await
            .expect_err("an out-of-range hour must be rejected before any request");
        assert!(
            matches!(
                err,
                Error::Validation { ref field, ref message, .. }
                    if field == "hour" && message == "must be an int from 0 to 23"
            ),
            "hour {hour}: got {err:?}"
        );
        assert_eq!(
            err.to_string(),
            "Validation error for 'hour': must be an int from 0 to 23"
        );
    }
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_validates_the_hour_before_anything_else() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post_never(&mock).await;
    let (_guard, logs) = capture_logs();

    // An invalid network id with an invalid hour: the hour error wins.
    let err = updates_api(&mock)
        .set_preferred_update_hour("../x", 24)
        .await
        .expect_err("an invalid hour wins over an invalid network id");
    assert!(
        matches!(err, Error::Validation { ref field, .. } if field == "hour"),
        "got {err:?}"
    );

    // No session at all: the hour error wins over the credential gate.
    let anonymous = UpdatesApi::new(Arc::new(mock.transport_anonymous()));
    let err = anonymous
        .set_preferred_update_hour("network-0001", 24)
        .await
        .expect_err("an invalid hour wins over a missing session");
    assert!(
        matches!(err, Error::Validation { ref field, .. } if field == "hour"),
        "got {err:?}"
    );
    assert!(!err.is_auth_error());

    assert!(
        logs.warnings().is_empty(),
        "an invalid call must log no uncharacterised-write warning: {:?}",
        logs.warnings()
    );
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_warns_once_after_validation() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post(&mock, 5).await;
    let (_guard, logs) = capture_logs();

    updates_api(&mock)
        .set_preferred_update_hour("network-0001", 5)
        .await?;

    assert_eq!(logs.warnings().len(), 1, "got {:?}", logs.warnings());
    assert_eq!(
        logs.count(
            tracing::Level::WARN,
            "Issuing write (set preferred update hour for network)"
        ),
        1
    );
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_with_an_unresolvable_id_sends_nothing_and_does_not_warn()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let (_guard, logs) = capture_logs();
    let api = updates_api(&mock);

    for bad_id in ["", "../x"] {
        let err = api
            .set_preferred_update_hour(bad_id, 3)
            .await
            .expect_err("an invalid network id must be rejected before any request");
        assert!(
            matches!(err, Error::Validation { .. }),
            "{bad_id:?}: got {err:?}"
        );
    }

    assert!(
        logs.warnings().is_empty(),
        "the warning follows URL resolution: {:?}",
        logs.warnings()
    );
    assert_no_requests(&mock).await;
    Ok(())
}

#[tokio::test]
async fn set_preferred_update_hour_not_authenticated_returns_authentication_error_without_a_request()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_hour_post_never(&mock).await;
    let api = UpdatesApi::new(Arc::new(mock.transport_anonymous()));

    let err = api
        .set_preferred_update_hour("network-0001", 3)
        .await
        .expect_err("an unauthenticated write must fail");
    assert!(err.is_auth_error(), "got {err:?}");
    assert_no_requests(&mock).await;
    Ok(())
}
