//! `DnsPoliciesApi`: `dns_policies` endpoints (`eero-api src/eero/api/dns_policies.py`, new in
//! v8.0.0).
//!
//! Premium (Eero Plus/Secure) feature family; replaces the removed `ProfilesAPI` content-filter/
//! block-list/blocked-application writes (`crate::endpoints::profiles`'s module docs) — those
//! never persisted (silent no-op, live-verified). No `DELETE` verb anywhere in this module:
//! removal is expressed as `is_delete: true` on the same `PUT`.
//!
//! Two read operations are live-verified (`get_advanced_content_filter` and
//! `get_profile_applications`). Every write in this module is unverified: its request shape
//! follows the API's own field declarations, but no write here has been confirmed against a live
//! network. Each write logs one warning via [`crate::links::warn_uncharacterised_write`] before
//! it is issued; follow the read-compare-skip discipline and never retry a failed write in a
//! loop.
//!
//! The network-level switches [`DnsPoliciesApi::set_network_ad_block`]
//! (`dns_policies/adblock`) and [`DnsPoliciesApi::set_network_block_malware`]
//! (`dns_policies/network`) are literal sub-paths of the network URL that no envelope publishes
//! as a link; they are exposed because a downstream integration has sent them for years, and
//! they remain uncharacterised writes. They resolve the network base through
//! [`crate::params::resolve_network_url`] (the parent's own `url` when supplied, else the
//! `networks/{id}` template) and append the literal suffix.
//!
//! Still not exposed: the profile-level content-filter settings (`dns_policies/profiles/{id}`,
//! the ten boolean category toggles such as `safe_search_enabled`), pending a live read-back.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use url::Url;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links;
use crate::params;
use crate::routes::{self, ApiVersion, Resource};
use crate::transport::{RequestBody, Transport};
use crate::util::id_from_url;

/// Body keys the network-level `dns_policies/network` switch accepts. The API silently discards
/// unknown keys with a `200`, so a key is checked client-side before any request
/// (`dns_policies.py:49-52`'s `_NETWORK_DNS_POLICY_KEYS`).
const NETWORK_DNS_POLICY_KEYS: &[&str] = &["block_malware"];

/// Rejects a `dns_policies/network` body key that is not in [`NETWORK_DNS_POLICY_KEYS`].
///
/// Ported from `_set_network_dns_policy` (`dns_policies.py:686-689`).
fn validate_network_dns_policy_key(key: &str) -> Result<(), Error> {
    if NETWORK_DNS_POLICY_KEYS.contains(&key) {
        return Ok(());
    }
    Err(Error::validation(
        "key",
        format!(
            "must be one of {}, got {}",
            params::py_list(NETWORK_DNS_POLICY_KEYS),
            params::py_quote(key)
        ),
    ))
}

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

    /// Resolves `route` under the network's own base URL: [`params::resolve_network_url`]
    /// (`parent`'s own published `url` when present, else the `networks/{id}` template at the
    /// route's API version), then the route's literal suffix (the template text after `{id}`).
    ///
    /// The network-level switches are literal sub-paths of the network URL that no envelope
    /// publishes as a link (`dns_policies.py:646-647,691-692`: `f"{base_url}/dns_policies/.."`),
    /// so the parent's `url` is used as the base, never fed back through [`Resource::resolve`]
    /// as if a caller had typed it.
    fn network_route_url(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
    ) -> Result<Url, Error> {
        let base = params::resolve_network_url(
            self.transport.api_host(),
            network_id,
            parent,
            route.version,
        )?;
        let suffix = route
            .template
            .split_once("{id}")
            .map_or("", |(_, suffix)| suffix);
        Url::parse(&format!("{}{suffix}", base.as_str().trim_end_matches('/')))
            .map_err(|err| Error::validation("url", format!("not a valid URL: {err}")))
    }

    /// Resolves `route` under the network's own base URL (see [`Self::network_route_url`]), logs
    /// the fixed uncharacterised-write `operation` warning, then `POST`s `body` — resolve, warn,
    /// post, exactly the order of the two network-level switches in `dns_policies.py`.
    async fn post_network_policy(
        &self,
        route: &Resource,
        network_id: &str,
        parent: Option<&Value>,
        operation: &str,
        body: Value,
    ) -> Result<Envelope, Error> {
        let url = self.network_route_url(route, network_id, parent)?;
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

    /// Normalises profile references to host-relative profile paths
    /// (`/2.2/networks/{network}/profiles/{profile}`), the form the ad-block endpoint takes.
    ///
    /// Ported from `DnsPoliciesAPI._profile_paths` (`dns_policies.py:556-584`). Each entry may be a
    /// bare id, an API-returned path, or an absolute URL on the API host.
    /// [`params::resolve_nested_url`] validates it (rejecting a path that belongs to another
    /// network or another resource family) and returns an absolute URL, whose path is taken.
    /// Order is preserved. Neither a parent nor a published link is consulted: bare ids resolve
    /// under `network_id`'s own template at API 2.2.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "child"` if an entry is not a valid profile
    /// reference for `network_id`.
    fn profile_paths(&self, network_id: &str, profiles: &[&str]) -> Result<Vec<String>, Error> {
        profiles
            .iter()
            .map(|profile| {
                params::resolve_nested_url(
                    self.transport.api_host(),
                    network_id,
                    profile,
                    "profiles",
                    "",
                    None,
                    None,
                    ApiVersion::V2_2,
                )
                .map(|url| url.path().to_owned())
            })
            .collect()
    }

    /// Switches ad blocking on or off, network-wide or per profile — returns the raw Eero API
    /// response.
    ///
    /// `POST`s a JSON body to the network's `dns_policies/adblock` resource: `{"enable":
    /// enabled}`, plus `"profiles"` when `profiles` is `Some`. The `profiles` list **replaces**
    /// the previous list; an empty list with `enabled: true` means network-wide. Pass `None` for
    /// the network-wide switch.
    ///
    /// This path is a literal sub-path of the network URL that no envelope publishes as a link;
    /// it is not in the observed spec and is exposed because a downstream integration has sent it
    /// for years.
    ///
    /// **Unverified write, premium feature.** It has not been confirmed against a live network,
    /// and it is an Eero Plus / Eero Secure feature: `capable: true` in the network's
    /// `premium_dns` block alone does not mean the DNS-policy subsystem is enabled. Follow the
    /// read-compare-skip discipline: read the network (`premium_dns`) first, and only issue this
    /// write when the stored state differs from the desired one. Never retry a failed write in a
    /// loop.
    ///
    /// `profiles` entries are each a bare id, an API-returned path, or an absolute URL on the API
    /// host; they are sent as `/2.2/networks/{network}/profiles/{profile}` paths, in order. An
    /// empty list is sent as `[]`, `None` omits the key.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:586-649` (`DnsPoliciesAPI.
    /// set_network_ad_block`): sends `POST` `routes::dns_policies::DNS_POLICIES_SET_NETWORK_AD_BLOCK`
    /// under `parent`'s own `url` when supplied, else the `networks/{id}` template. The profile
    /// references are validated first, then the URL resolved, then one
    /// `warn_uncharacterised_write("set network ad block for network")` is logged immediately
    /// before the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "child"` if a profile reference is not valid for
    /// `network_id` (empty, under another network, or another resource family), before any
    /// request is sent. Returns `Error::PremiumRequired` if the network is not on Eero
    /// Plus/Secure. Otherwise, see [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn set_network_ad_block(
        &self,
        network_id: &str,
        enabled: bool,
        profiles: Option<&[&str]>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let mut body = Map::new();
        body.insert("enable".to_owned(), Value::Bool(enabled));
        if let Some(profiles) = profiles {
            body.insert(
                "profiles".to_owned(),
                to_string_array_owned(&self.profile_paths(network_id, profiles)?),
            );
        }
        self.post_network_policy(
            &routes::dns_policies::DNS_POLICIES_SET_NETWORK_AD_BLOCK,
            network_id,
            parent,
            "set network ad block for network",
            Value::Object(body),
        )
        .await
    }

    /// `POST`s one boolean switch to the network's `dns_policies/network` resource.
    ///
    /// The API silently discards unknown body keys with a `200`, so `key` is checked against
    /// [`NETWORK_DNS_POLICY_KEYS`] before any request.
    ///
    /// Ported from `DnsPoliciesAPI._set_network_dns_policy` (`dns_policies.py:651-694`): sends
    /// `POST` `routes::dns_policies::DNS_POLICIES_SET_NETWORK_POLICY` with body `{key: value}`,
    /// logging one `warn_uncharacterised_write("set network DNS policy {key} for network")`
    /// immediately before the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] with `field: "key"` if `key` is not a known switch. See
    /// [`DnsPoliciesApi::set_network_ad_block`] for the rest.
    async fn set_network_dns_policy(
        &self,
        network_id: &str,
        key: &str,
        value: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        validate_network_dns_policy_key(key)?;
        self.post_network_policy(
            &routes::dns_policies::DNS_POLICIES_SET_NETWORK_POLICY,
            network_id,
            parent,
            &format!("set network DNS policy {key} for network"),
            json!({ key: value }),
        )
        .await
    }

    /// Switches malware blocking on or off for the network — returns the raw Eero API response.
    ///
    /// `POST`s `{"block_malware": enabled}` to the network's `dns_policies/network` resource.
    ///
    /// This path is a literal sub-path of the network URL that no envelope publishes as a link;
    /// it is not in the observed spec and is exposed because a downstream integration has sent it
    /// for years.
    ///
    /// **Unverified write, premium feature.** It has not been confirmed against a live network,
    /// and it is an Eero Plus / Eero Secure feature: `capable: true` in the network's
    /// `premium_dns` block alone does not mean the DNS-policy subsystem is enabled. Follow the
    /// read-compare-skip discipline: read the network (`premium_dns`) first, and only issue this
    /// write when the stored state differs from the desired one. Never retry a failed write in a
    /// loop.
    ///
    /// Ported from `eero-api src/eero/api/dns_policies.py:696-739`
    /// (`DnsPoliciesAPI.set_network_block_malware`), which delegates to the private
    /// `_set_network_dns_policy` with the key `"block_malware"`; the warning's operation string is
    /// `"set network DNS policy block_malware for network"`.
    ///
    /// # Errors
    ///
    /// Returns `Error::PremiumRequired` if the network is not on Eero Plus/Secure. Otherwise, see
    /// [`DnsPoliciesApi::get_advanced_content_filter`].
    pub async fn set_network_block_malware(
        &self,
        network_id: &str,
        enabled: bool,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        self.set_network_dns_policy(network_id, "block_malware", enabled, parent)
            .await
    }
}

/// Converts owned strings into a JSON array of strings.
fn to_string_array_owned(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{DnsPoliciesApi, validate_network_dns_policy_key};
    use crate::error::Error;
    use crate::transport::Transport;

    #[test]
    fn the_only_network_dns_policy_key_is_block_malware() {
        assert!(validate_network_dns_policy_key("block_malware").is_ok());
    }

    #[test]
    fn an_unknown_network_dns_policy_key_is_rejected_with_pythons_message() {
        let Err(Error::Validation { field, message, .. }) =
            validate_network_dns_policy_key("block_malwre")
        else {
            panic!("a misspelt key must be a validation error");
        };
        assert_eq!(field, "key");
        assert_eq!(
            message,
            "must be one of ['block_malware'], got 'block_malwre'"
        );
    }

    /// The private helper rejects an unknown key before resolving a URL or issuing a request: the
    /// transport below points at a host nothing listens on, so any request would fail with a
    /// network error, not a validation error.
    #[tokio::test]
    async fn set_network_dns_policy_rejects_an_unknown_key_before_any_request() {
        let transport = Transport::builder()
            .base_url("http://mock.test:1")
            .build()
            .expect("a literal http base url is valid");
        let api = DnsPoliciesApi::new(Arc::new(transport));

        let err = api
            .set_network_dns_policy("network-0001", "block_malwre", true, None)
            .await
            .expect_err("an unknown key must be rejected");

        assert!(
            matches!(&err, Error::Validation { field, .. } if field == "key"),
            "{err:?}"
        );
    }
}
