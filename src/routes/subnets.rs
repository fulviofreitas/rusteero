//! `subnets` routes (`SubnetsAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/subnets.py` (v8.0.4). No `Resource`/`Nested` constants yet
//! — those are added by the domain port (see `.claude/tasks/briefs/v8/g6-forwards-dhcp.md` for the method
//! table this domain is scoped by), one per wire endpoint, exactly like every other file in
//! [`crate::routes`].
