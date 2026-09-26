//! HTTP integration tests for `ScheduleApi` (v8.0.4) — scheduled pauses as sub-resources of a
//! profile, per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::schedule::{ScheduleApi, UpdateScheduleOptions};
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie, user_token_header};

fn schedule_api(mock: &MockEero) -> ScheduleApi {
    ScheduleApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_schedules =====================

#[tokio::test]
async fn get_schedules_builds_the_profile_schedules_collection_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedules.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let env = api
        .get_schedules("network-0001", "profile-0001", None)
        .await?;
    assert_eq!(env.into_value(), fixture_json("schedules.json"));
    Ok(())
}

#[tokio::test]
async fn get_schedules_prefers_the_parents_published_schedules_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.5/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedules.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let parent = json!({
        "resources": {
            "schedules": "/2.5/networks/network-0001/profiles/profile-0001/schedules"
        }
    });
    api.get_schedules("network-0001", "profile-0001", Some(&parent))
        .await?;
    Ok(())
}

// ===================== create_schedule =====================

#[tokio::test]
async fn create_schedule_posts_all_five_fields_to_the_schedules_collection() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .and(body_json(json!({
            "name": "Study Time",
            "days": ["monday", "tuesday"],
            "start": "15:00",
            "end": "16:00",
            "enabled": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.create_schedule(
        "network-0001",
        "profile-0001",
        "Study Time",
        &["monday", "tuesday"],
        "15:00",
        "16:00",
        true,
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn create_schedule_prefers_the_parents_published_schedules_link() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/2.5/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let parent = json!({
        "resources": {
            "schedules": "/2.5/networks/network-0001/profiles/profile-0001/schedules"
        }
    });
    api.create_schedule(
        "network-0001",
        "profile-0001",
        "Study Time",
        &["monday", "tuesday"],
        "15:00",
        "16:00",
        true,
        Some(&parent),
    )
    .await?;
    Ok(())
}

// ===================== update_schedule =====================

#[tokio::test]
async fn update_schedule_from_a_bare_path_sends_only_the_supplied_fields() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.update_schedule(
        "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        &UpdateScheduleOptions {
            enabled: Some(false),
            ..UpdateScheduleOptions::default()
        },
        None,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_schedule_from_an_envelope_uses_its_own_self_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "start": "20:00" })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let parent = json!({
        "url": "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001"
    });
    // The bare `"ignored"` id below is never used: `parent` takes over entirely once its own
    // `url` resolves.
    api.update_schedule(
        "ignored",
        &UpdateScheduleOptions {
            start: Some("20:00"),
            ..UpdateScheduleOptions::default()
        },
        Some(&parent),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_schedule_envelope_without_url_is_a_validation_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let parent = json!({"name": "no url field here"});
    let err = api
        .update_schedule(
            "ignored",
            &UpdateScheduleOptions {
                name: Some("New name"),
                ..UpdateScheduleOptions::default()
            },
            Some(&parent),
        )
        .await
        .expect_err("an envelope with no resolvable url must be a validation error");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "schedule"));
    Ok(())
}

#[tokio::test]
async fn update_schedule_with_no_fields_supplied_fails_before_any_request() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let err = api
        .update_schedule(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
            &UpdateScheduleOptions::default(),
            None,
        )
        .await
        .expect_err("no fields supplied must be a validation error, and must not hit the mock");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "schedule"));
    Ok(())
}

// ===================== delete_schedule =====================

#[tokio::test]
async fn delete_schedule_deletes_its_own_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.delete_schedule(
        "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        None,
    )
    .await?;
    Ok(())
}

// ===================== clear_profile_schedule =====================

#[tokio::test]
async fn clear_profile_schedule_issues_one_delete_per_pause() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedules.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0002",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let results = api
        .clear_profile_schedule("network-0001", "profile-0001", None)
        .await?;
    assert_eq!(results.len(), 2);
    Ok(())
}

#[tokio::test]
async fn clear_profile_schedule_with_no_pauses_issues_no_deletes() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200},"data":[]}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let results = api
        .clear_profile_schedule("network-0001", "profile-0001", None)
        .await?;
    assert!(results.is_empty());
    Ok(())
}

/// Ported from `_resolve_schedule_url`'s `Mapping` branch (`schedule.py:60-63`), reached via
/// `delete_schedule(pause)` (`schedule.py:313`): a pause with no `url` field (or an empty one)
/// must raise the identical `Error::Validation { field: "schedule", message: "envelope has no
/// resolvable 'url' field" }` every other unresolvable-schedule case in this module raises — not
/// a `clear_profile_schedule`-specific message.
#[tokio::test]
async fn clear_profile_schedule_with_an_unresolvable_pause_uses_the_shared_message()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"meta":{"code":200},"data":[{"name":"Bedtime","url":""}]}"#),
        )
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let err = api
        .clear_profile_schedule("network-0001", "profile-0001", None)
        .await
        .expect_err("an empty `url` field must be treated as missing, not as an empty path");
    let Error::Validation { field, message, .. } = &err else {
        panic!("expected Error::Validation, got {err:?}");
    };
    assert_eq!(field, "schedule");
    assert_eq!(message, "envelope has no resolvable 'url' field");
    Ok(())
}

// ===================== enable_bedtime / weekday / weekend =====================

#[tokio::test]
async fn enable_bedtime_with_no_days_defaults_to_all_seven() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Bedtime",
            "days": [
                "monday", "tuesday", "wednesday", "thursday",
                "friday", "saturday", "sunday"
            ],
            "start": "21:00",
            "end": "07:00",
            "enabled": true
        })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.enable_bedtime("network-0001", "profile-0001", "21:00", "07:00", None, None)
        .await?;
    Ok(())
}

/// Proves the weekday delegator scopes its bedtime block to Monday through Friday only.
#[tokio::test]
async fn set_weekday_bedtime_emits_a_bedtime_block_scoped_to_monday_through_friday()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Bedtime",
            "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
            "start": "20:30",
            "end": "06:30",
            "enabled": true
        })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.set_weekday_bedtime("network-0001", "profile-0001", "20:30", "06:30", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn set_weekend_bedtime_emits_a_bedtime_block_scoped_to_saturday_and_sunday()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Bedtime",
            "days": ["saturday", "sunday"],
            "start": "23:00",
            "end": "09:00",
            "enabled": true
        })))
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.set_weekend_bedtime("network-0001", "profile-0001", "23:00", "09:00", None)
        .await?;
    Ok(())
}
