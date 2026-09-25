//! HTTP integration tests for `ScheduleApi` (`src/endpoints/schedule.rs`) against a local
//! `wiremock` server per the crate's testing conventions.
//!
//! [`ScheduleApi::get_profile_schedule`] hits the exact same path as `ProfilesApi::get_profile`
//! and returns the whole profile object untransformed — no `schedule`-field extraction.
//!
//! Every mutating test pins the exact verb, path and JSON body (`body_json`) `wiremock`
//! receives, plus the session cookie, with a `.expect(n)` call count — a wrong verb, path, or
//! body shape fails to match the mock and the request comes back unmocked (404), which is what
//! turns a request-shape regression into a loud test failure instead of a silent pass.
//!
//! [`set_weekday_bedtime_emits_a_bedtime_block_scoped_to_monday_through_friday`] proves the
//! weekday delegator emits the Monday-through-Friday day list, not all seven days; it was
//! red-then-green verified by hand (see the task report for the exact command output).

mod common;

use std::sync::Arc;

use rusteero::endpoints::schedule::ScheduleApi;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};

fn schedule_api(mock: &MockEero) -> ScheduleApi {
    ScheduleApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== get_profile_schedule =====================

#[tokio::test]
async fn get_profile_schedule_hits_networks_profiles_id_and_returns_the_whole_profile()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    let response = json!({
        "meta": { "code": 200 },
        "data": {
            "name": "Kids",
            "schedule": [{ "days": ["monday"], "start": "21:00", "end": "07:00" }],
        },
    });

    Mock::given(method("GET"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let env = api
        .get_profile_schedule("network-0001", "profile-0001")
        .await?;
    // Untransformed: the whole profile object comes back, not just its `schedule` field.
    assert_eq!(env.into_value(), response);
    Ok(())
}

// ===================== set_profile_schedule =====================

#[tokio::test]
async fn set_profile_schedule_puts_the_time_blocks_verbatim() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "schedule": [
                { "days": ["monday", "wednesday"], "start": "08:00", "end": "15:00" }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    let block = json!({ "days": ["monday", "wednesday"], "start": "08:00", "end": "15:00" });
    api.set_profile_schedule("network-0001", "profile-0001", &[block])
        .await?;
    Ok(())
}

// ===================== clear_profile_schedule =====================

#[tokio::test]
async fn clear_profile_schedule_puts_an_empty_array_not_null() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "schedule": [] })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.clear_profile_schedule("network-0001", "profile-0001")
        .await?;
    Ok(())
}

// ===================== enable_bedtime =====================

#[tokio::test]
async fn enable_bedtime_with_no_days_defaults_to_all_seven() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "schedule": [{
                "days": [
                    "monday", "tuesday", "wednesday", "thursday",
                    "friday", "saturday", "sunday"
                ],
                "start": "21:00",
                "end": "07:00",
                "type": "bedtime"
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.enable_bedtime("network-0001", "profile-0001", "21:00", "07:00", None)
        .await?;
    Ok(())
}

#[tokio::test]
async fn enable_bedtime_with_explicit_days_uses_them_instead_of_the_default() -> anyhow::Result<()>
{
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "schedule": [{
                "days": ["friday"],
                "start": "22:00",
                "end": "06:00",
                "type": "bedtime"
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.enable_bedtime(
        "network-0001",
        "profile-0001",
        "22:00",
        "06:00",
        Some(&["friday"]),
    )
    .await?;
    Ok(())
}

// ===================== set_weekday_bedtime =====================

/// Proves the weekday delegator (`schedule.py:169-188`) scopes its bedtime block to Monday
/// through Friday — not all seven days, and not some other subset.
///
/// Red-then-green verified by hand: temporarily changing `set_weekday_bedtime`'s call site in
/// `src/endpoints/schedule.rs` to pass `Some(ALL_DAYS)` instead of `Some(WEEKDAYS)` made this
/// test fail (the mocked body no longer matched, so the request came back unmocked); restoring
/// `Some(WEEKDAYS)` made it pass again. See the task report for the exact command output.
#[tokio::test]
async fn set_weekday_bedtime_emits_a_bedtime_block_scoped_to_monday_through_friday()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "schedule": [{
                "days": ["monday", "tuesday", "wednesday", "thursday", "friday"],
                "start": "20:30",
                "end": "06:30",
                "type": "bedtime"
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.set_weekday_bedtime("network-0001", "profile-0001", "20:30", "06:30")
        .await?;
    Ok(())
}

// ===================== set_weekend_bedtime =====================

#[tokio::test]
async fn set_weekend_bedtime_emits_a_bedtime_block_scoped_to_saturday_and_sunday()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "schedule": [{
                "days": ["saturday", "sunday"],
                "start": "23:00",
                "end": "09:00",
                "type": "bedtime"
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = schedule_api(&mock);
    api.set_weekend_bedtime("network-0001", "profile-0001", "23:00", "09:00")
        .await?;
    Ok(())
}
