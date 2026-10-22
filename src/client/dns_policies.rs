//! `Client` methods for the `DnsPoliciesAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api`'s `EeroClient` dns_policies-scoped wrappers
//! (`.claude/tasks/briefs/v8/client.md` §4 "`dns_policies` (entirely new — no v6.2.0 equivalent)").
//! Every method here passes `auto_discover = false` (`client.md`'s `net_id` column is `—` for the
//! whole table) and, except [`Client::get_dns_policy_applications`]/
//! [`Client::set_profile_blocked_applications`] (no published link for that sub-tree — see
//! [`crate::endpoints::dns_policies::DnsPoliciesApi::get_profile_applications`]'s docs), passes
//! the cached network envelope as `parent=` (`+net`).

use serde_json::Value;

use super::Client;
use crate::cache::Bucket;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Gets the network's advanced content filter allow/block lists — returns the raw Eero API
    /// response.
    ///
    /// Ported from `get_advanced_content_filter` (`eero-api src/eero/client.py:2430-2437`).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_advanced_content_filter(
        &self,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        self.api
            .dns_policies()
            .get_advanced_content_filter(&network_id, parent.as_ref())
            .await
    }

    /// Adds (or removes) a domain from the network-wide allow list — returns the raw Eero API
    /// response.
    ///
    /// Ported from `allow_domain` (`eero-api src/eero/client.py:2437-2464`). On success,
    /// invalidates `network[{nid}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn allow_domain(
        &self,
        domain: &str,
        add_cname: Option<bool>,
        reason_to_allow: Option<i64>,
        is_delete: Option<bool>,
        keep_profiles: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .allow_domain(
                &network_id,
                domain,
                add_cname,
                reason_to_allow,
                is_delete,
                keep_profiles,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        Ok(response)
    }

    /// Allows a list of CNAME domains network-wide — returns the raw Eero API response.
    ///
    /// Ported from `allow_cnames` (`eero-api src/eero/client.py:2464-2475`). On success,
    /// invalidates `network[{nid}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn allow_cnames(
        &self,
        domains: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .allow_cnames(&network_id, domains, parent.as_ref())
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        Ok(response)
    }

    /// Adds (or removes) a domain from the network-wide block list — returns the raw Eero API
    /// response.
    ///
    /// Ported from `block_domain` (`eero-api src/eero/client.py:2475-2495`). On success,
    /// invalidates `network[{nid}]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn block_domain(
        &self,
        domain: &str,
        is_delete: Option<bool>,
        keep_profiles: Option<&[&str]>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .block_domain(
                &network_id,
                domain,
                is_delete,
                keep_profiles,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        Ok(response)
    }

    /// Adds (or removes) a domain from one or more profiles' allow lists — returns the raw Eero
    /// API response.
    ///
    /// Ported from `allow_domain_for_profiles` (`eero-api src/eero/client.py:2495-2521`). On
    /// success, invalidates `profiles[{nid}_profiles]` — **not** `network[{nid}]`, despite `+net`
    /// parent passing (`client.md` §4 flags this invalidation-target/parent-source mismatch
    /// explicitly).
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    #[allow(clippy::too_many_arguments)] // mirrors client.py:2495-2505's own signature
    pub async fn allow_domain_for_profiles(
        &self,
        domain: &str,
        profiles: &[&str],
        override_: Option<bool>,
        add_cname: Option<bool>,
        reason_to_allow: Option<i64>,
        is_delete: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .allow_domain_for_profiles(
                &network_id,
                domain,
                profiles,
                override_,
                add_cname,
                reason_to_allow,
                is_delete,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Allows a list of CNAME domains for one or more profiles — returns the raw Eero API
    /// response.
    ///
    /// Ported from `allow_cnames_for_profiles` (`eero-api src/eero/client.py:2521-2532`). On
    /// success, invalidates `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn allow_cnames_for_profiles(
        &self,
        domains: &[&str],
        profiles: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .allow_cnames_for_profiles(&network_id, domains, profiles, parent.as_ref())
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Adds (or removes) a domain from one or more profiles' block lists — returns the raw Eero
    /// API response.
    ///
    /// Ported from `block_domain_for_profiles` (`eero-api src/eero/client.py:2532-2554`). On
    /// success, invalidates `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn block_domain_for_profiles(
        &self,
        domain: &str,
        profiles: &[&str],
        is_delete: Option<bool>,
        override_: Option<bool>,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent(network_id.as_str());
        let response = self
            .api
            .dns_policies()
            .block_domain_for_profiles(
                &network_id,
                domain,
                profiles,
                is_delete,
                override_,
                parent.as_ref(),
            )
            .await?;
        self.invalidate_profiles_list_cache(network_id.as_str());
        Ok(response)
    }

    /// Gets the applications a profile can block, and which are blocked — returns the raw Eero
    /// API response.
    ///
    /// Ported from `get_dns_policy_applications` (`eero-api src/eero/client.py:2554-2561`) — the
    /// `EeroClient` wrapper name for the domain method
    /// `DnsPoliciesAPI.get_profile_applications` (wiki `API-Reference.md:300`). No `parent=`: the
    /// domain method itself accepts none.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`].
    pub async fn get_dns_policy_applications(
        &self,
        profile_id: &str,
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        self.api
            .dns_policies()
            .get_profile_applications(&network_id, profile_id)
            .await
    }

    /// Sets the applications blocked for a profile — returns the raw Eero API response.
    ///
    /// Ported from `set_profile_blocked_applications` (`eero-api src/eero/client.py:2561-2574`).
    /// No `parent=`. On success, invalidates `profiles[{nid}_{pid}]` and
    /// `profiles[{nid}_profiles]`.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_profile_blocked_applications(
        &self,
        profile_id: &str,
        applications: &[&str],
        network_id: Option<&str>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let response = self
            .api
            .dns_policies()
            .set_profile_blocked_applications(&network_id, profile_id, applications)
            .await?;
        self.invalidate_profile_cache(network_id.as_str(), profile_id);
        Ok(response)
    }

    /// Switches ad blocking on or off for the network or profiles — returns the raw Eero API
    /// response.
    ///
    /// Ported from `set_network_ad_block()` (`eero-api src/eero/client.py:3142-3193` at
    /// `v8.4.1`). `profiles` **replaces** the previous profile list; `Some(&[])` with `enabled:
    /// true` means network-wide. Entries may be ids, paths or URLs (see
    /// [`crate::endpoints::dns_policies::DnsPoliciesApi::set_network_ad_block`]). Pass `None` for
    /// the network-wide switch.
    ///
    /// **Unverified write, premium feature** (`Error::PremiumRequired` otherwise).
    /// `capable: true` in the network's `premium_dns` block alone does not mean the DNS-policy
    /// subsystem is enabled. Read [`Client::get_network`] first and only write when the stored
    /// state differs; never retry a failed write in a loop.
    ///
    /// `auto_discover = false`. `parent` is the network's own envelope; when `None`, the cached
    /// network envelope is used if one is fresh (`_network_parent_kwargs_or`), and its own `url`
    /// is the base of the endpoint. On success, invalidates `network[{nid}]`, and — when
    /// `profiles` is `Some`, even if empty — every cached profile of the network (single
    /// profiles and the list); other networks' profiles survive. A premium (or any other) error
    /// leaves the cache untouched.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_network_ad_block(
        &self,
        enabled: bool,
        profiles: Option<&[&str]>,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        let response = self
            .api
            .dns_policies()
            .set_network_ad_block(&network_id, enabled, profiles, parent.as_ref())
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        if profiles.is_some() {
            self.cache
                .invalidate_bucket(Bucket::Profiles, network_id.as_str());
        }
        Ok(response)
    }

    /// Switches malware blocking on or off for the network — returns the raw Eero API response.
    ///
    /// Ported from `set_network_block_malware()` (`eero-api src/eero/client.py:3195-3229` at
    /// `v8.4.1`). POSTs `{"block_malware": enabled}` to the network's `dns_policies/network`
    /// resource; the body key is validated client-side.
    ///
    /// **Unverified write, premium feature** (`Error::PremiumRequired` otherwise); the same
    /// read-compare-skip discipline as [`Client::set_network_ad_block`] applies.
    ///
    /// `auto_discover = false`; `parent` as for [`Client::set_network_ad_block`]. On success,
    /// invalidates `network[{nid}]` only — profile caches stay. A premium (or any other) error
    /// leaves the cache untouched.
    ///
    /// # Errors
    ///
    /// See [`Client::get_diagnostics`]. The cache is left untouched on any `Err`.
    pub async fn set_network_block_malware(
        &self,
        enabled: bool,
        network_id: Option<&str>,
        parent: Option<&Value>,
    ) -> Result<Envelope, Error> {
        let network_id = self.ensure_network_id(network_id, false).await?;
        let parent = self.network_parent_or(network_id.as_str(), parent);
        let response = self
            .api
            .dns_policies()
            .set_network_block_malware(&network_id, enabled, parent.as_ref())
            .await?;
        self.invalidate_network_cache(network_id.as_str());
        Ok(response)
    }
}
