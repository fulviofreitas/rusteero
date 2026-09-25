//! `Client` methods for the `DnsPoliciesAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/dns_policies.py` (v8.0.4) by way of `EeroClient`'s own
//! `dns_policies`-scoped wrappers in `client.py`. No methods yet — those are added by the domain
//! port (see `.claude/tasks/briefs/v8/g4-profiles.md` for the method table this domain is scoped by),
//! following the same `ensure_network_id`/cache-invalidation pattern every other file under
//! [`crate::client`] already uses.
