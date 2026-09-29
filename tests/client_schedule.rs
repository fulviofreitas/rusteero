//! `Client` integration suite for `ScheduleApi` at v8.0.4 — scheduled pauses as sub-resources of
//! a profile, no cache invalidation anywhere in this domain (see `src/client/schedule.rs`'s
//! module docs for why the pre-v8 F3 divergence was retired).

mod common;

use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::schedule::UpdateScheduleOptions;
use serde_json::json;

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
async fn get_schedules_reaches_the_schedules_collection_endpoint() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedules.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let response = client
        .get_schedules("profile-0001", Some("network-0001"))
        .await?;
    assert_eq!(response.as_value(), &fixture_json("schedules.json"));
    Ok(())
}

#[tokio::test]
async fn create_schedule_posts_to_the_schedules_collection() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(body_json(json!({
            "name": "Study Time",
            "days": ["wednesday"],
            "start": "15:00",
            "end": "16:00",
            "enabled": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .create_schedule(
            "profile-0001",
            "Study Time",
            &["wednesday"],
            "15:00",
            "16:00",
            true,
            Some("network-0001"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn update_schedule_takes_no_network_id_and_resolves_from_its_own_path() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .update_schedule(
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

/// Ported from `client.py:2015-2044`'s `schedule: Any` — `Client::update_schedule` accepts the
/// pause's own cached envelope via `parent`, not just a path/URL string (phase-G fix list item
/// 19).
#[tokio::test]
async fn update_schedule_accepts_a_cached_pause_envelope_as_parent() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let parent = json!({
        "url": "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001"
    });
    client
        .update_schedule(
            "ignored",
            &UpdateScheduleOptions {
                enabled: Some(false),
                ..UpdateScheduleOptions::default()
            },
            Some(&parent),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn delete_schedule_takes_no_network_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .delete_schedule(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
            None,
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn clear_profile_schedule_returns_one_response_per_deleted_pause() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedules.json")))
        .expect(1)
        .mount(&mock.server)
        .await;
    Mock::given(method("DELETE"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(2)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    let results = client
        .clear_profile_schedule("profile-0001", Some("network-0001"))
        .await?;
    assert_eq!(results.len(), 2);
    Ok(())
}

#[tokio::test]
async fn enable_bedtime_reaches_the_schedules_collection() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let client = client(&mock).await;
    client
        .enable_bedtime("profile-0001", "21:00", "07:00", None, Some("network-0001"))
        .await?;
    Ok(())
}
