# PARITY — rusteero vs eero-api

Per-method checklist against `fulviofreitas/eero-api` at commit `e7bcfd9` (2026-09-10). Completion criterion: every row is **ported**, **dropped** (with reason), or **changed** (with reason), and every ported row has a wiremock test pinning verb, path and body.

Status values: `planned` → `ported` (with test) · `changed` · `renamed` · `identical` · `dropped`.

**Summary:** 127 rows — changed: 9, dropped: 7, identical: 2, ported: 107, renamed: 2. **All five implementation phases are complete.** Every row is ported, changed with a reason, dropped with a reason, identical or renamed; no row remains planned. Remaining work is phase 6: docs, live validation against a real account, and release.

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
| EeroClient (client-only logic) | `_ensure_network_id` | `resolve_network_id (private)` | — | `local` | ported | `tests/client.rs` |  |
| EeroClient (client-only logic) | `set_preferred_network / preferred_network_id` | `same` | — | `local` | identical | — |  |
| EeroClient (client-only logic) | `get_account` | `get_account` | GET | `2.2/account` | ported | `tests/client.rs` | cached |
| EeroClient (client-only logic) | `get_networks (+ /account fallback, preferred side-effect)` | `get_networks` | GET | `2.2/networks` | ported | `tests/client.rs` | cached |
| EeroClient (client-only logic) | `get_device_priority` | `(none)` | — | `—` | dropped | — | server no-op, eero-api #111 |
| EeroClient (client-only logic) | `set_device_priority` | `(none)` | — | `—` | dropped | — | server no-op, eero-api #111 |
| EeroClient (client-only logic) | `get_activity* (5)` | `(none)` | — | `—` | dropped | — | endpoints 404, eero-api #107 |
| EeroClient (client-only logic) | `local {meta:{code:400}} for empty setters (4)` | `Err(Error::Validation)` | — | `—` | changed | — | Result already expresses it |
| ac_compat | `get_ac_compat` | `get_ac_compat` | GET | `2.2/networks/{nid}/ac_compat` | ported | `tests/endpoints_*` |  |
| backup | `get_backup_network` | `get_backup_network` | GET | `2.2/networks/{nid}/backup` | ported | `tests/endpoints_*` |  |
| backup | `get_backup_status` | `get_backup_status` | GET | `2.2/networks/{nid}/backup/status` | ported | `tests/endpoints_*` |  |
| backup | `set_backup_network` | `set_backup_network` | PUT | `2.2/networks/{nid}/backup` | ported | `tests/endpoints_*` |  |
| backup | `configure_backup_network` | `configure_backup_network` | PUT | `2.2/networks/{nid}/backup` | ported | `tests/endpoints_*` | empty → Validation |
| blacklist | `get_blacklist` | `get_blacklist` | GET | `2.2/networks/{nid}/blacklist` | ported | `tests/endpoints_*` |  |
| blacklist | `add_to_blacklist` | `add_to_blacklist` | POST | `2.2/networks/{nid}/blacklist` | ported | `tests/endpoints_*` |  |
| blacklist | `remove_from_blacklist` | `remove_from_blacklist` | DELETE | `2.2/networks/{nid}/blacklist/{mac_or_id}` | ported | `tests/endpoints_*` |  |
| burst_reporters | `get_burst_reporters` | `get_burst_reporters` | GET | `2.2/networks/{nid}/burst_reporters` | ported | `tests/endpoints_*` |  |
| burst_reporters | `create_burst_reporter` | `create_burst_reporter` | POST | `2.2/networks/{nid}/burst_reporters` | ported | `tests/endpoints_*` |  |
| data_usage | `get_data_usage` | `get_data_usage` | GET+body | `2.2/networks/{nid}/data_usage[/{resource}]` | ported | `tests/endpoints_*` | JSON body on GET |
| devices | `get_devices` | `get_devices` | GET | `2.2/networks/{nid}/devices` | ported | `tests/endpoints_*` |  |
| devices | `get_device` | `get_device` | GET | `2.2/networks/{nid}/devices/{did}` | ported | `tests/endpoints_*` |  |
| devices | `set_device_nickname` | `set_device_nickname` | PUT | `**2.3**/networks/{nid}/devices/{did}` | ported | `tests/endpoints_*` |  |
| devices | `pause_device` | `pause_device` | PUT | `**2.3**/networks/{nid}/devices/{did}` | ported | `tests/endpoints_*` |  |
| devices | `block_device` | `block_device` | GET+POST / DELETE | `2.2/networks/{nid}/blacklist[/{did}]` | ported | `tests/endpoints_*` | two round-trips on block |
| diagnostics | `get_diagnostics` | `get_diagnostics` | GET | `2.2/networks/{nid}/diagnostics` | ported | `tests/endpoints_*` |  |
| diagnostics | `run_diagnostics` | `run_diagnostics` | POST | `2.2/networks/{nid}/diagnostics` | ported | `tests/endpoints_*` |  |
| dns | `get_dns_settings` | `get_dns_settings` | GET | `2.2/networks/{nid}` | ported | `tests/endpoints_*` |  |
| dns | `set_dns_caching` | `set_dns_caching` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| dns | `set_custom_dns` | `set_custom_dns` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | ≤2 servers |
| dns | `clear_custom_dns` | `clear_custom_dns` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| dns | `set_dns_mode` | `set_dns_mode` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | invalid → Validation |
| dns | `set_ipv6_dns` | `set_ipv6_dns` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| eeros | `get_eeros` | `get_eeros` | GET | `2.2/networks/{nid}/eeros` | ported | `tests/endpoints_*` |  |
| eeros | `get_eero` | `get_eero` | GET | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` |  |
| eeros | `reboot_eero` | `reboot_eero` | POST | `2.2/eeros/{eid}/reboot` | ported | `tests/endpoints_*` |  |
| eeros | `get_led_status` | `get_led_status` | GET | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` |  |
| eeros | `set_led` | `set_led` | PUT | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` |  |
| eeros | `set_led_brightness` | `set_led_brightness` | PUT | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` | clamp 0–100 |
| eeros | `get_nightlight` | `get_nightlight` | GET | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` |  |
| eeros | `set_nightlight` | `set_nightlight` | PUT | `2.2/eeros/{eid}` | ported | `tests/endpoints_*` | empty → Validation |
| eeros | `set_nightlight_brightness` | `set_nightlight_brightness` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| eeros | `set_nightlight_schedule` | `set_nightlight_schedule` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| forwards | `get_forwards` | `get_forwards` | GET | `2.2/networks/{nid}/forwards` | ported | `tests/endpoints_*` |  |
| forwards | `create_forward` | `create_forward` | POST | `2.2/networks/{nid}/forwards` | ported | `tests/endpoints_*` |  |
| forwards | `delete_forward` | `delete_forward` | DELETE | `2.2/networks/{nid}/forwards/{fid}` | ported | `tests/endpoints_*` |  |
| insights | `get_insights` | `get_insights` | GET | `2.2/networks/{nid}/insights?start&end&cadence&insight_type` | ported | `tests/endpoints_*` |  |
| insights | `run_insights` | `run_insights` | POST | `2.2/networks/{nid}/insights` | ported | `tests/endpoints_*` |  |
| networks | `get_networks` | `get_networks` | GET | `2.2/networks` | ported | `tests/endpoints_*` |  |
| networks | `get_network` | `get_network` | GET | `2.2/networks/{nid}` | ported | `tests/endpoints_*` |  |
| networks | `set_guest_network` | `set_guest_network` | PUT | `2.2/networks/{nid}/guestnetwork` | ported | `tests/endpoints_*` |  |
| networks | `run_speed_test` | `run_speed_test` | POST | `2.2/networks/{nid}/speedtest` | ported | `tests/endpoints_*` |  |
| networks | `reboot_network` | `reboot_network` | POST | `2.2/networks/{nid}/reboot` | ported | `tests/endpoints_*` |  |
| networks | `get_premium_status` | `get_premium_status` | GET | `2.2/networks/{nid}` | ported | `tests/endpoints_*` |  |
| networks | `set_network_name` | `set_network_name` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| ouicheck | `get_ouicheck` | `get_ouicheck` | GET | `2.2/networks/{nid}/ouicheck` | ported | `tests/endpoints_*` |  |
| ouicheck | `run_ouicheck` | `run_ouicheck` | POST | `2.2/networks/{nid}/ouicheck` | ported | `tests/endpoints_*` |  |
| password | `get_password` | `get_password` | GET | `2.2/networks/{nid}/password` | ported | `tests/endpoints_*` | never logged |
| profiles | `get_profiles` | `get_profiles` | GET | `2.2/networks/{nid}/profiles` | ported | `tests/endpoints_*` |  |
| profiles | `get_profile` | `get_profile` | GET | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `pause_profile` | `pause_profile` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `get_profile_devices` | `get_profile_devices` | GET | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `set_profile_devices` | `set_profile_devices` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `update_profile_content_filter` | `update_profile_content_filter` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` | key whitelist |
| profiles | `update_profile_block_list` | `update_profile_block_list` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `get_blocked_applications` | `get_blocked_applications` | GET | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `set_blocked_applications` | `set_blocked_applications` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `create_profile` | `create_profile` | POST | `2.2/networks/{nid}/profiles` | ported | `tests/endpoints_*` |  |
| profiles | `rename_profile` | `rename_profile` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| profiles | `delete_profile` | `delete_profile` | DELETE | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| reservations | `get_reservations` | `get_reservations` | GET | `2.2/networks/{nid}/reservations` | ported | `tests/endpoints_*` |  |
| reservations | `create_reservation` | `create_reservation` | POST | `2.2/networks/{nid}/reservations` | ported | `tests/endpoints_*` |  |
| reservations | `update_reservation` | `update_reservation` | PUT | `2.2/networks/{nid}/reservations/{rid}` | ported | `tests/endpoints_*` |  |
| reservations | `delete_reservation` | `delete_reservation` | DELETE | `2.2/networks/{nid}/reservations/{rid}` | ported | `tests/endpoints_*` |  |
| routing | `get_routing` | `get_routing` | GET | `2.2/networks/{nid}/routing` | ported | `tests/endpoints_*` |  |
| schedule | `get_profile_schedule` | `get_profile_schedule` | GET | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| schedule | `set_profile_schedule` | `set_profile_schedule` | PUT | `2.2/networks/{nid}/profiles/{pid}` | ported | `tests/endpoints_*` |  |
| schedule | `clear_profile_schedule` | `clear_profile_schedule` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| schedule | `enable_bedtime` | `enable_bedtime` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| schedule | `set_weekday_bedtime` | `set_weekday_bedtime` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| schedule | `set_weekend_bedtime` | `set_weekend_bedtime` | PUT | `delegates` | ported | `tests/endpoints_*` |  |
| security | `get_security_settings` | `get_security_settings` | GET | `2.2/networks/{nid}` | ported | `tests/endpoints_*` |  |
| security | `set_wpa3` | `set_wpa3` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| security | `set_band_steering` | `set_band_steering` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| security | `set_upnp` | `set_upnp` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| security | `set_ipv6` | `set_ipv6` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| security | `set_thread` | `set_thread` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| security | `configure_security` | `configure_security` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | empty → Validation |
| settings | `get_settings` | `get_settings` | GET | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` |  |
| sqm | `get_sqm_settings` | `get_sqm_settings` | GET | `2.2/networks/{nid}` | ported | `tests/endpoints_*` |  |
| sqm | `set_sqm_enabled` | `set_sqm_enabled` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | flat bool |
| sqm | `set_sqm_bandwidth` | `set_sqm_bandwidth` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | shape unverified upstream (sqm.py TODOs at :128,:173,:197) |
| sqm | `configure_sqm` | `configure_sqm` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | shape unverified upstream (sqm.py TODOs at :128,:173,:197) |
| sqm | `set_sqm_auto` | `set_sqm_auto` | PUT | `2.2/networks/{nid}/settings` | ported | `tests/endpoints_*` | shape unverified upstream (sqm.py TODOs at :128,:173,:197) |
| support | `get_support` | `get_support` | GET | `2.2/networks/{nid}/support` | ported | `tests/endpoints_*` |  |
| support | `request_support` | `request_support` | POST | `2.2/networks/{nid}/support` | ported | `tests/endpoints_*` |  |
| thread | `get_thread` | `get_thread` | GET | `2.2/networks/{nid}/thread` | ported | `tests/endpoints_*` |  |
| transfer | `get_transfer_stats` | `get_transfer_stats` | GET | `2.2/networks/{nid}/transfer | …/devices/{did}/transfer` | ported | `tests/endpoints_*` |  |
| updates | `get_updates` | `get_updates` | GET | `2.2/networks/{nid}/updates` | ported | `tests/endpoints_*` |  |
| activity (dropped module) | `get_activity, get_activity_clients, get_activity_for_device, get_activity_history, get_activity_categories` | `(none)` | GET | `2.2/networks/{nid}/activity*` | dropped | — | 404 on 2.2 and 2.3 upstream (eero-api #107) |
| misc | `id_from_url` | `id_from_url` | — | `local` | ported | `tests/endpoints_*` |  |
| misc | `redact_sensitive` | `redact::redact_sensitive` | — | `local` | ported | `tests/endpoints_*` | Value only |
| misc | `get_secure_logger / SecureLoggerAdapter` | `(none)` | — | `—` | dropped | — | Python-logging specific; tracing + SecretString |
| misc | `const.py enums (EeroDeviceType, …)` | `(none)` | — | `—` | dropped | — | unused in Python |

## Live validation (2026-09-14)

A read-only sweep against a real account (4 eeros, 135 devices, 10 profiles) with an
`eeroctl`-issued session token. Every path below was additionally re-probed with raw `curl` to
separate "this port builds the wrong URL" from "the endpoint is gone or gated" — the two agree in
every case, so **no route constant is wrong**. Reproduce with
`cargo test --test live -- --ignored --exact live_read_only_endpoint_sweep`.

**18 of 25 read-only endpoints answered `200`**: account, networks, network, eeros, devices,
profiles, dns_settings, security_settings, sqm_settings, blacklist, reservations, forwards,
routing, thread, updates, ac_compat, support, diagnostics, premium_status.

Confirmed by observation, not just by reading Python:

- `get_dns_settings`, `get_security_settings`, `get_sqm_settings` and `get_premium_status` really
  do return the whole network object, byte-identical to `get_network`. Modelling them as aliases
  of one route is correct.
- **`GET /networks` returned an empty list on this account**, so the `/account` fallback is what
  actually resolved the network id. That fallback is load-bearing in the real world, not a
  vestigial branch — worth knowing before anyone "simplifies" it away.
- The account's stored `cookies.json` has `"refresh_token": null`, matching §1.3's finding that a
  refresh token is never issued at login.

Seven endpoints did not answer. These are observations from **one** account on one day, so a 404
here may mean "feature not enabled for this account" rather than "removed upstream"; they are
recorded, not acted on, and no row below was changed to `dropped` on this evidence alone.

| Endpoint | Live result | Note |
|---|---|---|
| `GET networks/{nid}/settings` | 404 | `get_settings`. The PUT to the same path is how every settings writer works, so the resource is not simply absent — the GET appears unsupported. |
| `GET networks/{nid}/password` | 404 | `get_password`. |
| `GET networks/{nid}/transfer` | **403** | `get_transfer_stats`. Forbidden rather than missing — looks permission- or tier-gated. |
| `GET networks/{nid}/backup` | 404 | `get_backup_network`; this account has no backup internet configured. |
| `GET networks/{nid}/backup/status` | 404 | `get_backup_status`. |
| `GET networks/{nid}/ouicheck` | 404 | `get_ouicheck`. |
| `GET networks/{nid}/burst_reporters` | 404 | `get_burst_reporters`. |


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
