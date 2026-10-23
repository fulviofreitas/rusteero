//! Schedule API: `eero-api`'s `ScheduleAPI` at v8.4.1.
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

/// All seven days, spelled exactly as `eero-api`'s bedtime helpers do (`schedule.py:37-45`), in
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

/// Monday through Friday, verbatim from `eero-api src/eero/api/schedule.py:47`
/// (`ScheduleAPI.set_weekday_bedtime`'s `WEEKDAYS` tuple).
const WEEKDAYS: &[&str] = &["monday", "tuesday", "wednesday", "thursday", "friday"];

/// Saturday and Sunday, verbatim from `eero-api src/eero/api/schedule.py:48`
/// (`ScheduleAPI.set_weekend_bedtime`'s `WEEKEND` tuple).
const WEEKEND: &[&str] = &["saturday", "sunday"];

/// The shape of a scheduled pause's own path, after the version segment (`schedule.py:34`,
/// `_SCHEDULE_FAMILY`).
const SCHEDULE_FAMILY: &str = "networks/{id}/profiles/{id}/schedules/{id}";

/// Why a bare id cannot name a scheduled pause (`schedule.py:83-88`): a pause lives under a
/// network and a profile that the update and delete methods are not given.
const BARE_ID_MESSAGE: &str =
    "must be the schedule's path, absolute URL or envelope; a bare id cannot be resolved";

/// Every optional keyword argument [`ScheduleApi::update_schedule`] accepts.
///
/// More than four optional keyword arguments, so they travel as one options struct (this
/// crate's convention). Ported from `ScheduleAPI.update_schedule`'s own five independent
/// keyword-only fields (`schedule.py:215-223`).
#[derive(Debug, Default, Clone)]
pub struct UpdateScheduleOptions<'a> {
    /// The schedule's name. Omitted from the request when `None`.
    pub name: Option<&'a str>,
    /// The days the schedule applies to. Omitted from the request when `None`.
    pub days: Option<&'a [&'a str]>,
    /// The schedule's start time. Omitted from the request when `None`.
    pub start: Option<&'a str>,
    /// The schedule's end time. Omitted from the request when `None`.
    pub end: Option<&'a str>,
    /// Whether the schedule is enabled. Omitted from the request when `None`.
    pub enabled: Option<bool>,
}

/// `eero-api`'s `ScheduleAPI` (`src/eero/api/schedule.py`) at v8.4.1.
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
    /// Ported from `_resolve_schedule_url` (`schedule.py:51-95`), which takes one argument that
    /// is *either* a cached pause envelope *or* an id/path/URL string — decomposed here into two
    /// Rust parameters per this port's id-or-url-plus-parent convention. The two are mutually
    /// exclusive, not a preference-with-fallback: when `parent` is `Some`, `id_or_url` is ignored
    /// entirely, exactly like Python's `Mapping` branch (`clear_profile_schedule` passes `""`);
    /// when `parent` is `None`, `id_or_url` is the `str` branch.
    ///
    /// - **An envelope** (`parent`, a full envelope or its `data` object) is trusted as published:
    ///   its `url` is taken through [`links::self_url`], which applies only the plain-path shape
    ///   rule (field `link`), and is **not** confined to the schedule family. An envelope with no
    ///   resolvable `url` is a validation error, not a silent fall-back to `id_or_url`.
    /// - **A bare id** is refused up front (field `schedule`): a pause lives under a network and
    ///   a profile that these methods are not given, so the id alone cannot be resolved, and
    ///   placing it under the version root would address whatever one-segment endpoint shares its
    ///   name (`/2.2/account`).
    /// - **A path or absolute URL a caller supplies** goes through the route's `"{id}"` template
    ///   (the shape rules, field `id_or_url` or `url`) and must then be exactly a scheduled
    ///   pause's own path, `/<version>/networks/{id}/profiles/{id}/schedules/{id}` (field
    ///   `schedule`), so a write cannot be redirected to another resource on the API host. The
    ///   caller's own version is kept, and every trailing `/` is dropped before the check.
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
        if !is_path_or_url(id_or_url) {
            return Err(Error::validation("schedule", BARE_ID_MESSAGE));
        }
        let url = route.resolve(host, id_or_url, None)?;
        links::require_family_path(
            id_or_url.trim_end_matches('/'),
            SCHEDULE_FAMILY,
            "",
            "schedule",
        )?;
        Ok(url)
    }

    /// Gets the scheduled pauses for a profile — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:144-171` (`ScheduleAPI.get_schedules`):
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
    /// Ported from `eero-api src/eero/api/schedule.py:173-213` (`ScheduleAPI.create_schedule`):
    /// sends `POST` `routes::schedule::SCHEDULE_CREATE_SCHEDULE` with body `{"name": name, "days":
    /// days, "start": start, "end": end, "enabled": enabled}`. Logs one
    /// `warn_uncharacterised_write("create_schedule")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`ScheduleApi::get_schedules`].
    #[allow(clippy::too_many_arguments)] // mirrors schedule.py:173-183's own signature
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
        let url = routes::schedule::SCHEDULE_CREATE_SCHEDULE.resolve(
            self.transport.api_host(),
            network_id,
            profile_id,
            parent,
        )?;
        links::warn_uncharacterised_write("create_schedule");
        self.transport
            .request(
                routes::schedule::SCHEDULE_CREATE_SCHEDULE.method.clone(),
                url,
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
    /// Ported from `eero-api src/eero/api/schedule.py:215-272` (`ScheduleAPI.update_schedule`):
    /// sends `PUT` `routes::schedule::SCHEDULE_UPDATE`. Logs one
    /// `warn_uncharacterised_write("update_schedule")` immediately before the request — but only
    /// once local validation has passed (see the `# Errors` section below), matching Python's own
    /// ordering (`schedule.py:263-270`).
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if every one of `name`, `days`,
    /// `start`, `end`, `enabled` is `None` — checked *before* any request is sent, matching
    /// Python's own `if not payload: raise ...` (`schedule.py:263-266`), and before the
    /// reference below is looked at. Returns `Error::Validation { field: "schedule", .. }` if
    /// `schedule` is a bare id, if a `schedule` path or URL is not a scheduled pause's own path,
    /// or if `parent` has no resolvable `url`; a shape error in a path, URL or published `url`
    /// (a dot segment, a control character, a query) reports `id_or_url`, `url` or `link`.
    /// Otherwise, [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces.
    pub async fn update_schedule(
        &self,
        schedule: &str,
        options: &UpdateScheduleOptions<'_>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut payload = serde_json::Map::new();
        if let Some(name) = options.name {
            payload.insert("name".to_owned(), Value::String(name.to_owned()));
        }
        if let Some(days) = options.days {
            payload.insert(
                "days".to_owned(),
                Value::Array(
                    days.iter()
                        .map(|d| Value::String((*d).to_owned()))
                        .collect(),
                ),
            );
        }
        if let Some(start) = options.start {
            payload.insert("start".to_owned(), Value::String(start.to_owned()));
        }
        if let Some(end) = options.end {
            payload.insert("end".to_owned(), Value::String(end.to_owned()));
        }
        if let Some(enabled) = options.enabled {
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
    /// Ported from `eero-api src/eero/api/schedule.py:274-298` (`ScheduleAPI.delete_schedule`):
    /// sends `DELETE` `routes::schedule::SCHEDULE_DELETE`. Logs one
    /// `warn_uncharacterised_write("delete_schedule")` immediately before the request.
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if `schedule` is a bare id, if a
    /// `schedule` path or URL is not a scheduled pause's own path, or if `parent` has no
    /// resolvable `url`; a shape error in a path, URL or published `url` (a dot segment, a
    /// control character, a query) reports `id_or_url`, `url` or `link`. Otherwise,
    /// [`Error::Authentication`] if no valid session is configured, or whatever status-mapped
    /// [`Error`] the request produces.
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
    /// (`schedule.py:300-343`) exactly.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:300-343`
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
            // Ported from `delete_schedule(pause)` (`schedule.py:342`): Python passes the pause's
            // own *envelope* (mapping), not a bare URL string, so this goes through the same
            // `parent`-is-`Some` branch of `resolve_schedule_url`/`_resolve_schedule_url` every
            // other caller uses — the placeholder `""` `id_or_url` is never consulted, matching
            // `_resolve_schedule_url`'s `Mapping` branch, which ignores nothing else. This is also
            // what gives an unresolvable pause the *identical* message
            // (`"envelope has no resolvable 'url' field"`, field `"schedule"`) every other
            // resolution failure in this module produces, and the identical treat-empty-as-missing
            // rule ([`links::self_url`]).
            results.push(self.delete_schedule("", Some(pause)).await?);
        }
        Ok(results)
    }

    /// Creates a single bedtime scheduled pause for a profile, scoped to `days` (all seven days
    /// if `None`) — returns the raw Eero API response.
    ///
    /// Built on [`ScheduleApi::create_schedule`] (one pause), not a `schedule` array written onto
    /// the profile.
    ///
    /// Ported from `eero-api src/eero/api/schedule.py:345-394` (`ScheduleAPI.enable_bedtime`).
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
    /// Ported from `eero-api src/eero/api/schedule.py:396-419` (`ScheduleAPI.set_weekday_bedtime`).
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
    /// Ported from `eero-api src/eero/api/schedule.py:421-444` (`ScheduleAPI.set_weekend_bedtime`).
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

/// Returns `true` when `reference` is a host-relative path (`/…`) or an absolute `http(s)` URL
/// rather than a bare id (`schedule.py:83`; the scheme is matched case-insensitively).
fn is_path_or_url(reference: &str) -> bool {
    let lower = reference.to_lowercase();
    reference.starts_with('/') || lower.starts_with("http://") || lower.starts_with("https://")
}
