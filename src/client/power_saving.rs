//! `Client` methods for the `PowerSavingAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/power_saving.py` (v8.0.4) by way of `EeroClient`'s own
//! `power_saving`-scoped wrappers in `client.py`. No methods yet — those are added by the domain
//! port (see `.claude/tasks/briefs/v8/g2-eeros.md` for the method table this domain is scoped by),
//! following the same `ensure_network_id`/cache-invalidation pattern every other file under
//! [`crate::client`] already uses.
