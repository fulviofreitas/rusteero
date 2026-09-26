//! `BackupAccessPointsApi`: `backup_access_points` endpoints (`eero-api
//! src/eero/api/backup_access_points.py`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/backup_access_points.py` (v8.0.4). See
//! `.claude/tasks/briefs/v8/g7-backup-members.md` §1 for the per-method table this file is
//! scoped by.
//!
//! Every method here funnels through `Transport::resource`/`Transport::nested`, which already
//! implement the "not authenticated" precondition Python repeats at the top of each method
//! (`get_auth_token()` / `EeroAuthenticationException("Not authenticated")`) and every
//! status-to-error mapping a response can produce — so, unlike the Python source, no method
//! below duplicates that guard.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// Every field the update (`PUT`) write can carry — see [`BackupAccessPointsApi::update`].
///
/// More than four optional keyword arguments, per this port's conventions
/// (`.claude/tasks/briefs/v8/g7-backup-members.md` "rules" reference,
/// `.claude/tasks/briefs/v8/phase-g-rules.md` item 2). `connectivity` is kept as an opaque
/// [`Value`] rather than a narrower Rust type: Python declares it `Optional[Mapping[str, Any]]`
/// (`backup_access_points.py:128`) with no further shape validation, and the wire shape of a
/// "connectivity status" object is not established by anything read while porting this domain.
/// `created`/`last_updated_at`, by contrast, are `Option<&'a str>` — Python types both
/// `Optional[str]` (`backup_access_points.py:129-130`), not an opaque mapping (phase-G fix list
/// item 25).
#[derive(Debug, Default, Clone)]
pub struct UpdateBackupAccessPointOptions<'a> {
    /// New SSID for the access point.
    pub ssid: Option<&'a str>,
    /// New password for the access point. Never logged — see [`BackupAccessPointsApi::update`]'s
    /// own docs.
    pub password: Option<&'a str>,
    /// Whether the access point is enabled.
    pub enabled: Option<bool>,
    /// The access point's UUID.
    pub uuid: Option<&'a str>,
    /// Opaque connectivity-state value, forwarded verbatim.
    pub connectivity: Option<Value>,
    /// The backup network's creation timestamp, forwarded verbatim.
    pub created: Option<&'a str>,
    /// The backup network's last-updated timestamp, forwarded verbatim.
    pub last_updated_at: Option<&'a str>,
}

/// `eero-api`'s `BackupAccessPointsAPI` (`src/eero/api/backup_access_points.py`, new in
/// v8.0.0).
///
/// Build one with [`BackupAccessPointsApi::new`], wrapping a [`Transport`] already shared with
/// the rest of the [`crate::api::EeroApi`] aggregator — `BackupAccessPointsApi` never constructs
/// or owns a `Transport` itself.
#[derive(Debug)]
pub struct BackupAccessPointsApi {
    transport: Arc<Transport>,
}

impl BackupAccessPointsApi {
    /// Wraps `transport` as a `BackupAccessPointsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// `GET /2.2/networks/{network_id}/backup_access_points` — list backup access points.
    ///
    /// Ported from `BackupAccessPointsAPI.list` (`backup_access_points.py:39-72`). Prefers
    /// `parent`'s own published `backup_access_points` link over the literal template when
    /// supplied (`routes::BACKUP_ACCESS_POINTS_LIST::link`). Returns the raw
    /// `{"meta": …, "data": {...}}` envelope; this method never inspects or reshapes it.
    ///
    /// # Errors
    ///
    /// Returns `Error::Authentication("Not authenticated")` if no valid session is configured,
    /// before any request is sent, or whatever other status-mapped error the request produces —
    /// see `Transport::resource`.
    pub async fn list(&self, network_id: &str, parent: Option<&Value>) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_LIST,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/backup_access_points` — add a backup access point.
    ///
    /// Ported from `BackupAccessPointsAPI.add` (`backup_access_points.py:74-117`). Sends
    /// `{"ssid": ssid, "password": password}`, plus `"uuid"` only when `uuid` is `Some`
    /// (`backup_access_points.py:106-109`) — the omitted key is absent from the body entirely,
    /// never sent as `null`. `password` is never logged (this crate's `Transport` never logs
    /// request bodies at any level — see `.claude/rules/security-review.md`).
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn add(
        &self,
        network_id: &str,
        ssid: &str,
        password: &str,
        uuid: Option<&str>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        body.insert("ssid".to_owned(), Value::String(ssid.to_owned()));
        body.insert("password".to_owned(), Value::String(password.to_owned()));
        if let Some(uuid) = uuid {
            body.insert("uuid".to_owned(), Value::String(uuid.to_owned()));
        }
        crate::links::warn_uncharacterised_write("add backup access point for network");
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_ADD,
                network_id,
                None,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// `PUT /2.2/networks/{network_id}/backup_access_points/{backup_network_id}` — update a
    /// backup access point.
    ///
    /// Ported from `BackupAccessPointsAPI.update` (`backup_access_points.py:119-195`). Sends
    /// only the fields actually supplied in `options`, one body key per `Some` field, same key
    /// names as [`UpdateBackupAccessPointOptions`]'s own fields (`backup_access_points.py:174-188`).
    /// `backup_network_id` is a bare id (see the module docs' "no id-or-url polymorphism" note).
    /// `password` is never logged — see [`Self::add`].
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn update(
        &self,
        network_id: &str,
        backup_network_id: &str,
        options: &UpdateBackupAccessPointOptions<'_>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        if let Some(ssid) = options.ssid {
            body.insert("ssid".to_owned(), Value::String(ssid.to_owned()));
        }
        if let Some(password) = options.password {
            body.insert("password".to_owned(), Value::String(password.to_owned()));
        }
        if let Some(enabled) = options.enabled {
            body.insert("enabled".to_owned(), Value::Bool(enabled));
        }
        if let Some(uuid) = options.uuid {
            body.insert("uuid".to_owned(), Value::String(uuid.to_owned()));
        }
        if let Some(connectivity) = options.connectivity.clone() {
            body.insert("connectivity".to_owned(), connectivity);
        }
        if let Some(created) = options.created {
            body.insert("created".to_owned(), Value::String(created.to_owned()));
        }
        if let Some(last_updated_at) = options.last_updated_at {
            body.insert(
                "last_updated_at".to_owned(),
                Value::String(last_updated_at.to_owned()),
            );
        }
        crate::links::warn_uncharacterised_write("update backup access point for network");
        self.transport
            .nested(
                &routes::BACKUP_ACCESS_POINTS_UPDATE,
                network_id,
                backup_network_id,
                None,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }

    /// `DELETE /2.2/networks/{network_id}/backup_access_points/{backup_network_id}` — delete a
    /// backup access point.
    ///
    /// Ported from `BackupAccessPointsAPI.delete_backup_access_point`
    /// (`backup_access_points.py:197-229`). The identifier is never logged — see [`Self::add`].
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn delete_backup_access_point(
        &self,
        network_id: &str,
        backup_network_id: &str,
    ) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("delete backup access point for network");
        self.transport
            .nested(
                &routes::BACKUP_ACCESS_POINTS_DELETE,
                network_id,
                backup_network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/backup_access_points/rearrange` — reorder backup access
    /// points.
    ///
    /// Ported from `BackupAccessPointsAPI.rearrange` (`backup_access_points.py:231-259`). Sends
    /// `{"rearranged_ids": order}` verbatim — no dedup or non-empty check, matching Python
    /// exactly (`backup_access_points.py:259`).
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn rearrange(&self, network_id: &str, order: &[&str]) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("rearrange backup access points for network");
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_REARRANGE,
                network_id,
                None,
                &[],
                RequestBody::Json(json!({ "rearranged_ids": order })),
            )
            .await
    }

    /// `GET /2.2/networks/{network_id}/backup_access_points/ssid_discovery` — read discovered
    /// SSIDs.
    ///
    /// Ported from `BackupAccessPointsAPI.discover_ssids` (`backup_access_points.py:261-283`).
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn discover_ssids(&self, network_id: &str) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_DISCOVER_SSIDS,
                network_id,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/backup_access_points/ssid_discovery` — start SSID
    /// discovery.
    ///
    /// Ported from `BackupAccessPointsAPI.start_ssid_discovery`
    /// (`backup_access_points.py:285-314`). Sends the literal two-byte body `""`
    /// (`RequestBody::EmptyJsonString`), matching `RequestEncoding.EMPTY_JSON_STRING`
    /// (`backup_access_points.py:312-313`) — **not** an empty JSON object `{}`.
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn start_ssid_discovery(&self, network_id: &str) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("start SSID discovery for network");
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_START_SSID_DISCOVERY,
                network_id,
                None,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }

    /// `POST /2.2/networks/{network_id}/backup_access_points/connectivity_check` — start a
    /// backup connectivity check.
    ///
    /// Ported from `BackupAccessPointsAPI.connectivity_check`
    /// (`backup_access_points.py:316-346`). Sends the literal two-byte body `""` — see
    /// [`Self::start_ssid_discovery`].
    ///
    /// # Errors
    ///
    /// See [`Self::list`].
    pub async fn connectivity_check(&self, network_id: &str) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("start backup connectivity check for network");
        self.transport
            .resource(
                &routes::BACKUP_ACCESS_POINTS_CONNECTIVITY_CHECK,
                network_id,
                None,
                &[],
                RequestBody::EmptyJsonString,
            )
            .await
    }
}
