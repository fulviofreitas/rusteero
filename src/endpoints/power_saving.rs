//! `PowerSavingApi`: `power_saving` endpoints (`eero-api src/eero/api/power_saving.py`, new in
//! v8.0.0).
//!
//! Implements all five `PowerSavingAPI` methods: [`PowerSavingApi::set_power_saving`],
//! [`PowerSavingApi::get_schedules`], [`PowerSavingApi::create_schedule`],
//! [`PowerSavingApi::update_schedule`], [`PowerSavingApi::delete_schedule`].
//!
//! [`PowerSavingApi::update_schedule`]/[`PowerSavingApi::delete_schedule`]'s
//! [`crate::links::warn_uncharacterised_write`] calls deliberately never include `schedule_id` in
//! their fixed operation string — consistent with that helper's own contract that `operation`
//! never carry a caller-supplied identifier (`_writes.py:49-56`).

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `PowerSavingAPI` (`src/eero/api/power_saving.py`, new in v8.0.0).
///
/// Build one with [`PowerSavingApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the `EeroApi` aggregator — `PowerSavingApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct PowerSavingApi {
    transport: Arc<Transport>,
}

impl PowerSavingApi {
    /// Wraps `transport` as a `PowerSavingApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Enables/disables power saving, or its schedule, for a network — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/power_saving.py:38-96` (`PowerSavingAPI.set_power_saving`).
    /// Sends `PUT` [`crate::routes::SET_POWER_SAVING`] with a JSON body carrying only the
    /// caller-supplied `enable`/`power_saving_schedule_enabled` keys. **May reboot the mesh;
    /// unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "power_saving", .. }` if both `enable` and
    /// `power_saving_schedule_enabled` are `None`. Otherwise as every other method here.
    pub async fn set_power_saving(
        &self,
        network_id: &str,
        enable: Option<bool>,
        power_saving_schedule_enabled: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        if let Some(enable) = enable {
            body.insert("enable".to_owned(), Value::Bool(enable));
        }
        if let Some(schedule_enabled) = power_saving_schedule_enabled {
            body.insert(
                "power_saving_schedule_enabled".to_owned(),
                Value::Bool(schedule_enabled),
            );
        }
        if body.is_empty() {
            return Err(Error::validation(
                "power_saving",
                "at least one of enable, power_saving_schedule_enabled must be supplied",
            ));
        }

        crate::links::warn_uncharacterised_write("set power saving for network");
        self.transport
            .resource(
                &routes::SET_POWER_SAVING,
                network_id,
                parent,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// Lists power-saving schedules for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/power_saving.py:98-121` (`PowerSavingAPI.get_schedules`).
    /// `parent` is accepted, matching the Python signature, but — verified v8.0.4 behaviour, not
    /// an oversight — is never consulted for URL resolution: [`crate::routes::GET_SCHEDULES`]
    /// declares `link: None`, so [`crate::routes::Resource::resolve`] never looks at `parent` at
    /// all for this one route, unlike every other method in this module. **Live-verified.**
    pub async fn get_schedules(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_SCHEDULES,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Creates a power-saving schedule for a network — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/power_saving.py:123-165` (`PowerSavingAPI.create_schedule`).
    /// No `parent=` parameter in Python. Sends `POST` [`crate::routes::CREATE_SCHEDULE`] with a
    /// JSON body always carrying all five keys (`name`, `days`, `start_time`, `end_time`,
    /// `enabled`) — `days` is forwarded unchanged as an opaque JSON value, matching Python's own
    /// untyped `days` parameter. **Unverified against a live network.**
    pub async fn create_schedule(
        &self,
        network_id: &str,
        name: &str,
        days: Value,
        start_time: &str,
        end_time: &str,
        enabled: bool,
    ) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("create power saving schedule for network");
        self.transport
            .resource(
                &routes::CREATE_SCHEDULE,
                network_id,
                None,
                &[],
                RequestBody::Json(json!({
                    "name": name,
                    "days": days,
                    "start_time": start_time,
                    "end_time": end_time,
                    "enabled": enabled,
                })),
            )
            .await
    }

    /// Updates a power-saving schedule — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/power_saving.py:167-224` (`PowerSavingAPI.update_schedule`).
    /// No `parent=` parameter in Python. Sends `PUT` [`crate::routes::UPDATE_SCHEDULE`] with a
    /// JSON body carrying only the caller-supplied keys among `name`, `days`, `start_time`,
    /// `end_time`, `enabled`. See the module docs for why `schedule_id` never appears in the
    /// warning log line this emits. **Unverified against a live network.**
    ///
    /// # Errors
    ///
    /// Returns `Error::Validation { field: "schedule", .. }` if every one of `name`, `days`,
    /// `start_time`, `end_time`, `enabled` is `None`. Otherwise as every other method here.
    #[allow(clippy::too_many_arguments)] // mirrors power_saving.py:167-176's own five independent keyword-only fields
    pub async fn update_schedule(
        &self,
        network_id: &str,
        schedule_id: &str,
        name: Option<&str>,
        days: Option<Value>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        enabled: Option<bool>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        if let Some(name) = name {
            body.insert("name".to_owned(), Value::String(name.to_owned()));
        }
        if let Some(days) = days {
            body.insert("days".to_owned(), days);
        }
        if let Some(start_time) = start_time {
            body.insert(
                "start_time".to_owned(),
                Value::String(start_time.to_owned()),
            );
        }
        if let Some(end_time) = end_time {
            body.insert("end_time".to_owned(), Value::String(end_time.to_owned()));
        }
        if let Some(enabled) = enabled {
            body.insert("enabled".to_owned(), Value::Bool(enabled));
        }
        if body.is_empty() {
            return Err(Error::validation(
                "schedule",
                "at least one of name, days, start_time, end_time, enabled must be supplied",
            ));
        }

        // Deliberately no `schedule_id` in this fixed operation string — see the module docs.
        crate::links::warn_uncharacterised_write("update power saving schedule for network");
        self.transport
            .nested(
                &routes::UPDATE_SCHEDULE,
                network_id,
                schedule_id,
                None,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// Deletes a power-saving schedule — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/power_saving.py:226-248` (`PowerSavingAPI.delete_schedule`).
    /// No `parent=` parameter in Python. See the module docs for why `schedule_id` never appears
    /// in the warning log line this emits. **Unverified against a live network.**
    pub async fn delete_schedule(
        &self,
        network_id: &str,
        schedule_id: &str,
    ) -> Result<Envelope, Error> {
        // Deliberately no `schedule_id` in this fixed operation string — see the module docs.
        crate::links::warn_uncharacterised_write("delete power saving schedule for network");
        self.transport
            .nested(
                &routes::DELETE_SCHEDULE,
                network_id,
                schedule_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }
}
