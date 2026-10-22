//! `Client` integration suite for `ScheduleApi` at v8.0.4 — scheduled pauses as sub-resources of
//! a profile, no cache invalidation anywhere in this domain (see `src/client/schedule.rs`'s
//! module docs for why the pre-v8 profile-cache invalidation was retired).

mod common;

use tracing::Level;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, capture_logs, fixture, fixture_json, session_cookie};
use rusteero::auth::Session;
use rusteero::client::Client;
use rusteero::endpoints::schedule::UpdateScheduleOptions;
use rusteero::error::Error;
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
/// pause's own cached envelope via `parent`, not just a path/URL string.
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

// ===================== set_weekday_bedtime / set_weekend_bedtime =====================

const WEEKDAYS: [&str; 5] = ["monday", "tuesday", "wednesday", "thursday", "friday"];
const WEEKEND: [&str; 2] = ["saturday", "sunday"];
const SCHEDULES_PATH: &str = "/2.2/networks/network-0001/profiles/profile-0001/schedules";

/// Which of the two bedtime helpers a table test drives, with the day set it must send.
#[derive(Clone, Copy, Debug)]
enum Bedtime {
    Weekday,
    Weekend,
}

impl Bedtime {
    const ALL: [Self; 2] = [Self::Weekday, Self::Weekend];

    fn days(self) -> &'static [&'static str] {
        match self {
            Self::Weekday => &WEEKDAYS,
            Self::Weekend => &WEEKEND,
        }
    }

    async fn run(
        self,
        client: &Client,
        network_id: Option<&str>,
        parent: Option<&serde_json::Value>,
    ) -> Result<rusteero::envelope::Envelope, Error> {
        match self {
            Self::Weekday => {
                client
                    .set_weekday_bedtime("profile-0001", "21:00", "07:00", network_id, parent)
                    .await
            }
            Self::Weekend => {
                client
                    .set_weekend_bedtime("profile-0001", "21:00", "07:00", network_id, parent)
                    .await
            }
        }
    }
}

/// Mounts a `GET` that must be served exactly `times` times.
async fn mount_get(mock: &MockEero, get_path: &str, body: String, times: u64) {
    Mock::given(method("GET"))
        .and(path(get_path))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(times)
        .mount(&mock.server)
        .await;
}

#[tokio::test]
async fn the_bedtime_helpers_post_one_schedule_with_their_day_set() -> anyhow::Result<()> {
    for bedtime in Bedtime::ALL {
        let mock = MockEero::start().await;
        Mock::given(method("POST"))
            .and(path(SCHEDULES_PATH))
            .and(session_cookie())
            .and(body_json(json!({
                "name": "Bedtime",
                "days": bedtime.days(),
                "start": "21:00",
                "end": "07:00",
                "enabled": true,
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
            .expect(1)
            .mount(&mock.server)
            .await;

        let client = client(&mock).await;
        let (_guard, logs) = capture_logs();
        let response = bedtime
            .run(&client, Some("network-0001"), None)
            .await
            .unwrap_or_else(|err| panic!("{bedtime:?}: {err}"));

        assert_eq!(response.as_value(), &fixture_json("schedule.json"));
        assert_eq!(
            logs.count(Level::WARN, "Issuing write (create_schedule)"),
            1,
            "{bedtime:?}: {:?}",
            logs.warnings()
        );
    }
    Ok(())
}

/// Neither the cached network envelope nor a cached profile envelope is used as the parent: only
/// the caller's own is, and the profile is never fetched on the helper's behalf.
#[tokio::test]
async fn the_bedtime_helpers_ignore_cached_envelopes() -> anyhow::Result<()> {
    for bedtime in Bedtime::ALL {
        let mock = MockEero::start().await;
        let network = json!({
            "meta": {"code": 200},
            "data": {
                "id": "network-0001",
                "url": "/2.4/networks/network-0001",
                "resources": {
                    "schedules": "/2.4/networks/network-0001/profiles/profile-0001/schedules"
                },
            },
        });
        let profile = json!({
            "meta": {"code": 200},
            "data": {
                "url": "/2.2/networks/network-0001/profiles/profile-0001",
                "resources": {
                    "schedules": "/2.3/networks/network-0001/profiles/profile-0001/schedules"
                },
            },
        });
        mount_get(&mock, "/2.2/networks/network-0001", network.to_string(), 1).await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001/profiles/profile-0001",
            profile.to_string(),
            1,
        )
        .await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001/profiles",
            fixture("profiles.json"),
            1,
        )
        .await;
        for distinct in [
            "/2.4/networks/network-0001/profiles/profile-0001/schedules",
            "/2.3/networks/network-0001/profiles/profile-0001/schedules",
        ] {
            Mock::given(method("POST"))
                .and(path(distinct))
                .respond_with(ResponseTemplate::new(500))
                .expect(0)
                .mount(&mock.server)
                .await;
        }
        Mock::given(method("POST"))
            .and(path(SCHEDULES_PATH))
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
            .expect(1)
            .mount(&mock.server)
            .await;

        let client = client(&mock).await;
        client.get_network(Some("network-0001"), false).await?;
        client
            .get_profile("profile-0001", Some("network-0001"), false)
            .await?;
        client.get_profiles(Some("network-0001"), false).await?;

        bedtime
            .run(&client, Some("network-0001"), None)
            .await
            .unwrap_or_else(|err| panic!("{bedtime:?}: {err}"));
    }
    Ok(())
}

/// A `parent` the caller supplies is the profile's own envelope and is forwarded unchanged.
#[tokio::test]
async fn the_bedtime_helpers_forward_an_explicit_profile_parent() -> anyhow::Result<()> {
    for bedtime in Bedtime::ALL {
        let mock = MockEero::start().await;
        Mock::given(method("POST"))
            .and(path(SCHEDULES_PATH))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;
        Mock::given(method("POST"))
            .and(path(
                "/2.2/networks/network-0001/profiles/profile-0001/custom-schedules",
            ))
            .and(session_cookie())
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
            .expect(1)
            .mount(&mock.server)
            .await;

        let parent = json!({"resources": {
            "schedules": "/2.2/networks/network-0001/profiles/profile-0001/custom-schedules"
        }});
        let client = client(&mock).await;
        bedtime
            .run(&client, Some("network-0001"), Some(&parent))
            .await
            .unwrap_or_else(|err| panic!("{bedtime:?}: {err}"));
    }
    Ok(())
}

/// The write drops the profile's own cache entry and the profile list, and nothing else.
#[tokio::test]
async fn the_bedtime_helpers_invalidate_the_profile_and_the_profile_list_only() -> anyhow::Result<()>
{
    for bedtime in Bedtime::ALL {
        let mock = MockEero::start().await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001/profiles/profile-0001",
            fixture("profile.json"),
            2,
        )
        .await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001/profiles/profile-0002",
            fixture("profile.json"),
            1,
        )
        .await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001/profiles",
            fixture("profiles.json"),
            2,
        )
        .await;
        mount_get(
            &mock,
            "/2.2/networks/network-0001",
            fixture("network.json"),
            1,
        )
        .await;
        Mock::given(method("POST"))
            .and(path(SCHEDULES_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture("schedule.json")))
            .expect(1)
            .mount(&mock.server)
            .await;

        let client = client(&mock).await;
        client.get_network(Some("network-0001"), false).await?;
        for _ in 0..2 {
            client
                .get_profile("profile-0001", Some("network-0001"), false)
                .await?;
        }
        client
            .get_profile("profile-0002", Some("network-0001"), false)
            .await?;
        client.get_profiles(Some("network-0001"), false).await?;

        bedtime
            .run(&client, Some("network-0001"), None)
            .await
            .unwrap_or_else(|err| panic!("{bedtime:?}: {err}"));

        // Re-reads: profile-0001 and the list refetch; profile-0002 and the network stay cached.
        client
            .get_profile("profile-0001", Some("network-0001"), false)
            .await?;
        client.get_profiles(Some("network-0001"), false).await?;
        client
            .get_profile("profile-0002", Some("network-0001"), false)
            .await?;
        client.get_network(Some("network-0001"), false).await?;
    }
    Ok(())
}

#[tokio::test]
async fn the_bedtime_helpers_without_a_network_fail_without_discovery() -> anyhow::Result<()> {
    for bedtime in Bedtime::ALL {
        let mock = MockEero::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock.server)
            .await;

        let client = client(&mock).await;
        let err = bedtime
            .run(&client, None, None)
            .await
            .expect_err("no network id and no preferred network must fail");
        assert!(
            matches!(err, Error::MissingNetworkId),
            "{bedtime:?}: {err:?}"
        );
    }
    Ok(())
}
