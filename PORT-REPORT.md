# PORT-REPORT — rusteero on eero-api 8.0.4

Report of the orchestrated port of `fulviofreitas/eero-api` **v8.0.4** (commit `ac7358f`,
2026-09-24) into `rusteero`, run unattended on 2026-09-25/26 on branch
`feat/eero-api-8.0.4-parity`. It records the gap analysis, what was reused from the earlier
port, the plan, which agent did what and where the orchestrator had to intervene, the files
touched, the commands run, the CI verification and what is left outstanding. `PARITY.md` is
the per-method checklist; this file is the narrative.

## 1. Baselines

| | Version / commit | Notes |
|---|---|---|
| Newest `eero-api` | **v8.0.4**, tag on `ac7358f` (2026-09-24) | determined from the repo's tags and `CHANGELOG.md`; releases between the previous baseline and this one: 7.0.0 (DNS write path rewrite, breaking), 8.0.0 ("converge the SDK on what the eero cloud API actually accepts", breaking), 8.0.1, 8.0.2, 8.0.3, 8.0.4 |
| `rusteero` default branch before this work | v1.0.0 (`07e6dfd`) | `PARITY.md` declared parity with `eero-api` at commit `e7bcfd9` (2026-09-10) |
| What `e7bcfd9` actually is | `src/` identical to tag **v6.2.0** | `git diff --stat e7bcfd9 v6.2.0 -- src/eero` is empty; the prior port therefore targeted the v6.2.0 feature set |
| Prior session `215441bd-7a92-47f3-aff8-b1eee22ec97e` | produced everything on `master` up to v1.0.0 | no separate branch, no uncommitted work of value (one stale stash from 2026-09-10, `orchestrator: pre-identity-rewrite`, contains an older copy of `auth/flow.rs` and was left untouched) |

## 2. Gap analysis (v6.2.0 → v8.0.4), and what the prior session's work covered

Full per-method detail lives in the private briefs used during the run; the summary:

| Area | v8.0.4 change | Prior work (v6.2.0) | Verdict |
|---|---|---|---|
| Session transport | `X-User-Token` header is the credential, legacy `s=` cookie optional and per request; both withheld off the configured host and scheme; `Accept`, `User-Agent` (`eero/3.0 (iPhone; iOS 17.0)`), `X-Accept-Language` on every request; body encodings JSON / form / literal `""`; GET-only bounded retry | cookie-only credential, no header set, JSON-only bodies, reqwest default UA (decision D-7) | **reworked** |
| Auth handshake | `login`/`verify`/`logout` form-encoded; `resend` JSON `{}`; refresh is `POST /2.2/login/refresh` with `""` authenticated by the current token, coalesced, server token ignored, credentials retained only for verification / refresh signals; no client-side expiry; `clear_session_token == clear_auth_data` | JSON bodies, fabricated 30-day expiry, refresh-token model, `account/refresh` fallback, provisional `Set-Cookie` hedge (D-16) | **reworked**; D-16 closed (v8.0.4 never reads `Set-Cookie`) |
| Credential record | `{"session_id", "schema_version": 2}`, legacy records migrated on load with a read-back check; `ChainedStore` verifies primary writes and promotes with read-back | record with `refresh_token` / `session_expiry`, no migration event | **reworked**; the Rust-only `StorageError` / `StorageFailures` design was kept |
| Errors | closed `meta.error` catalogue, 401-first precedence, every error carries `envelope` + `error_code`, message is the catalogue string or `unrecognised error string`; `NotFound`, `AccessDenied`, `ClientBlocked`, `PremiumRequired`, `FeatureUnavailable`, `RateLimit` raised for real | body-derived, truncated, sanitised messages; 404 as `Api` | **reworked** |
| URL resolution | every method resolves through published `resources` links / the resource's own `url` when a `parent` envelope is supplied; ids are polymorphic (bare id, host-relative path, absolute API URL); nested-family checks | hand-rendered path templates, bare ids only | **added** (`links.rs`, `params.rs`, `Resource`/`Nested` route model) |
| Existing domains (23) | writes re-pointed to the forms the API declares (LED, guest network, passwords, reboot/speedtest `""` bodies, nightlight sub-resource, block/unblock split, data-usage query family, ouicheck params, DNS per-family model, SQM query param, thread writes, backup internet, schedules sub-resource, diagnostics body) | v6.2.0 shapes, several verified no-ops | **reworked** module by module |
| New domains (14) | `account`, `entitlements`, `permissions`, `notifications`, `dns_policies`, `members`, `dhcp`, `wpa3`, `power_saving`, `ddns`, `backup_access_points`, `subnets`, `wan`, `events` | absent | **added** |
| Removed surface | `activity`, `settings`, `password`, device priority, `set_ipv6_dns`, `run_insights`, `run_ouicheck`, `SecurityApi::set_thread`, the four SQM variants, the old backup methods, profile schedule/content-filter/block-list/app writes, client `reboot_network`, weekday/weekend bedtime wrappers, 3-arg `block_device` | present (except activity and device priority, already dropped) | **removed** |
| Client facade | `parent` passing from cached network / eero / device envelopes; `get_devices(thread, proxied_node)` bypasses the cache; `delete_reservation(delete_forwards)`; ~150 new or changed wrappers; cache buckets unchanged | eight cached getters and the invalidation rules already matched | **kept** cache, **added** parent helpers, **reworked** wrappers |
| Redaction | identifier-shaped patterns (`login`, `email`, `phone`, `sms`, `serial`, `mac`, `ssid`, …) and zero-visibility credential keys | credential patterns only, 4-char prefix everywhere | **reworked** |

Reused unchanged from the prior session: the crate skeleton, CI, release wiring, `cache.rs`
(its bucket keys, falsy rule and TTL semantics are byte-for-byte what v8.0.4 still uses), the
`Envelope` type, the `StorageError`/`StorageFailures` design, the file-store atomic write, the
wiremock harness, and the phase-structure of the tests.

## 3. Plan and execution

| Step | Agent(s) | Scope / definition of done | Commit(s) |
|---|---|---|---|
| Gap briefs | 9 × `python-pro` (read-only) | one brief per area (core, client, 7 endpoint groups), every row marked keep / rework / add / remove | private context repo |
| R — restructure | 2 × `rust-engineer` | `routes.rs`, `client.rs` and the test suites split one file per domain so later agents own disjoint files; test count unchanged (568/558) | `59a0a23`, `b5036aa` |
| A — core types | `rust-engineer` | constants, `errors.rs`, `Error` rework, `links.rs`, `params.rs`, `Resource`/`Nested`, redaction; crate green | `4e30fad` |
| B — transport/auth/storage | `rust-engineer` | v8 request pipeline, handshake, coalesced refresh, schema-2 storage; crate green; `transport-api.md` for phase G | `0685271`, `bad5a3c` |
| S — scaffold | `rust-engineer` | 14 empty domain modules wired into `EeroApi`, `Client` parent helpers, builder options; `settings`/`password` removed | `165643b` |
| G1–G7 — endpoints | 7 × `rust-engineer` in parallel, same tree, disjoint files | every method ported per `phase-g-rules.md`; then one integration agent removed the legacy route model and fixed shared files | `e2cc560` |
| Verification | 3 × `Explore` (read-only, method-by-method against Python) → `rust-engineer` ×3 | 30-item fix list (wire bugs, signatures, coverage) | `c7ee8d7`, `ef2c55d` |
| Security | `security-engineer` (read-only, table only) → `rust-engineer` | 2 medium + 6 low findings fixed with tests; `cargo deny` advisory (rustls) bumped | `e2413d5`, `ec1488e` |
| Docs | 3 × `general-purpose` | `PARITY.md` regenerated (267 rows), README + wiki rewritten against the code, private design docs updated | `5d2b247`, `88987ef` |

### Interventions (where an agent's claim did not survive verification)

1. **All seven phase-G agents were blocked by a phase-A defect** none of the earlier checks
   caught: `links::resource_url`'s bare-id branch resolved against the production host
   constant instead of the configured host, so every bare-id wiremock test silently sent its
   request to the real Eero API. The phase-A unit tests had not caught it because their host
   fixture equalled the production host. Fixed by the orchestrator (`src/links.rs`), the unit
   fixture changed to `http://mock.test:1234`, and the lesson recorded in the private rules.
2. **Phase B** wrapped rate-limit and (on verify) validation errors into authentication
   errors where Python propagates them; corrected by the orchestrator before commit.
3. **The integration agent was cut off by an API rate limit** near the end; the orchestrator
   checked the tree (it compiled, legacy model already gone) and continued with a gate run
   instead of re-running the agent.
4. **Read-only verification found 30 concrete discrepancies** after the groups had reported
   "done": among them `set_device_labels` PUT to the wrong path, `set_profile_devices` not
   invalidating the profiles list, `get_device_priority` auto-discovering, `get_insights`
   query order, `get_members` not passing its parent, two wrappers Python never had, Python
   quoting in validation messages, warn-before-resolution, and missing parent-preference and
   credential-matcher tests. Every item was assigned, fixed and re-verified by the gate.
5. **The first fix agent stopped at ~60 % of the list** and said so; the remainder was split
   into a source-side and a test-side agent that ran in parallel on disjoint files.
6. **The docs agent recorded the HTTP-client escape hatch as `http(Client)`** while the
   security fix was changing it to `http_builder(ClientBuilder)`; the orchestrator corrected
   the wiki, the technical reference and the testing rule afterwards.
7. **Disk**: stale test binaries had grown `target/` to 48 GB on a 100 GB volume;
   `cargo clean -p rusteero` reclaimed 40 GB twice during the run.

Orchestration rules that held: foreground agents only (background work does not survive a
turn boundary in this environment); one tree, strict file ownership; nobody but the
orchestrator commits; every "done" report was checked against `git diff` and a gate run.

## 4. Files touched

- `src/`: `consts.rs`, `errors.rs` (new), `error.rs`, `links.rs` (new), `params.rs` (new),
  `redact.rs`, `transport.rs`, `auth/{mod,flow,session}.rs`, `storage/*`, `api.rs`,
  `routes/{mod + 37 domain files}`, `endpoints/{37 domain files}`, `client/{mod + 37 domain
  files}`, `lib.rs`, `util.rs`; `endpoints/{password,settings}.rs` and their routes/client
  files deleted.
- `tests/`: `common/mod.rs`, `auth.rs`, `transport.rs`, `storage.rs`, `client_core.rs`,
  `endpoints_<domain>.rs` and `client_<domain>.rs` for every domain, `live.rs`,
  `security_paths.rs`, `path_safety.rs`, `api_aggregator.rs`; new scrubbed fixtures.
- Docs: `PARITY.md`, `README.md`, `wiki/{Home,Rust-API,Configuration,Troubleshooting}.md`,
  `wiki/Migration.md` (new), this file.
- `Cargo.lock` (rustls 0.23.45).

Size after the port: about 29.7 k lines under `src/`, 22.1 k under `tests/`, 84 test files.

## 5. Commands run for verification (every push was preceded by the full set)

```
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-features
cargo test --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo deny check
```

Local result at the final commit: format clean, both clippy runs clean, docs clean,
`cargo deny` clean, tests below.

| Suite | Passed | Failed | Ignored |
|---|---|---|---|
| `--all-features` | 1169 | 0 | 4 (live) |
| `--no-default-features` | 1159 | 0 | 3 (live) |

Baseline before the port was 568 / 558.

## 6. CI verification

Filled in after the pull request's checks completed — see the section at the end of this
file.

## 7. Outstanding

- **Live validation** against a real account has not been re-run on the v8 surface; every
  write that `eero-api`'s API reference marks unverified is documented as such and logs a
  warning before the request. `tests/live.rs` has a read-only sweep (`RUSTEERO_LIVE=1`).
- **Not published** (decision D-15 holds; `publish = false`).
- The next semantic release will be **2.0.0** (several `BREAKING CHANGE` footers).
- `rstest` is declared as a dev-dependency but unused.
- Follow-ups recorded in the private handover: typed models feature, agentic workflows,
  a write-side live sweep.
