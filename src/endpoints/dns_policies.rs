//! `DnsPoliciesApi`: `dns_policies` endpoints (`eero-api src/eero/api/dns_policies.py`, new in
//! v8.0.0).
//!
//! Premium (Eero Plus/Secure) feature family; replaces the removed `ProfilesAPI` content-filter/
//! block-list/blocked-application writes (`crate::endpoints::profiles`'s module docs) — those
//! never persisted (silent no-op, live-verified). No `DELETE` verb anywhere in this module:
//! removal is expressed as `is_delete: true` on the same `PUT`.
//!
//! Not exposed by this module (and deliberately not ported): network-level and profile-level
//! DNS-policy *settings* (the twelve boolean content-category toggles such as `ad_block`,
//! `block_malware`, `safe_search_enabled`, and the ad-block on/off switches). No field on any
//! envelope this SDK can read carries a URL for them (`dns_policies.py:26-40`'s own module
//! docstring); hardcoding a guessed literal path would be unverifiable.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::routes::{self, Resource};
use crate::transport::{RequestBody, Transport};
use crate::util::id_from_url;

/// Builds a request body from required fields plus only the supplied optional ones.
///
/// Ported from `_payload` (`dns_policies.py:50-65`).
fn payload(required: &[(&str, Value)], optional: &[(&str, Option<Value>)]) -> Value {
    let mut map = Map::new();
    for (key, value) in required {
        map.insert((*key).to_owned(), value.clone());
    }
    for (key, value) in optional {
        if let Some(value) = value {
            map.insert((*key).to_owned(), value.clone());
        }
    }
    Value::Object(map)
}

/// `eero-api`'s `DnsPoliciesAPI` (`src/eero/api/dns_policies.py`, new in v8.0.0).
#[derive(Debug)]
pub struct DnsPoliciesApi {
    transport: Arc<Transport>,
}

impl DnsPoliciesApi {
    /// Wraps `transport` as a `DnsPoliciesApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Resolves `route`'s URL for `network_id`/`parent`, logs the fixed uncharacterised-write
    /// `operation` warning, then `PUT`s `body` — in that exact order, mirroring every write in
    /// `dns_policies.py` (resolve the URL, build the payload, warn, then `put(...)`), rather than
    /// warning before URL resolution/validation can fail.
    async fn put_policy(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
        operation: &str,
        body: Value,
    ) -> Result<Envelope, Error> {
        let url = route.resolve(self.transport.api_host(), network_id, parent)?;
        links::warn_uncharacterised_write(operation);
        self.transport
            .request(route.method.clone(), url, &[], RequestBody::Json(body))
            .await
    }

    /// Gets the network's advanced content filter allow/block lists — returns the raw Eero API
    /// response. Live-verified.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:129-163`
    /// (`DnsPoliciesAPI.get_advanced_content_filter`): sends `GET`
    /// `routes::dns_policies::DNS_POLICIES_GET_ADVANCED_CONTENT_FILTER`, preferring the parent
    /// network envelope's own `advanced_content_filter` link when supplied and resolvable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured,
    /// `Error::PremiumRequired` if the network is not on Eero Plus/Secure, or whatever other
    /// status-mapped [`Error`] the request produces.
    pub async fn get_advanced_content_filter(
        &self,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::dns_policies::DNS_POLICIES_GET_ADVANCED_CONTENT_FILTER,
                network_id,
                parent,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Adds (or, with `is_delete: Some(true)`, removes) a domain from the network-wide allow
    /// list — returns the raw Eero API response.
    ///
    /// Only `domain` plus whichever optional fields are `Some` are sent; every `None` optional is
    /// omitted from the request entirely.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:167-230` (`DnsPoliciesAPI.allow_domain`):
    /// sends `PUT` `routes::dns_policies::DNS_POLICIES_ALLOW_DOMAIN`. Logs one
    /// `warn_uncharacterised_write("allow domain for network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    #[allow(clippy::too_many_arguments)] // mirrors dns_policies.py:167-176's own signature
    pub async fn allow_domain(
        &self,
        network_id: &str,
        domain: &str,
        add_cname: Option<bool>,
        reason_to_allow: Option<i64>,
        is_delete: Option<bool>,
        keep_profiles: Option<&[&str]>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let body = payload(
            &[("domain", Value::String(domain.to_owned()))],
            &[
                ("add_cname", add_cname.map(Value::Bool)),
                ("reason_to_allow", reason_to_allow.map(Into::into)),
                ("is_delete", is_delete.map(Value::Bool)),
                ("keep_profiles", keep_profiles.map(to_string_array)),
            ],
        );
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_ALLOW_DOMAIN,
            network_id,
            parent,
            "allow domain for network",
            body,
        )
        .await
    }

    /// Allows a list of CNAME domains network-wide — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:233-272` (`DnsPoliciesAPI.allow_cnames`):
    /// sends `PUT` `routes::dns_policies::DNS_POLICIES_ALLOW_CNAMES` with body `{"domains":
    /// domains}` — this endpoint declares only the `domains` field. Logs one
    /// `warn_uncharacterised_write("allow CNAMEs for network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn allow_cnames(
        &self,
        network_id: &str,
        domains: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_ALLOW_CNAMES,
            network_id,
            parent,
            "allow CNAMEs for network",
            json!({ "domains": domains }),
        )
        .await
    }

    /// Adds (or, with `is_delete: Some(true)`, removes) a domain from the network-wide block
    /// list — returns the raw Eero API response.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:275-324` (`DnsPoliciesAPI.block_domain`):
    /// sends `PUT` `routes::dns_policies::DNS_POLICIES_BLOCK_DOMAIN`. Logs one
    /// `warn_uncharacterised_write("block domain for network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn block_domain(
        &self,
        network_id: &str,
        domain: &str,
        is_delete: Option<bool>,
        keep_profiles: Option<&[&str]>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let body = payload(
            &[("domain", Value::String(domain.to_owned()))],
            &[
                ("is_delete", is_delete.map(Value::Bool)),
                ("keep_profiles", keep_profiles.map(to_string_array)),
            ],
        );
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_BLOCK_DOMAIN,
            network_id,
            parent,
            "block domain for network",
            body,
        )
        .await
    }

    /// Adds (or, with `is_delete: Some(true)`, removes) a domain from one or more profiles'
    /// allow lists — returns the raw Eero API response.
    ///
    /// `domain` and `profiles` are always sent; the remaining fields only when `Some`.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:327-389`
    /// (`DnsPoliciesAPI.allow_domain_for_profiles`): sends `PUT`
    /// `routes::dns_policies::DNS_POLICIES_ALLOW_DOMAIN_FOR_PROFILES`. Logs one
    /// `warn_uncharacterised_write("allow domain for profiles on network")` immediately before
    /// the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    #[allow(clippy::too_many_arguments)] // mirrors dns_policies.py:327-336's own signature
    pub async fn allow_domain_for_profiles(
        &self,
        network_id: &str,
        domain: &str,
        profiles: &[&str],
        override_: Option<bool>,
        add_cname: Option<bool>,
        reason_to_allow: Option<i64>,
        is_delete: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let body = payload(
            &[
                ("domain", Value::String(domain.to_owned())),
                ("profiles", to_string_array(profiles)),
            ],
            &[
                ("override", override_.map(Value::Bool)),
                ("add_cname", add_cname.map(Value::Bool)),
                ("reason_to_allow", reason_to_allow.map(Into::into)),
                ("is_delete", is_delete.map(Value::Bool)),
            ],
        );
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_ALLOW_DOMAIN_FOR_PROFILES,
            network_id,
            parent,
            "allow domain for profiles on network",
            body,
        )
        .await
    }

    /// Allows a list of CNAME domains for one or more profiles — returns the raw Eero API
    /// response.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:392-432`
    /// (`DnsPoliciesAPI.allow_cnames_for_profiles`): sends `PUT`
    /// `routes::dns_policies::DNS_POLICIES_ALLOW_CNAMES_FOR_PROFILES` with body `{"domains":
    /// domains, "profiles": profiles}`. Logs one `warn_uncharacterised_write("allow CNAMEs for
    /// profiles on network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn allow_cnames_for_profiles(
        &self,
        network_id: &str,
        domains: &[&str],
        profiles: &[&str],
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_ALLOW_CNAMES_FOR_PROFILES,
            network_id,
            parent,
            "allow CNAMEs for profiles on network",
            json!({ "domains": domains, "profiles": profiles }),
        )
        .await
    }

    /// Adds (or, with `is_delete: Some(true)`, removes) a domain from one or more profiles'
    /// block lists — returns the raw Eero API response.
    ///
    /// `domain` and `profiles` are always sent; `is_delete`/`override_` only when `Some`.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:437-488`
    /// (`DnsPoliciesAPI.block_domain_for_profiles`): sends `PUT`
    /// `routes::dns_policies::DNS_POLICIES_BLOCK_DOMAIN_FOR_PROFILES`. Logs one
    /// `warn_uncharacterised_write("block domain for profiles on network")` immediately before
    /// the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn block_domain_for_profiles(
        &self,
        network_id: &str,
        domain: &str,
        profiles: &[&str],
        is_delete: Option<bool>,
        override_: Option<bool>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let body = payload(
            &[
                ("domain", Value::String(domain.to_owned())),
                ("profiles", to_string_array(profiles)),
            ],
            &[
                ("is_delete", is_delete.map(Value::Bool)),
                ("override", override_.map(Value::Bool)),
            ],
        );
        self.put_policy(
            &routes::dns_policies::DNS_POLICIES_BLOCK_DOMAIN_FOR_PROFILES,
            network_id,
            parent,
            "block domain for profiles on network",
            body,
        )
        .await
    }

    /// Gets the applications a profile can block, and which are blocked — returns the raw Eero
    /// API response. Live-verified.
    ///
    /// `profile_id` is first normalised to its trailing id via [`id_from_url`] (a no-op for an
    /// already-bare id) before being substituted into the `dns_policies/profiles/{id}` sub-tree —
    /// a profile's own path/URL (`networks/{id}/profiles/{pid}`) cannot be mapped onto that
    /// sub-tree by template substitution, since the two resource families share only the trailing
    /// id (`dns_policies.py:77-92`). Takes no `parent`: this sub-tree publishes no link on any
    /// envelope this SDK can read.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:491-521`
    /// (`DnsPoliciesAPI.get_profile_applications`): sends `GET`
    /// `routes::dns_policies::DNS_POLICIES_GET_PROFILE_APPLICATIONS`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] if `profile_id` cannot be normalised to a trailing path
    /// segment (see [`id_from_url`]). Otherwise, see
    /// [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn get_profile_applications(
        &self,
        network_id: &str,
        profile_id: &str,
    ) -> Result<Envelope, Error> {
        let child = id_from_url(profile_id)?;
        self.transport
            .nested(
                &routes::dns_policies::DNS_POLICIES_GET_PROFILE_APPLICATIONS,
                network_id,
                &child,
                None,
                &[],
                RequestBody::None,
            )
            .await
    }

    /// Sets the applications blocked for a profile — returns the raw Eero API response.
    ///
    /// **Fully replaces** the profile's blocked-applications list; there is no add/remove
    /// primitive. Same `profile_id` normalisation as
    /// [`DnsPoliciesApi::get_profile_applications`]. Takes no `parent`.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:521-551`
    /// (`DnsPoliciesAPI.set_profile_blocked_applications`): sends `PUT`
    /// `routes::dns_policies::DNS_POLICIES_SET_PROFILE_BLOCKED_APPLICATIONS` with body
    /// `{"applications": applications}`. Logs one `warn_uncharacterised_write("set blocked
    /// applications for profile on network")` immediately before the request.
    ///
    /// # Errors
    ///
    /// See [`DnsPoliciesApi::get_profile_applications`].
    pub async fn set_profile_blocked_applications(
        &self,
        network_id: &str,
        profile_id: &str,
        applications: &[&str],
    ) -> Result<Envelope, Error> {
        let child = id_from_url(profile_id)?;
        let url = routes::dns_policies::DNS_POLICIES_SET_PROFILE_BLOCKED_APPLICATIONS.resolve(
            self.transport.api_host(),
            network_id,
            &child,
            None,
        )?;
        links::warn_uncharacterised_write("set blocked applications for profile on network");
        self.transport
            .request(
                routes::dns_policies::DNS_POLICIES_SET_PROFILE_BLOCKED_APPLICATIONS
                    .method
                    .clone(),
                url,
                &[],
                RequestBody::Json(json!({ "applications": applications })),
            )
            .await
    }
}

/// Converts a slice of string references into a JSON array of strings.
fn to_string_array(values: &[&str]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|v| Value::String((*v).to_owned()))
            .collect(),
    )
}
