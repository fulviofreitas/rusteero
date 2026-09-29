# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0](https://github.com/fulviofreitas/rusteero/compare/v1.0.0...v2.0.0) (2026-09-29)

### ⚠ BREAKING CHANGES

* see the branch commits — session transport, Error shape, removed dead surface, split block/unblock, new data-usage and ouicheck signatures, http_builder escape hatch.
* **security:** TransportBuilder::http, ClientBuilder::http and
LoginFlow::new take a reqwest::ClientBuilder (http_builder) instead of a
built reqwest::Client.
* **endpoints:** removed dead surface (activity, settings, password reads,
device priority, set_ipv6_dns, run_insights, run_ouicheck, SecurityApi
set_thread, the SQM variants, the old backup and profile-schedule methods,
profile content-filter writes); block_device is split into
block_device/unblock_device; get_data_usage and get_ouicheck take the
parameters the API requires; set_guest_network no longer takes a password.
* **api:** SettingsApi, PasswordApi, Client::get_settings and
Client::get_password are removed (eero-api 8.0.0 dropped both modules).
* **auth:** Session has no expiry or refresh token; logout returns
bool; the account/refresh fallback and the verify Set-Cookie hedge are gone.
* **auth:** Session has no expiry or refresh token; logout returns
bool; the account/refresh fallback and the verify Set-Cookie hedge are gone.
* **core:** Error variants change shape; 404 maps to Error::NotFound.

### ✨ Features

* **api:** scaffold the fourteen v8 domain modules and drop settings and password ([165643b](https://github.com/fulviofreitas/rusteero/commit/165643bc57d02b6768e2c50f87657b1e23bc09ca))
* **auth:** v8 session transport, form-encoded handshake and schema-2 credentials ([bad5a3c](https://github.com/fulviofreitas/rusteero/commit/bad5a3c0c6d32f686ad3ed5e746d9b099dc70a9c))
* **auth:** v8 session transport, form-encoded handshake and schema-2 credentials ([0685271](https://github.com/fulviofreitas/rusteero/commit/0685271ec63d03d5d3ee095ce419c677738e7ee1))
* **core:** error catalogue, envelope-carrying Error, link resolution and v8 constants ([4e30fad](https://github.com/fulviofreitas/rusteero/commit/4e30fada903b14f8724238ef1a8e524e43992b21))
* **endpoints:** port every domain module to the eero-api 8.0.4 wire contract ([e2cc560](https://github.com/fulviofreitas/rusteero/commit/e2cc560869f6860fed5fd94f121e94be644098ba))
* port eero-api 8.0.4 (v8 transport, error catalogue, link resolution, 14 new domains) ([#3](https://github.com/fulviofreitas/rusteero/issues/3)) ([0c527b4](https://github.com/fulviofreitas/rusteero/commit/0c527b45ded12ce0b052d4974df7a5fd787bd826))

### 🐛 Bug Fixes

* **endpoints:** close the phase G verification findings ([c7ee8d7](https://github.com/fulviofreitas/rusteero/commit/c7ee8d7ecdb6ed8a3fc56cb6c184e260e8fa5732))
* **security:** close the review findings on the v8 transport, links and storage ([ec1488e](https://github.com/fulviofreitas/rusteero/commit/ec1488e4756f2469796c628acd4bdfa4356d543e))

### 📚 Documentation

* add the eero-api 8.0.4 port report ([c123a75](https://github.com/fulviofreitas/rusteero/commit/c123a757d998665b41b64fede2145243d85eb442))
* keep only technical usage documentation in the crate ([8da305a](https://github.com/fulviofreitas/rusteero/commit/8da305a99b9762b5b10595e13be54fc843d07def))
* regenerate PARITY.md and rewrite the README and wiki for the 8.0.4 surface ([5d2b247](https://github.com/fulviofreitas/rusteero/commit/5d2b247886bf15ea7e6b57fcb02abcb3e8524fef))
* **report:** record the CI verification results ([3045452](https://github.com/fulviofreitas/rusteero/commit/3045452588baccf52d8da134d64cc039e685b620))
* **wiki:** describe the http_builder escape hatch and its always-on redirect refusal ([88987ef](https://github.com/fulviofreitas/rusteero/commit/88987ef7fa836de77ab031b71d3479c2d42922d1))

### ♻️ Refactoring

* split routes and client into one file per domain module ([59a0a23](https://github.com/fulviofreitas/rusteero/commit/59a0a23885a3cb5e122ed88886525f3077601e60))
* **tests:** split the integration suites into one file per domain ([b5036aa](https://github.com/fulviofreitas/rusteero/commit/b5036aa5a50fc0aadb19c219d8fa68672b49181b))

## 1.0.0 (2026-09-14)

### ✨ Features

* **api:** EeroApi aggregator over one shared transport ([4b2a1a3](https://github.com/fulviofreitas/rusteero/commit/4b2a1a3b911cd4543a0dea5e6439a1b3afbc8aac))
* **auth:** AuthApi and the shared wiremock test harness ([b11bc28](https://github.com/fulviofreitas/rusteero/commit/b11bc28f2a779840c7e5e068f2092b07f2303a4f))
* **auth:** LoginFlow and PendingLogin for the one-time-code handshake ([0c17859](https://github.com/fulviofreitas/rusteero/commit/0c178592c3df31167f3395163ce4615155ff2f72))
* **cache:** TTL cache with Python's bucket keys and falsy-value rule ([725c26b](https://github.com/fulviofreitas/rusteero/commit/725c26be0fdd6778defd9724ee4e64aa9cf7f498))
* **client:** facade with cached getters and network-id resolution ([e09bedc](https://github.com/fulviofreitas/rusteero/commit/e09bedc3cb2898f86da91ad7b0c5da924f390270))
* **client:** mutation pass-throughs with targeted cache invalidation ([ef41a79](https://github.com/fulviofreitas/rusteero/commit/ef41a79b25cb43d1947bc0f62ae2bbb60c5cbd4f))
* **core:** error taxonomy, envelope, routes, consts, session and memory store ([cfffbde](https://github.com/fulviofreitas/rusteero/commit/cfffbde9e9d8c2365e84ffb29e25210109e0c165))
* **endpoints:** mutating halves of every domain module ([60eeaf7](https://github.com/fulviofreitas/rusteero/commit/60eeaf7d1c76dba83c0940962c27acb28a917746)), closes [#102](https://github.com/fulviofreitas/rusteero/issues/102) [#109](https://github.com/fulviofreitas/rusteero/issues/109)
* **endpoints:** read-only halves of the remaining 21 modules ([0a932e0](https://github.com/fulviofreitas/rusteero/commit/0a932e03b3196f9f3990d93f2f6823b66dac7fb4))
* **endpoints:** read-only networks, devices, eeros and profiles ([1c7902b](https://github.com/fulviofreitas/rusteero/commit/1c7902bb1c46ac08992f3f485f1882591dec918a))
* **live:** read-only endpoint sweep, and re-export Client at the crate root ([ef0c5f5](https://github.com/fulviofreitas/rusteero/commit/ef0c5f5d33162a35efe2542818e9f039a568820c))
* **routes:** every wire endpoint as a Route constant ([48f3ab9](https://github.com/fulviofreitas/rusteero/commit/48f3ab90414394bc64de18a317c8451f1d1bafbf)), closes [#102](https://github.com/fulviofreitas/rusteero/issues/102)
* **storage:** file, chained and keyring credential stores with the factory ([92842cd](https://github.com/fulviofreitas/rusteero/commit/92842cdb1ee286caae1942fe090603d79b149235))
* **transport:** request core with status mapping, 10 MiB cap and refresh retry ([c37ee3c](https://github.com/fulviofreitas/rusteero/commit/c37ee3ccbe15ba8ffdc7c2c2ae4b824d290597ec))

### 🐛 Bug Fixes

* **auth:** close the phase 1 review findings ([5dc9f2d](https://github.com/fulviofreitas/rusteero/commit/5dc9f2da11bd62fbbd00609919980751772c9baa))
* **security:** reject traversable path segments and close five cache and redaction gaps ([d572680](https://github.com/fulviofreitas/rusteero/commit/d572680ba118912e0f1846386909728cebee9643))
* **storage:** partial-erase, stale-credential and log-leak findings ([82f3020](https://github.com/fulviofreitas/rusteero/commit/82f3020edcca154578a0d3a57f1905a2c1c40311))
* **transport:** stop logging the refresh response body at error level ([eb469d4](https://github.com/fulviofreitas/rusteero/commit/eb469d4c7a7cce4643105156970e731c6eff6100))

### 📚 Documentation

* move the long-form acknowledgments to the wiki ([8f883ac](https://github.com/fulviofreitas/rusteero/commit/8f883aca0ba60635ddafd227f50f0601d5d68b3d))
* **parity:** mark phases 1 and 2 rows ported, add the storage section ([daf5c03](https://github.com/fulviofreitas/rusteero/commit/daf5c03dcd945055126ecc3ac4cf592848c81ee8))
* **parity:** mark the 25 read-only endpoint modules ported ([e3f797c](https://github.com/fulviofreitas/rusteero/commit/e3f797c849c5d200d1d8fb1d3f365df42001d272))
* remove every reference to the private companion repo ([b67e63a](https://github.com/fulviofreitas/rusteero/commit/b67e63aa1ea4a8df4834012d9c9ea95759655947))
* rewrite the wiki and README against the shipped API ([42881aa](https://github.com/fulviofreitas/rusteero/commit/42881aaa66eacb90bdc34ffea83536c991e87599))

_No releases yet. Entries are generated by semantic-release from conventional commits._
