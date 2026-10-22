//! HTTP integration tests for `ScheduleApi` (v8.0.4) — scheduled pauses as sub-resources of a
//! profile, per the crate's testing conventions.

mod common;

use std::sync::Arc;

use rusteero::endpoints::schedule::{ScheduleApi, UpdateScheduleOptions};
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{any, body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{
    MockEero, TEST_TOKEN, capture_logs, fixture, fixture_json, session_cookie, user_token_header,
};

fn schedule_api(mock: &MockEero) -> ScheduleApi {
    ScheduleApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

/// A genuine scheduled pause's own path.
const SCHEDULE_PATH: &str = "/2.2/networks/n1/profiles/p1/schedules/s1";

/// The message `update_schedule` and `delete_schedule` give a bare id.
const BARE_ID_MESSAGE: &str =
    "must be the schedule's path, absolute URL or envelope; a bare id cannot be resolved";

/// The message they give a path or URL that is not a scheduled pause.
const FAMILY_MESSAGE: &str =
    "must be a path under networks/{id}/profiles/{id}/schedules/{id} on the API host";

/// Which of the two schedule writes a table test drives.
#[derive(Clone, Copy, Debug)]
enum Write {
    Update,
    Delete,
}

impl Write {
    const ALL: [Self; 2] = [Self::Update, Self::Delete];

    fn verb(self) -> &'static str {
        match self {
            Self::Update => "PUT",
            Self::Delete => "DELETE",
        }
    }

    /// Runs the write against `target`, passing no envelope.
    async fn run(
        self,
        api: &ScheduleApi,
        target: &str,
    ) -> Result<rusteero::envelope::Envelope, Error> {
        match self {
            Self::Update => {
                api.update_schedule(
                    target,
                    &UpdateScheduleOptions {
                        enabled: Some(false),
                        ..UpdateScheduleOptions::default()
                    },
                    None,
                )
                .await
            }
            Self::Delete => api.delete_schedule(target, None).await,
        }
    }

    /// Runs the write against a published envelope.
    async fn run_with_envelope(
        self,
        api: &ScheduleApi,
        envelope: &serde_json::Value,
    ) -> Result<rusteero::envelope::Envelope, Error> {
        match self {
            Self::Update => {
                api.update_schedule(
                    "ignored",
                    &UpdateScheduleOptions {
                        enabled: Some(false),
                        ..UpdateScheduleOptions::default()
                    },
                    Some(envelope),
                )
                .await
            }
            Self::Delete => api.delete_schedule("ignored", Some(envelope)).await,
        }
    }
}

/// Asserts `err` is a client-side validation failure on `field` with exactly `message`.
fn assert_validation(err: &Error, field: &str, message: &str, context: &str) {
    match err {
        Error::Validation {
            field: got_field,
            message: got_message,
            ..
        } => {
            assert_eq!(got_field, field, "{context}");
            assert_eq!(got_message, message, "{context}");
        }
        other => panic!("{context}: expected a validation error, got {other:?}"),
    }
}

/// Mounts a catch-all mock that fails the test if it receives any request at all.
async fn expect_no_request(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock.server)
        .await;
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

// ===================== schedule-family validation =====================

#[tokio::test]
async fn delete_schedule_rejects_a_bare_identifier_that_is_not_a_schedule_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let err = api
        .delete_schedule("networks", None)
        .await
        .expect_err("a bare identifier can never resolve to a schedule family path");
    assert_validation(&err, "schedule", BARE_ID_MESSAGE, "networks");
    Ok(())
}

#[tokio::test]
async fn delete_schedule_rejects_an_unrelated_same_host_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let err = api
        .delete_schedule("/2.2/networks/1", None)
        .await
        .expect_err("a same-host path outside the schedule family must be rejected");
    assert_validation(&err, "schedule", FAMILY_MESSAGE, "/2.2/networks/1");
    Ok(())
}

#[tokio::test]
async fn update_schedule_rejects_a_bare_identifier_that_is_not_a_schedule_path()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
        .expect(0)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let err = api
        .update_schedule(
            "networks",
            &UpdateScheduleOptions {
                enabled: Some(false),
                ..UpdateScheduleOptions::default()
            },
            None,
        )
        .await
        .expect_err("a bare identifier can never resolve to a schedule family path");
    assert!(matches!(err, Error::Validation { ref field, .. } if field == "schedule"));
    Ok(())
}

#[tokio::test]
async fn update_schedule_still_accepts_a_genuine_schedule_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
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
async fn delete_schedule_still_accepts_a_genuine_schedule_envelope() -> anyhow::Result<()> {
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
    let parent = json!({
        "url": "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001"
    });
    api.delete_schedule("ignored", Some(&parent)).await?;
    Ok(())
}

// ===================== bare ids, caller paths and published envelopes =====================

/// A bare id is refused before it is resolved: a pause lives under a network and a profile these
/// methods are not given, and `/2.2/<id>` could address an unrelated one-segment endpoint.
#[tokio::test]
async fn a_bare_schedule_id_is_refused_before_any_request() -> anyhow::Result<()> {
    for write in Write::ALL {
        for target in ["s1", "account", "networks", "S1", "../account", ""] {
            let mock = MockEero::start().await;
            expect_no_request(&mock).await;
            let (_guard, logs) = capture_logs();

            let err = write
                .run(&schedule_api(&mock), target)
                .await
                .expect_err("a bare id must be refused");

            assert_validation(
                &err,
                "schedule",
                BARE_ID_MESSAGE,
                &format!("{write:?} {target:?}"),
            );
            assert!(
                logs.warnings().is_empty(),
                "{write:?} {target:?} warned: {:?}",
                logs.warnings()
            );
        }
    }
    Ok(())
}

/// The empty-payload check still comes first, as in Python.
#[tokio::test]
async fn update_schedule_checks_the_payload_before_the_bare_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    expect_no_request(&mock).await;

    let err = schedule_api(&mock)
        .update_schedule("s1", &UpdateScheduleOptions::default(), None)
        .await
        .expect_err("an empty payload must be refused");

    assert_validation(
        &err,
        "schedule",
        "at least one of name, days, start, end, enabled must be supplied",
        "empty payload",
    );
    Ok(())
}

/// A path or URL a caller supplies must be exactly a scheduled pause's own: no other resource,
/// no extra or missing component, and every identifier starts with an ASCII alphanumeric.
#[tokio::test]
async fn a_path_that_is_not_a_scheduled_pause_is_refused_before_any_request() -> anyhow::Result<()>
{
    for write in Write::ALL {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;
        let uri = mock.uri();
        let targets = [
            "/2.2/networks/n1".to_owned(),
            "/2.2/networks/n1/forwards/f1".to_owned(),
            "/2.2/account".to_owned(),
            "/2.2/networks/n1/profiles/p1".to_owned(),
            "/2.2/networks/n1/profiles/p1/schedules".to_owned(),
            "/2.2/networks/n1/profiles/p1/schedules/s1/extra".to_owned(),
            "/2.2/networks/n1/profiles/p1/devices/s1".to_owned(),
            "/2.2/networks/n1/extra/profiles/p1/schedules/s1".to_owned(),
            "/2.2/profiles/p1/schedules/s1".to_owned(),
            format!("{uri}/2.2/networks/n1/profiles/p1/schedules"),
            format!("{uri}/2.2/account"),
            format!("{uri}/2.2/networks/n1/profiles/p1/devices/s1"),
            "/2.2/networks/n1/profiles/_p1/schedules/s1".to_owned(),
        ];

        for target in &targets {
            let err = write
                .run(&schedule_api(&mock), target)
                .await
                .expect_err("a path that is not a scheduled pause must be refused");
            assert_validation(
                &err,
                "schedule",
                FAMILY_MESSAGE,
                &format!("{write:?} {target:?}"),
            );
        }
    }
    Ok(())
}

/// A scheduled pause's own path or URL reaches that exact URL with the right verb, on the version
/// the caller wrote, whatever trailing slashes it carries.
#[tokio::test]
async fn a_scheduled_pauses_own_path_and_url_are_sent_as_given() -> anyhow::Result<()> {
    for write in Write::ALL {
        for (target_form, version) in [
            ("path", "2.2"),
            ("url", "2.2"),
            ("upper-case-scheme-url", "2.2"),
            ("path-and-slash", "2.2"),
            ("path-and-two-slashes", "2.2"),
            ("path", "2.3"),
        ] {
            let mock = MockEero::start().await;
            let uri = mock.uri();
            let own = format!("/{version}/networks/n1/profiles/p1/schedules/s1");
            Mock::given(method(write.verb()))
                .and(path(own.clone()))
                .and(session_cookie())
                .and(user_token_header())
                .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
                .expect(1)
                .mount(&mock.server)
                .await;

            let target = match target_form {
                "path" => own.clone(),
                "url" => format!("{uri}{own}"),
                "upper-case-scheme-url" => format!("{}{own}", uri.replacen("http", "HTTP", 1)),
                "path-and-slash" => format!("{own}/"),
                _ => format!("{own}//"),
            };
            write
                .run(&schedule_api(&mock), &target)
                .await
                .unwrap_or_else(|err| panic!("{write:?} {target:?}: {err}"));
        }
    }
    Ok(())
}

/// A link the API published in an envelope is trusted as published and not confined to a
/// scheduled pause's shape: only the plain-path rule applies.
#[tokio::test]
async fn a_published_envelope_url_is_not_confined_to_the_schedule_family() -> anyhow::Result<()> {
    for write in Write::ALL {
        let mock = MockEero::start().await;
        Mock::given(method(write.verb()))
            .and(path("/2.4/some/other/published/shape"))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .expect(1)
            .mount(&mock.server)
            .await;

        let envelope = json!({"url": "/2.4/some/other/published/shape"});
        write
            .run_with_envelope(&schedule_api(&mock), &envelope)
            .await
            .unwrap_or_else(|err| panic!("{write:?}: {err}"));
    }
    Ok(())
}

/// That includes a network's own envelope handed over by mistake: it is sent to the network's
/// URL, as Python does.
#[tokio::test]
async fn a_network_envelope_passed_as_the_schedule_is_sent_to_its_own_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/1"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let network_envelope = json!({"url": "/2.2/networks/1"});
    schedule_api(&mock)
        .delete_schedule("ignored", Some(&network_envelope))
        .await?;
    Ok(())
}

/// A full `{meta, data}` envelope resolves through its `data.url`.
#[tokio::test]
async fn a_full_pause_envelope_resolves_through_its_data_url() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/n1/profiles/p1/schedules/s1"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let envelope = json!({"meta": {"code": 200}, "data": {"url": SCHEDULE_PATH}});
    Write::Update
        .run_with_envelope(&schedule_api(&mock), &envelope)
        .await?;
    Ok(())
}

/// A dot segment in a caller's path is refused as a path (field `id_or_url`) rather than resolved
/// away by the URL parser into a pause's own path.
#[tokio::test]
async fn a_schedule_path_with_a_dot_segment_is_refused_before_any_request() -> anyhow::Result<()> {
    for write in Write::ALL {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        let err = write
            .run(
                &schedule_api(&mock),
                "/2.2/networks/n1/profiles/p1/schedules/x/../s1",
            )
            .await
            .expect_err("a dot segment must be refused");

        assert_validation(
            &err,
            "id_or_url",
            "must not contain dot segments",
            &format!("{write:?}"),
        );
    }
    Ok(())
}

/// A dot segment in a published envelope url is refused too (field `link`).
#[tokio::test]
async fn a_published_envelope_url_with_a_dot_segment_is_refused() -> anyhow::Result<()> {
    for write in Write::ALL {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        let envelope = json!({"url": "/2.4/some/../shape"});
        let err = write
            .run_with_envelope(&schedule_api(&mock), &envelope)
            .await
            .expect_err("a dot segment in a published url must be refused");

        assert_validation(
            &err,
            "link",
            "must not contain dot segments",
            &format!("{write:?}"),
        );
    }
    Ok(())
}

/// A published url that is not a plain host-relative path is still refused, never sent.
#[tokio::test]
async fn a_published_envelope_url_must_still_be_a_plain_host_relative_path() -> anyhow::Result<()> {
    for write in Write::ALL {
        let mock = MockEero::start().await;
        expect_no_request(&mock).await;

        for url in [
            "https://elsewhere.example/2.2/networks/n1/profiles/p1/schedules/s1",
            "//elsewhere.example/2.2/networks/n1/profiles/p1/schedules/s1",
            "/2.2/networks/n1/profiles/p1/schedules/s1?x=1",
        ] {
            let envelope = json!({ "url": url });
            let err = write
                .run_with_envelope(&schedule_api(&mock), &envelope)
                .await
                .expect_err("a malformed published url must be refused");
            assert!(
                matches!(err, Error::Validation { ref field, .. } if field == "link"),
                "{write:?} {url:?} gave {err:?}"
            );
        }
    }
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

/// Each pause is deleted through its own published url, which is trusted as published: a pause
/// whose url is not schedule-shaped is deleted too instead of aborting the clear.
#[tokio::test]
async fn clear_profile_schedule_deletes_each_pause_through_its_published_url() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    let pauses = json!({
        "meta": {"code": 200},
        "data": [
            {"url": "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001"},
            {"url": "/2.4/some/other/published/shape"},
        ],
    });
    Mock::given(method("GET"))
        .and(path(
            "/2.2/networks/network-0001/profiles/profile-0001/schedules",
        ))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(ResponseTemplate::new(200).set_body_string(pauses.to_string()))
        .expect(1)
        .mount(&mock.server)
        .await;
    for deleted in [
        "/2.2/networks/network-0001/profiles/profile-0001/schedules/schedule-0001",
        "/2.4/some/other/published/shape",
    ] {
        Mock::given(method("DELETE"))
            .and(path(deleted))
            .and(session_cookie())
            .and(user_token_header())
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"meta":{"code":200}}"#))
            .expect(1)
            .mount(&mock.server)
            .await;
    }

    let results = schedule_api(&mock)
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
