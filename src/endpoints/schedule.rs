//! Schedule API: `eero-api`'s `ScheduleAPI` at v8.0.4.
//!
//! Ported from `eero-api src/eero/api/schedule.py`. Scheduled pauses are sub-resources of a
//! profile, not a field on the profile itself: they live at
//! `networks/{id}/profiles/{profile}/schedules`, are created with a `POST` to that collection,
//! and are updated/deleted through their own URL — replacing the pre-v8 design of writing a
//! `schedule` array directly onto the profile (`get_profile_schedule`/`set_profile_schedule`,
//! both removed; see `routes::schedule`'s module docs).
//!
//! None of the writes in this module have been verified against a live network. Each logs a
//! [`crate::links::warn_uncharacterised_write`] before the request; follow the
//! read-compare-skip discipline documented there.

use std::sync::Arc;

use serde_json::{Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes::{self, Resource};
use crate::transport::{RequestBody, Transport};

/// All seven days, spelled exactly as `eero-api`'s bedtime helpers do (`schedule.py:34-42`), in
/// the order [`ScheduleApi::enable_bedtime`]'s own default uses.
const ALL_DAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// Monday through Friday, verbatim from `eero-api src/eero/api/schedule.py:44`
/// (`ScheduleAPI.set_weekday_bedtime`'s `WEEKDAYS` tuple).
const WEEKDAYS: &[&str] = &["monday", "tuesday", "wednesday", "thursday", "friday"];

/// Saturday and Sunday, verbatim from `eero-api src/eero/api/schedule.py:45`
/// (`ScheduleAPI.set_weekend_bedtime`'s `WEEKEND` tuple).
const WEEKEND: &[&str] = &["saturday", "sunday"];

/// `eero-api`'s `ScheduleAPI` (`src/eero/api/schedule.py`) at v8.0.4.
#[derive(Debug)]
pub struct ScheduleApi {
    transport: Arc<Transport>,
}

impl ScheduleApi {
    /// Wraps `transport` as a `ScheduleApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Resolves a single scheduled pause's request URL.
    ///
    /// Ported from `_resolve_schedule_url` (`schedule.py:48-74`), which takes one argument that
    /// is *either* a cached pause envelope *or* an id/path/URL string — decomposed here into two
    /// Rust parameters per this port's id-or-url-plus-parent convention
    /// (`.claude/tasks/briefs/v8/phase-g-rules.md` item 2). The two are mutually exclusive, not a
    /// preference-with-fallback: when `parent` is `Some`, `id_or_url` is ignored entirely and
    /// [`links::self_url`] must resolve something, exactly like Python's `Mapping` branch (a
    /// `parent` envelope with no `url` field is a validation error, not a silent fallback to
    /// `id_or_url`); when `parent` is `None`, `id_or_url` is resolved via `route`'s `"{id}"`
    /// template, exactly like Python's `str` branch.
    fn resolve_schedule_url(
        &self,
        route: &Resource,
        id_or_url: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let host = self.transport.api_host();
        if let Some(parent) = parent {
            return links::self_url(host, parent)?.ok_or_else(|| {
                Error::validation("schedule", "envelope has no resolvable 'url' field")
            });
        }
        route.resolve(host, id_or_url, None)
    }

    /// Gets the scheduled pauses for a profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:123-150` (`ScheduleAPI.get_schedules`):
    /// sends `GET` `routes::schedule::SCHEDULE_GET_SCHEDULES`
    /// (`networks/{network_id}/profiles/{profile_id}/schedules`), preferring the profile's own
    /// published `schedules` link when `parent` is supplied and resolvable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, [`Error::Validation`]
    /// if the URL cannot be resolved, or whatever status-mapped [`Error`] the request produces.
    pub async fn get_schedules(
        &self,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .nested(
                &routes::schedule::SCHEDULE_GET_SCHEDULES,
                network_id,
                profile_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Creates a scheduled pause for a profile — returns the raw Eero API response.
    ///
    /// `enabled` has no Python default here — call with `true` to reproduce
    /// `create_schedule`'s own `enabled: bool = True` default. All five body keys (`name`,
    /// `days`, `start`, `end`, `enabled`) are always sent.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:152-192` (`ScheduleAPI.create_schedule`):
    /// sends `POST` `routes::schedule::SCHEDULE_CREATE_SCHEDULE` with body `{"name": name, "days":
    /// days, "start": start, "end": end, "enabled": enabled}`. Logs one
    /// `warn_uncharacterised_write("create_schedule")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`ScheduleApi::get_schedules`].
    #[allow(clippy::too_many_arguments)] // mirrors schedule.py:152-162's own signature
    pub async fn create_schedule(
        &self,
        network_id: &str,
        profile_id: &str,
        name: &str,
        days: &[&str],
        start: &str,
        end: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        links::warn_uncharacterised_write("create_schedule");
        self.transport
            .nested(
                &routes::schedule::SCHEDULE_CREATE_SCHEDULE,
                network_id,
                profile_id,
                parent,
                &[],
                RequestBody::Json(json!({
                    "name": name,
                    "days": days,
                    "start": start,
                    "end": end,
                    "enabled": enabled,
                })),
            )
            .await
    }

    /// Updates a scheduled pause via its own URL — returns the raw Eero API response.
    ///
    /// Every field is optional; only the supplied ones are sent. See
    /// `resolve_schedule_url` for how `schedule`/`parent` resolve the pause's URL.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:194-247` (`ScheduleAPI.update_schedule`):
    /// sends `PUT` `routes::schedule::SCHEDULE_UPDATE`. Logs one
    /// `warn_uncharacterised_write("update_schedule")` immediately before the request — but only
    /// once local validation has passed (see the `# Errors` section below), matching Python's own
    /// ordering (`schedule.py:227-246`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if every one of `name`, `days`,
    /// `start`, `end`, `enabled` is `None` — checked *before* any request is sent, matching
    /// Python's own `if not payload: raise ...` (`schedule.py:227-231`). Returns
    /// `Error::Validation { field: "schedule", .. }` if `schedule`/`parent` cannot be resolved to
    /// a URL. Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    #[allow(clippy::too_many_arguments)] // mirrors schedule.py:194-202's own five-optional-keyword signature
    pub async fn update_schedule(
        &self,
        schedule: &str,
        name: Option<&str>,
        days: Option<&[&str]>,
        start: Option<&str>,
        end: Option<&str>,
        enabled: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut payload = serde_json::Map::new();
        if let Some(name) = name {
            payload.insert("name".to_owned(), Value::String(name.to_owned()));
        }
        if let Some(days) = days {
            payload.insert(
                "days".to_owned(),
                Value::Array(
                    days.iter()
                        .map(|d| Value::String((*d).to_owned()))
                        .collect(),
                ),
            );
        }
        if let Some(start) = start {
            payload.insert("start".to_owned(), Value::String(start.to_owned()));
        }
        if let Some(end) = end {
            payload.insert("end".to_owned(), Value::String(end.to_owned()));
        }
        if let Some(enabled) = enabled {
            payload.insert("enabled".to_owned(), Value::Bool(enabled));
        }

        if payload.is_empty() {
            return Err(Error::validation(
                "schedule",
                "at least one of name, days, start, end, enabled must be supplied",
            ));
        }

        let url =
            self.resolve_schedule_url(&routes::schedule::SCHEDULE_UPDATE, schedule, parent)?;
        links::warn_uncharacterised_write("update_schedule");
        self.transport
            .request(
                routes::schedule::SCHEDULE_UPDATE.method.clone(),
                url,
                &[],
                RequestBody::Json(Value::Object(payload)),
            )
            .await
    }

    /// Deletes a scheduled pause via its own URL — returns the raw Eero API response.
    ///
    /// See `resolve_schedule_url` for how `schedule`/`parent` resolve the pause's
    /// URL.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:250-270` (`ScheduleAPI.delete_schedule`):
    /// sends `DELETE` `routes::schedule::SCHEDULE_DELETE`. Logs one
    /// `warn_uncharacterised_write("delete_schedule")` immediately before the request.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if `schedule`/`parent` cannot be
    /// resolved to a URL. Otherwise, [`Error::Authentication`] if no valid session is configured,
    /// or whatever status-mapped [`Error`] the request produces.
    pub async fn delete_schedule(
        &self,
        schedule: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let url =
            self.resolve_schedule_url(&routes::schedule::SCHEDULE_DELETE, schedule, parent)?;
        links::warn_uncharacterised_write("delete_schedule");
        self.transport
            .request(
                routes::schedule::SCHEDULE_DELETE.method.clone(),
                url,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Deletes every scheduled pause currently set on a profile.
    ///
    /// Issues one read ([`ScheduleApi::get_schedules`]) followed by one `DELETE` per existing
    /// pause. Never retried; a pause that fails to delete aborts every remaining delete and its
    /// error propagates immediately — matching Python's own "not swallowed" behaviour
    /// (`schedule.py:274-317`) exactly.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:274-317`
    /// (`ScheduleAPI.clear_profile_schedule`). Unlike every other method in this module, returns
    /// `Vec<Envelope>` — one raw response per `DELETE`, in read order — matching Python's own
    /// `List[Dict[str, Any]]` return shape.
    ///
    /// # Errors
    ///
    /// Whatever [`ScheduleApi::get_schedules`] or [`ScheduleApi::delete_schedule`] returns on
    /// failure; a delete failure aborts immediately, leaving any remaining pauses undeleted.
    pub async fn clear_profile_schedule(
        &self,
        network_id: &str,
        profile_id: &str,
        parent: Option<&Value>,
    ) -> Result<Vec<Envelope>, Error> {
        let schedules_response = self.get_schedules(network_id, profile_id, parent).await?;
        let data = schedules_response.data();
        let pauses: &[Value] = data.as_array().map_or(&[], Vec::as_slice);

        let mut results = Vec::with_capacity(pauses.len());
        for pause in pauses {
            let self_url = pause.get("url").and_then(Value::as_str).ok_or_else(|| {
                Error::validation("schedule", "pause envelope has no resolvable 'url' field")
            })?;
            results.push(self.delete_schedule(self_url, None).await?);
        }
        Ok(results)
    }

    /// Creates a single bedtime scheduled pause for a profile, scoped to `days` (all seven days
    /// if `None`) — returns the raw Eero API response.
    ///
    /// Built on [`ScheduleApi::create_schedule`] (one pause), not a `schedule` array written onto
    /// the profile.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:319-368` (`ScheduleAPI.enable_bedtime`).
    ///
    /// # Errors
    ///
    /// See [`ScheduleApi::create_schedule`].
    pub async fn enable_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        days: Option<&[&str]>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let days = days.unwrap_or(ALL_DAYS);
        self.create_schedule(
            network_id, profile_id, "Bedtime", days, start_time, end_time, true, parent,
        )
        .await
    }

    /// Sets bedtime for weekdays only (Monday through Friday) — returns the raw Eero API
    /// response.
    ///
    /// Delegates to [`ScheduleApi::enable_bedtime`] with `days` fixed to `["monday", "tuesday",
    /// "wednesday", "thursday", "friday"]`.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:370-393` (`ScheduleAPI.set_weekday_bedtime`).
    ///
    /// # Errors
    ///
    /// See [`ScheduleApi::create_schedule`].
    pub async fn set_weekday_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.enable_bedtime(
            network_id,
            profile_id,
            start_time,
            end_time,
            Some(WEEKDAYS),
            parent,
        )
        .await
    }

    /// Sets bedtime for weekends only (Saturday and Sunday) — returns the raw Eero API response.
    ///
    /// Delegates to [`ScheduleApi::enable_bedtime`] with `days` fixed to `["saturday",
    /// "sunday"]`.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:395-418` (`ScheduleAPI.set_weekend_bedtime`).
    ///
    /// # Errors
    ///
    /// See [`ScheduleApi::create_schedule`].
    pub async fn set_weekend_bedtime(
        &self,
        network_id: &str,
        profile_id: &str,
        start_time: &str,
        end_time: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.enable_bedtime(
            network_id,
            profile_id,
            start_time,
            end_time,
            Some(WEEKEND),
            parent,
        )
        .await
    }
}
