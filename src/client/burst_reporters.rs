//! `Client` methods for the `BurstReportersAPI` domain.
//!
//! **Deliberately empty.** `client.py` has no `EeroClient` wrapper for either
//! `BurstReportersAPI` method at v8.0.4 or at v6.2.0 — `get_burst_reporters` was removed
//! upstream entirely (`.claude/tasks/briefs/v8/client.md` §5) and `create_burst_reporter` was
//! never wrapped in the first place (`.claude/tasks/briefs/v8/client.md`'s `burst_reporters`
//! section: "No `EeroClient` wrapper exists at v8.0.4 or v6.2.0 ... `create_burst_reporter`
//! should move to `client.api().burst_reporters()`-only, matching Python's domain-API-only
//! access").
//!
//! `rusteero`'s pre-v8.0.4 `Client::get_burst_reporters`/`Client::create_burst_reporter` (the
//! shape this file used to carry) are removed for the same reason: `get_burst_reporters` no
//! longer has a domain method to call at all, and `create_burst_reporter` is reachable via
//! `client.api().burst_reporters().create_burst_reporter(...)`, matching Python's own
//! domain-API-only access — see `.claude/tasks/briefs/v8/g7-backup-members.md`.
