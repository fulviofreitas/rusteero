//! Diagnostics API: `eero-api`'s `DiagnosticsAPI` (`src/eero/api/diagnostics.py` at v8.0.4).
//!
//! Implements both `DiagnosticsAPI` methods: [`DiagnosticsApi::get_diagnostics`] and
//! [`DiagnosticsApi::run_diagnostics`].

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `DiagnosticsAPI` (`src/eero/api/diagnostics.py`).
///
/// Build one with [`DiagnosticsApi::new`], wrapping a [`Transport`] already shared with the rest
/// of the `EeroApi` aggregator — `DiagnosticsApi` never constructs or owns a `Transport` itself.
#[derive(Debug)]
pub struct DiagnosticsApi {
    transport: Arc<Transport>,
}

impl DiagnosticsApi {
    /// Wraps `transport` as a `DiagnosticsApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Gets network diagnostics information — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/diagnostics.py:35-64` (`DiagnosticsAPI.get_diagnostics`).
    /// Sends `GET` [`crate::routes::GET_DIAGNOSTICS`], preferring `parent`'s
    /// `resources.diagnostics` link over the `networks/{id}/diagnostics` template.
    pub async fn get_diagnostics(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::GET_DIAGNOSTICS,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Runs network diagnostics — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/diagnostics.py:66-113` (`DiagnosticsAPI.run_diagnostics`).
    /// Sends `POST` [`crate::routes::RUN_DIAGNOSTICS`] (the same path as
    /// [`crate::routes::GET_DIAGNOSTICS`]) with a JSON body carrying only the caller-supplied
    /// `device`/`symptom` keys — an empty JSON object `{}` when neither is given, never an absent
    /// body, matching `diagnostics.py:105-111` exactly. **Unverified body shape against a live
    /// network.**
    pub async fn run_diagnostics(
        &self,
        network_id: &str,
        device: Option<&str>,
        symptom: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        crate::links::warn_uncharacterised_write("run diagnostics for network");

        let mut body = Map::new();
        if let Some(device) = device {
            body.insert("device".to_owned(), Value::String(device.to_owned()));
        }
        if let Some(symptom) = symptom {
            body.insert("symptom".to_owned(), Value::String(symptom.to_owned()));
        }

        self.transport
            .resource(
                &routes::RUN_DIAGNOSTICS,
                network_id,
                parent,
                &[],
                RequestBody::Json(Value::Object(body)),
            )
            .await
    }
}
