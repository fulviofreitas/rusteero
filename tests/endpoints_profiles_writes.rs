//! HTTP integration tests for the mutating halves of `ProfilesApi` and `ScheduleApi`
//! (`pause_profile`, `set_profile_devices`, `update_profile_content_filter`,
//! `update_profile_block_list`, `set_blocked_applications`, `create_profile`,
//! `rename_profile`, `delete_profile`, `set_profile_schedule`, `clear_profile_schedule`,
//! `enable_bedtime`, `set_weekday_bedtime`, `set_weekend_bedtime`), per
//! the crate's testing conventions.
//!
//! Every test pins the exact verb, path and JSON body (`body_json`) `wiremock` receives, plus
//! the session cookie, with a `.expect(n)` call count — a wrong verb, path, or body shape fails
//! to match the mock and the request comes back unmocked (404), which is what turns a request-
//! shape regression into a loud test failure instead of a silent pass.
//!
//! Two tests here matter more than the rest and were red-then-green verified by hand (see the
//! task report for the actual command output):
//! - [`update_profile_content_filter_drops_a_key_outside_the_whitelist_and_keeps_the_rest`]
//!   proves the client-side content-filter key whitelist (`profiles.py:201-217`) actually drops
//!   an unlisted key rather than forwarding it.
//! - [`set_weekday_bedtime_emits_a_bedtime_block_scoped_to_monday_through_friday`] proves the
//!   weekday delegator emits the Monday-through-Friday day list, not all seven days.

mod common;

use std::sync::Arc;

use rusteero::endpoints::profiles::ProfilesApi;
use rusteero::endpoints::schedule::ScheduleApi;
use rusteero::error::Error;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, fixture, session_cookie};

fn profiles_api(mock: &MockEero) -> ProfilesApi {
    ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

fn schedule_api(mock: &MockEero) -> ScheduleApi {
    ScheduleApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
}

// ===================== pause_profile =====================

#[tokio::test]
async fn pause_profile_puts_paused_true_to_the_profile_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "paused": true })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let env = api
        .pause_profile("network-0001", "profile-0001", true)
        .await?;

    assert_eq!(env.meta().code, Some(200));
    Ok(())
}

#[tokio::test]
async fn pause_profile_puts_paused_false_to_unpause() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "paused": false })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.pause_profile("network-0001", "profile-0001", false)
        .await?;
    Ok(())
}

// ===================== set_profile_devices =====================

#[tokio::test]
async fn set_profile_devices_wraps_each_url_in_its_own_object_and_replaces_the_list()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "devices": [
                { "url": "/2.2/networks/network-0001/devices/device-0001" },
                { "url": "/2.2/networks/network-0001/devices/device-0002" }
            ]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.set_profile_devices(
        "network-0001",
        "profile-0001",
        &[
            "/2.2/networks/network-0001/devices/device-0001",
            "/2.2/networks/network-0001/devices/device-0002",
        ],
    )
    .await?;
    Ok(())
}

// ===================== update_profile_content_filter =====================

/// Proves the client-side content-filter whitelist (`profiles.py:201-217`) drops
/// `"not_a_real_filter"` — a key outside `VALID_CONTENT_FILTER_KEYS` — while keeping the two
/// whitelisted keys, `adblock` and `safe_search`. The mock only matches the whitelisted-only
/// body; if the implementation forwarded the extra key, this request would not match any mock
/// and the call would fail with an unmocked-request error instead of `Ok`.
///
/// Red-then-green verified by hand: temporarily short-circuiting the whitelist filter in
/// `src/endpoints/profiles.rs` (forwarding `filters` unfiltered) made this test fail; restoring
/// the filter made it pass again. See the task report for the exact command output.
#[tokio::test]
async fn update_profile_content_filter_drops_a_key_outside_the_whitelist_and_keeps_the_rest()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "content_filter": {
                "adblock": true,
                "safe_search": false
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.update_profile_content_filter(
        "network-0001",
        "profile-0001",
        &[
            ("adblock", true),
            ("safe_search", false),
            ("not_a_real_filter", true),
        ],
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_profile_content_filter_accepts_every_whitelisted_key() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({
            "content_filter": {
                "adblock": true,
                "adblock_plus": true,
                "safe_search": true,
                "block_malware": true,
                "block_illegal": true,
                "block_violent": true,
                "block_adult": true,
                "youtube_restricted": true
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.update_profile_content_filter(
        "network-0001",
        "profile-0001",
        &[
            ("adblock", true),
            ("adblock_plus", true),
            ("safe_search", true),
            ("block_malware", true),
            ("block_illegal", true),
            ("block_violent", true),
            ("block_adult", true),
            ("youtube_restricted", true),
        ],
    )
    .await?;
    Ok(())
}

// ===================== update_profile_block_list =====================

#[tokio::test]
async fn update_profile_block_list_true_sends_custom_block_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(
            json!({ "custom_block_list": ["ads.example.com", "tracker.example.com"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.update_profile_block_list(
        "network-0001",
        "profile-0001",
        &["ads.example.com", "tracker.example.com"],
        true,
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn update_profile_block_list_false_sends_custom_allow_list_not_block_list()
-> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(
            json!({ "custom_allow_list": ["homework.example.com"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.update_profile_block_list(
        "network-0001",
        "profile-0001",
        &["homework.example.com"],
        false,
    )
    .await?;
    Ok(())
}

// ===================== set_blocked_applications =====================

#[tokio::test]
async fn set_blocked_applications_puts_the_full_replacement_list() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(
            json!({ "blocked_applications": ["tiktok", "fortnite"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.set_blocked_applications("network-0001", "profile-0001", &["tiktok", "fortnite"])
        .await?;
    Ok(())
}

// ===================== create_profile =====================

#[tokio::test]
async fn create_profile_posts_the_name_to_the_profiles_list_path() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("POST"))
        .and(path("/2.2/networks/network-0001/profiles"))
        .and(session_cookie())
        .and(body_json(json!({ "name": "Guests" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.create_profile("network-0001", "Guests").await?;
    Ok(())
}

// ===================== rename_profile =====================

#[tokio::test]
async fn rename_profile_puts_the_new_name() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("PUT"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .and(body_json(json!({ "name": "Teenagers" })))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.rename_profile("network-0001", "profile-0001", "Teenagers")
        .await?;
    Ok(())
}

// ===================== delete_profile =====================

#[tokio::test]
async fn delete_profile_sends_no_body() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/profiles/profile-0001"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("profile.json")))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    api.delete_profile("network-0001", "profile-0001").await?;
    Ok(())
}

#[tokio::test]
async fn delete_profile_404_maps_to_error_api_and_is_not_an_auth_error() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("DELETE"))
        .and(path("/2.2/networks/network-0001/profiles/missing-profile"))
        .and(session_cookie())
        .respond_with(ResponseTemplate::new(404).set_body_string("no such profile"))
        .expect(1)
        .mount(&mock.server)
        .await;

    let api = profiles_api(&mock);
    let err = api
        .delete_profile("network-0001", "missing-profile")
        .await
        .expect_err("a 404 must surface as Error::Api");

    let Error::Api { status, .. } = &err else {
        panic!("expected Error::Api, got {err:?}");
    };
    assert_eq!(*status, 404);
    assert!(!err.is_auth_error());
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
