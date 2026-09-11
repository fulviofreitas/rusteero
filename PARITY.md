# PARITY — rusteero vs eero-api

Per-method checklist against `fulviofreitas/eero-api` at commit `e7bcfd9` (2026-09-10). Completion criterion: every row is **ported**, **dropped** (with reason), or **changed** (with reason), and every ported row has a wiremock test pinning verb, path and body.

Status values: `planned` → `ported` (with test) · `changed` · `renamed` · `identical` · `dropped`.

**Summary:** 127 rows — changed: 9, dropped: 7, identical: 2, planned: 95, ported: 12, renamed: 2. Phases 1 and 2 (transport, errors, envelope, routes, auth, credential storage) are complete; the remaining `planned` rows are the endpoint modules delivered in phases 3 and 5.

| Module | Python | Rust | Verb | Path | Status | Test | Note |
|---|---|---|---|---|---|---|---|
| auth (AuthAPI) | `is_authenticated` | `is_authenticated` | — | `local` | ported | `auth::tests` | local only: token set and now <= expiry |
| auth (AuthAPI) | `login` | `LoginFlow::start` | POST | `2.2/login` | ported | `tests/auth.rs` | separable from Client |
| auth (AuthAPI) | `verify` | `PendingLogin::verify` | POST | `2.2/login/verify` | ported | `tests/auth.rs` | **provisional** — both Set-Cookie branches implemented and tested; needs live confirmation (plan 7.2, D-16) |
| auth (AuthAPI) | `resend_verification_code` | `PendingLogin::resend` | POST | `2.2/login/resend` | ported | `tests/auth.rs` |  |
| auth (AuthAPI) | `logout` | `logout` | POST | `2.2/logout` | changed | `tests/auth.rs` | always clears local session and store, on every outcome; Python skips cleanup on 429/network error (auth.py:239-277) despite its own comment at :269 |
| auth (AuthAPI) | `refresh_session` | `refresh_session` | POST | `2.2/login/refresh → 2.2/account/refresh` | changed | `tests/transport.rs` | retry uses the REFRESHED token; Python re-passes the stale one and clobbers the fresh cookie (base.py:296-298), so its refresh cannot succeed. Route order and 404-fallthrough are as Python |
| auth (AuthAPI) | `ensure_authenticated` | `ensure_authenticated` | — | `local` | ported | `auth::tests` | local check; Python's refresh branch is unreachable, same observable behaviour |
| auth (AuthAPI) | `get_auth_token` | `Client::session` | — | `local` | renamed | — | returns Session (SecretString), not a bare string |
| auth (AuthAPI) | `clear_auth_data` | `clear_auth_data` | — | `local` | ported | `tests/auth.rs` | clears store too; honours StorageFailures |
| auth (AuthAPI) | `set_session_token` | `set_session_token / Session::from_token` | — | `local` | ported | `tests/auth.rs` | preserves an existing refresh token, as Python does |
| auth (AuthAPI) | `clear_session_token` | `clear_session_token` | — | `local` | ported | `tests/auth.rs` | leaves the refresh token in place, unlike clear_auth_data |
| EeroAPI | `EeroAPI(session, cookie_file, use_keyring)` | `EeroApi::new(transport)` | — | `—` | changed | — | storage moves to Client::builder().store() |
| EeroAPI | `__aenter__/__aexit__` | `(none)` | — | `—` | dropped | — | no context manager in Rust |
| EeroAPI | `is_authenticated / login / verify / logout` | `same on Client` | — | `—` | identical | — |  |
| EeroClient (client-only logic) | `EeroClient(session, cookie_file, use_keyring, cache_timeout)` | `Client::builder()` | — | `—` | renamed | — | builder |
| EeroClient (client-only logic) | `clear_cache` | `clear_cache` | — | `local` | changed | — | also clears timestamps |
| EeroClient (client-only logic) | `_ensure_network_id` | `resolve_network_id (private)` | — | `local` | planned | — |  |
| EeroClient (client-only logic) | `set_preferred_network / preferred_network_id` | `same` | — | `local` | identical | — |  |
| EeroClient (client-only logic) | `get_account` | `get_account` | GET | `2.2/account` | planned | — | cached |
| EeroClient (client-only logic) | `get_networks (+ /account fallback, preferred side-effect)` | `get_networks` | GET | `2.2/networks` | planned | — | cached |
| EeroClient (client-only logic) | `get_device_priority` | `(none)` | — | `—` | dropped | — | server no-op, eero-api #111 |
| EeroClient (client-only logic) | `set_device_priority` | `(none)` | — | `—` | dropped | — | server no-op, eero-api #111 |
| EeroClient (client-only logic) | `get_activity* (5)` | `(none)` | — | `—` | dropped | — | endpoints 404, eero-api #107 |
| EeroClient (client-only logic) | `local {meta:{code:400}} for empty setters (4)` | `Err(Error::Validation)` | — | `—` | changed | — | Result already expresses it |
| ac_compat | `get_ac_compat` | `get_ac_compat` | GET | `2.2/networks/{nid}/ac_compat` | planned | — |  |
| backup | `get_backup_network` | `get_backup_network` | GET | `2.2/networks/{nid}/backup` | planned | — |  |
| backup | `get_backup_status` | `get_backup_status` | GET | `2.2/networks/{nid}/backup/status` | planned | — |  |
| backup | `set_backup_network` | `set_backup_network` | PUT | `2.2/networks/{nid}/backup` | planned | — |  |
| backup | `configure_backup_network` | `configure_backup_network` | PUT | `2.2/networks/{nid}/backup` | planned | — | empty → Validation |
| blacklist | `get_blacklist` | `get_blacklist` | GET | `2.2/networks/{nid}/blacklist` | planned | — |  |
| blacklist | `add_to_blacklist` | `add_to_blacklist` | POST | `2.2/networks/{nid}/blacklist` | planned | — |  |
| blacklist | `remove_from_blacklist` | `remove_from_blacklist` | DELETE | `2.2/networks/{nid}/blacklist/{mac_or_id}` | planned | — |  |
| burst_reporters | `get_burst_reporters` | `get_burst_reporters` | GET | `2.2/networks/{nid}/burst_reporters` | planned | — |  |
| burst_reporters | `create_burst_reporter` | `create_burst_reporter` | POST | `2.2/networks/{nid}/burst_reporters` | planned | — |  |
| data_usage | `get_data_usage` | `get_data_usage` | GET+body | `2.2/networks/{nid}/data_usage[/{resource}]` | planned | — | JSON body on GET |
| devices | `get_devices` | `get_devices` | GET | `2.2/networks/{nid}/devices` | planned | — |  |
| devices | `get_device` | `get_device` | GET | `2.2/networks/{nid}/devices/{did}` | planned | — |  |
| devices | `set_device_nickname` | `set_device_nickname` | PUT | `**2.3**/networks/{nid}/devices/{did}` | planned | — |  |
| devices | `pause_device` | `pause_device` | PUT | `**2.3**/networks/{nid}/devices/{did}` | planned | — |  |
| devices | `block_device` | `block_device` | GET+POST / DELETE | `2.2/networks/{nid}/blacklist[/{did}]` | planned | — | two round-trips on block |
| diagnostics | `get_diagnostics` | `get_diagnostics` | GET | `2.2/networks/{nid}/diagnostics` | planned | — |  |
| diagnostics | `run_diagnostics` | `run_diagnostics` | POST | `2.2/networks/{nid}/diagnostics` | planned | — |  |
| dns | `get_dns_settings` | `get_dns_settings` | GET | `2.2/networks/{nid}` | planned | — |  |
| dns | `set_dns_caching` | `set_dns_caching` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| dns | `set_custom_dns` | `set_custom_dns` | PUT | `2.2/networks/{nid}/settings` | planned | — | ≤2 servers |
| dns | `clear_custom_dns` | `clear_custom_dns` | PUT | `delegates` | planned | — |  |
| dns | `set_dns_mode` | `set_dns_mode` | PUT | `2.2/networks/{nid}/settings` | planned | — | invalid → Validation |
| dns | `set_ipv6_dns` | `set_ipv6_dns` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| eeros | `get_eeros` | `get_eeros` | GET | `2.2/networks/{nid}/eeros` | planned | — |  |
| eeros | `get_eero` | `get_eero` | GET | `2.2/eeros/{eid}` | planned | — |  |
| eeros | `reboot_eero` | `reboot_eero` | POST | `2.2/eeros/{eid}/reboot` | planned | — |  |
| eeros | `get_led_status` | `get_led_status` | GET | `2.2/eeros/{eid}` | planned | — |  |
| eeros | `set_led` | `set_led` | PUT | `2.2/eeros/{eid}` | planned | — |  |
| eeros | `set_led_brightness` | `set_led_brightness` | PUT | `2.2/eeros/{eid}` | planned | — | clamp 0–100 |
| eeros | `get_nightlight` | `get_nightlight` | GET | `2.2/eeros/{eid}` | planned | — |  |
| eeros | `set_nightlight` | `set_nightlight` | PUT | `2.2/eeros/{eid}` | planned | — | empty → Validation |
| eeros | `set_nightlight_brightness` | `set_nightlight_brightness` | PUT | `delegates` | planned | — |  |
| eeros | `set_nightlight_schedule` | `set_nightlight_schedule` | PUT | `delegates` | planned | — |  |
| forwards | `get_forwards` | `get_forwards` | GET | `2.2/networks/{nid}/forwards` | planned | — |  |
| forwards | `create_forward` | `create_forward` | POST | `2.2/networks/{nid}/forwards` | planned | — |  |
| forwards | `delete_forward` | `delete_forward` | DELETE | `2.2/networks/{nid}/forwards/{fid}` | planned | — |  |
| insights | `get_insights` | `get_insights` | GET | `2.2/networks/{nid}/insights?start&end&cadence&insight_type` | planned | — |  |
| insights | `run_insights` | `run_insights` | POST | `2.2/networks/{nid}/insights` | planned | — |  |
| networks | `get_networks` | `get_networks` | GET | `2.2/networks` | planned | — |  |
| networks | `get_network` | `get_network` | GET | `2.2/networks/{nid}` | planned | — |  |
| networks | `set_guest_network` | `set_guest_network` | PUT | `2.2/networks/{nid}/guestnetwork` | planned | — |  |
| networks | `run_speed_test` | `run_speed_test` | POST | `2.2/networks/{nid}/speedtest` | planned | — |  |
| networks | `reboot_network` | `reboot_network` | POST | `2.2/networks/{nid}/reboot` | planned | — |  |
| networks | `get_premium_status` | `get_premium_status` | GET | `2.2/networks/{nid}` | planned | — |  |
| networks | `set_network_name` | `set_network_name` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| ouicheck | `get_ouicheck` | `get_ouicheck` | GET | `2.2/networks/{nid}/ouicheck` | planned | — |  |
| ouicheck | `run_ouicheck` | `run_ouicheck` | POST | `2.2/networks/{nid}/ouicheck` | planned | — |  |
| password | `get_password` | `get_password` | GET | `2.2/networks/{nid}/password` | planned | — | never logged |
| profiles | `get_profiles` | `get_profiles` | GET | `2.2/networks/{nid}/profiles` | planned | — |  |
| profiles | `get_profile` | `get_profile` | GET | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `pause_profile` | `pause_profile` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `get_profile_devices` | `get_profile_devices` | GET | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `set_profile_devices` | `set_profile_devices` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `update_profile_content_filter` | `update_profile_content_filter` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — | key whitelist |
| profiles | `update_profile_block_list` | `update_profile_block_list` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `get_blocked_applications` | `get_blocked_applications` | GET | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `set_blocked_applications` | `set_blocked_applications` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `create_profile` | `create_profile` | POST | `2.2/networks/{nid}/profiles` | planned | — |  |
| profiles | `rename_profile` | `rename_profile` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| profiles | `delete_profile` | `delete_profile` | DELETE | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| reservations | `get_reservations` | `get_reservations` | GET | `2.2/networks/{nid}/reservations` | planned | — |  |
| reservations | `create_reservation` | `create_reservation` | POST | `2.2/networks/{nid}/reservations` | planned | — |  |
| reservations | `update_reservation` | `update_reservation` | PUT | `2.2/networks/{nid}/reservations/{rid}` | planned | — |  |
| reservations | `delete_reservation` | `delete_reservation` | DELETE | `2.2/networks/{nid}/reservations/{rid}` | planned | — |  |
| routing | `get_routing` | `get_routing` | GET | `2.2/networks/{nid}/routing` | planned | — |  |
| schedule | `get_profile_schedule` | `get_profile_schedule` | GET | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| schedule | `set_profile_schedule` | `set_profile_schedule` | PUT | `2.2/networks/{nid}/profiles/{pid}` | planned | — |  |
| schedule | `clear_profile_schedule` | `clear_profile_schedule` | PUT | `delegates` | planned | — |  |
| schedule | `enable_bedtime` | `enable_bedtime` | PUT | `delegates` | planned | — |  |
| schedule | `set_weekday_bedtime` | `set_weekday_bedtime` | PUT | `delegates` | planned | — |  |
| schedule | `set_weekend_bedtime` | `set_weekend_bedtime` | PUT | `delegates` | planned | — |  |
| security | `get_security_settings` | `get_security_settings` | GET | `2.2/networks/{nid}` | planned | — |  |
| security | `set_wpa3` | `set_wpa3` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| security | `set_band_steering` | `set_band_steering` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| security | `set_upnp` | `set_upnp` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| security | `set_ipv6` | `set_ipv6` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| security | `set_thread` | `set_thread` | PUT | `2.2/networks/{nid}/settings` | planned | — |  |
| security | `configure_security` | `configure_security` | PUT | `2.2/networks/{nid}/settings` | planned | — | empty → Validation |
| settings | `get_settings` | `get_settings` | GET | `2.2/networks/{nid}/settings` | planned | — |  |
| sqm | `get_sqm_settings` | `get_sqm_settings` | GET | `2.2/networks/{nid}` | planned | — |  |
| sqm | `set_sqm_enabled` | `set_sqm_enabled` | PUT | `2.2/networks/{nid}/settings` | planned | — | flat bool |
| sqm | `set_sqm_bandwidth` | `set_sqm_bandwidth` | PUT | `2.2/networks/{nid}/settings` | planned | — | shape unverified upstream |
| sqm | `configure_sqm` | `configure_sqm` | PUT | `2.2/networks/{nid}/settings` | planned | — | shape unverified upstream |
| sqm | `set_sqm_auto` | `set_sqm_auto` | PUT | `2.2/networks/{nid}/settings` | planned | — | shape unverified upstream |
| support | `get_support` | `get_support` | GET | `2.2/networks/{nid}/support` | planned | — |  |
| support | `request_support` | `request_support` | POST | `2.2/networks/{nid}/support` | planned | — |  |
| thread | `get_thread` | `get_thread` | GET | `2.2/networks/{nid}/thread` | planned | — |  |
| transfer | `get_transfer_stats` | `get_transfer_stats` | GET | `2.2/networks/{nid}/transfer | …/devices/{did}/transfer` | planned | — |  |
| updates | `get_updates` | `get_updates` | GET | `2.2/networks/{nid}/updates` | planned | — |  |
| activity (dropped module) | `get_activity, get_activity_clients, get_activity_for_device, get_activity_history, get_activity_categories` | `(none)` | GET | `2.2/networks/{nid}/activity*` | dropped | — | 404 on 2.2 and 2.3 upstream (eero-api #107) |
| misc | `id_from_url` | `id_from_url` | — | `local` | planned | — |  |
| misc | `redact_sensitive` | `redact::redact_sensitive` | — | `local` | planned | — | Value only |
| misc | `get_secure_logger / SecureLoggerAdapter` | `(none)` | — | `—` | dropped | — | Python-logging specific; tracing + SecretString |
| misc | `const.py enums (EeroDeviceType, …)` | `(none)` | — | `—` | dropped | — | unused in Python |

## Credential storage (phase 2, `api/auth_storage.py`)

| Python | Rust | Status | Test | Note |
|---|---|---|---|---|
| `AuthCredentials` | `Session` + `StoredSession` | ported | `auth::session::tests`, `tests/storage.rs` | same JSON keys, naive 19-char ISO 8601 expiry, legacy `user_token` accepted on read (D-5) |
| `CredentialStorage` (ABC) | `CredentialStore` trait | ported | `storage::*` | sync on purpose; async callers use the `spawn_blocking` adapters |
| `MemoryStorage` | `MemoryStore` | ported | `storage::memory::tests` | |
| `FileStorage` | `FileStore` | changed | `tests/storage.rs`, `storage::file::tests` | 0600 applied atomically at open; Python chmods after write (`auth_storage.py:215-224`), leaving a world-readable window. Unique temp name per write, temp removed on every error path, `clear()` also removes an orphaned temp |
| `KeyringStorage` | `KeyringStore` | changed | `storage::keyring::tests` | same service `eero-api` / account `auth-tokens` (D-5). Returns `StorageError` instead of swallowing every exception at DEBUG (`auth_storage.py:142-145,152-155`) |
| `ChainedStorage` | `ChainedStore` | changed | `tests/storage.rs`, `storage::chained::tests` | save fallback actually fires, because `KeyringStore::save` can return `Err`; Python's never can. A failed primary save clears the stale primary. `clear()` attempts both and reports `Err` if either fails, so a partial erase is never reported as success |
| `create_storage(use_keyring, cookie_file)` | `create_storage(&StorageConfig)` | ported | `storage::tests`, `tests/storage.rs` | same four-way matrix; degrades to file-then-memory without the `keyring` feature |
| swallowed storage exceptions | `StorageFailures {Warn, Fatal}` | changed | `tests/storage.rs`, `auth::tests` | D-13. Warn is the default. On `TransportBuilder` for now; `Client::builder()` forwards to it in phase 4 |
