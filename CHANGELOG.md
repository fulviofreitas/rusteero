# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0](https://github.com/fulviofreitas/rusteero/compare/v2.0.0...v3.0.0) (2026-10-23)

### ⚠ BREAKING CHANGES

* **burst:** BurstReportersApi::create_burst_reporter now takes reporter_data: Option<Value>
instead of a mandatory Value. Pass None for the new default, or Some(value) for the deprecated body.
* **auth:** AuthApi::set_session_token, AuthApi::clear_session_token,
AuthApi::clear_auth_data, Client::set_session_token and Client::clear_session_token are
now async fn; add .await at every call site. Validation and its messages are unchanged.
* **api:** the nine DataUsageApi time-windowed reads (get_data_usage, get_breakdown,
get_devices_usage, get_device_usage, get_eeros_summary, get_eero_usage, get_profile_usage,
get_unprofiled_devices, get_unprofiled_summary) gain a timeout: Option<RequestTimeout>
parameter immediately before parent. The nine Client wrappers (get_data_usage,
get_data_usage_breakdown, get_devices_data_usage, get_device_data_usage,
get_eeros_data_usage_summary, get_eero_data_usage, get_profile_data_usage,
get_unprofiled_devices_data_usage, get_unprofiled_data_usage_summary) gain the same
parameter immediately before network_id. Pass None to keep the previous behaviour.
* **api:** Error::Authentication and Error::RateLimit gained a status: Option<u16> field. Struct literals and exhaustive patterns must add status (or use `..`).

### ✨ Features

* **account:** add AccountApi::get_account ([40c5976](https://github.com/fulviofreitas/rusteero/commit/40c59767a1e7c8b0eea7a51f3b5dd00eee47be87))
* **api:** add get_eeros_usage ([958dfd7](https://github.com/fulviofreitas/rusteero/commit/958dfd73e2b9d49ec274f1f792f3dca71924f39d))
* **api:** add links::rewrite_version for API-version pinning ([ff59d00](https://github.com/fulviofreitas/rusteero/commit/ff59d0040511512f18f96f2c28f2141ac43870aa))
* **api:** add require_non_empty_string and require_hour validators ([fc9d6d7](https://github.com/fulviofreitas/rusteero/commit/fc9d6d737ee74424df56668ceb6f88cc8c7c5eae))
* **api:** add the RETRY_AFTER_MAX constant ([8475ce9](https://github.com/fulviofreitas/rusteero/commit/8475ce95a986d2b4ff1fa5dc4263a8a4f3923321))
* **api:** add the shared origin_mismatch rule ([a427750](https://github.com/fulviofreitas/rusteero/commit/a42775083b7de3df9a0a9f86b7dfa356c4481988))
* **api:** carry the HTTP status on authentication and rate-limit errors ([d92c0f0](https://github.com/fulviofreitas/rusteero/commit/d92c0f0a8be78fcff16f491f9c42f7af53872ea5))
* **api:** expose Transport::user_agent and validate headers like eero-api ([fccaa91](https://github.com/fulviofreitas/rusteero/commit/fccaa913ece3170399fa7c830c0fa2ced77fc5df))
* **api:** per-call RequestTimeout on the transport ([45c1fc8](https://github.com/fulviofreitas/rusteero/commit/45c1fc89fccf223226e0b0a218dfec989861cf8a))
* **api:** per-call timeout on data-usage reads ([59d2dae](https://github.com/fulviofreitas/rusteero/commit/59d2dae67aafc117bea7e03ee1dd4759de3a01b5))
* **api:** re-export RequestTimeout, rewrite_version and the origin rule at the crate root ([ca7da08](https://github.com/fulviofreitas/rusteero/commit/ca7da08672dd73125dfb3f9eab4bd29943be3fa9))
* **auth:** expose credentials_persisted on AuthApi, EeroApi and Client ([81adc97](https://github.com/fulviofreitas/rusteero/commit/81adc974e9e693babb7da7f753fd6429d12db3a3))
* **burst:** send the empty-string body by default and deprecate reporter_data ([38554f1](https://github.com/fulviofreitas/rusteero/commit/38554f17285e28b675ed35268bb064772243d0d6))
* **client:** add add_to_blacklist and remove_from_blacklist ([e59cb75](https://github.com/fulviofreitas/rusteero/commit/e59cb7563d09b4dc89ee1124548939e7257d0251))
* **client:** add create_burst_reporter ([d068b69](https://github.com/fulviofreitas/rusteero/commit/d068b69e39e025cd455ef33e94dc80c9f04b7fad))
* **client:** add get_eeros_data_usage ([2b640cd](https://github.com/fulviofreitas/rusteero/commit/2b640cd484ddcdd7ebe0351490ca03e18a881304))
* **client:** add set_network_ad_block and set_network_block_malware ([8ec04f5](https://github.com/fulviofreitas/rusteero/commit/8ec04f51558f3dc1dd3d590ea60e0714275d99ae))
* **client:** expose reboot_network and set_ipv6_upstream ([f997c43](https://github.com/fulviofreitas/rusteero/commit/f997c434938db300fa5c51da7c5f2cb10535a7ee))
* **client:** expose request_support and the bedtime helpers ([37360e3](https://github.com/fulviofreitas/rusteero/commit/37360e3a6c9c41489485b3e8fd9ae042ba30b6ae))
* **client:** expose set_hide_5g and set_preferred_update_hour on Client ([a3f15e0](https://github.com/fulviofreitas/rusteero/commit/a3f15e0dec4b77a88bb9225b712e2b0248c6b59b))
* **dhcp:** accept the custom mode and map modes to wire values ([9362025](https://github.com/fulviofreitas/rusteero/commit/9362025bd686805d584368002b9c7b788d8a4f25))
* **dns-policies:** add network ad-block and malware switches ([7a6eba2](https://github.com/fulviofreitas/rusteero/commit/7a6eba285bc3f47f8b6bf0f09062b8c88638cdc5))
* **networks:** add set_hide_5g ([c608838](https://github.com/fulviofreitas/rusteero/commit/c6088382cd68923f04b06b456e252d272b647ef6))
* **security:** add set_ipv6_upstream ([8fb2c62](https://github.com/fulviofreitas/rusteero/commit/8fb2c62aa8de2d9cce8c31d01698fdb5a2aef6b7))
* **updates:** add set_preferred_update_hour ([b9a4867](https://github.com/fulviofreitas/rusteero/commit/b9a4867262b91b0824c0919562b46b032b5609cc))

### 🐛 Bug Fixes

* **api:** confine caller paths to the template family ([e52a85b](https://github.com/fulviofreitas/rusteero/commit/e52a85b641d862f0f4d9c6235ab704861a0de747))
* **api:** enforce write version pins after resolving network URLs ([e899a63](https://github.com/fulviofreitas/rusteero/commit/e899a63e4edbdfeff17935f536900841760fc3ef))
* **api:** harden transport bodies, retries, origin gate and base_url like eero-api 8.4.1 ([053dcf8](https://github.com/fulviofreitas/rusteero/commit/053dcf8ad44e66513c94c43e23b8c62cefe6cedb))
* **api:** reject invalid names in network and guest setters ([6fb2c5f](https://github.com/fulviofreitas/rusteero/commit/6fb2c5fc16be9bfce53576fc847900cd08a7f109))
* **api:** reject invalid Wi-Fi passwords before sending requests ([eba33eb](https://github.com/fulviofreitas/rusteero/commit/eba33eba339c276792fe72e5b15ca52258d0fc7d))
* **api:** resolve data-usage parents without re-confining published links ([015985d](https://github.com/fulviofreitas/rusteero/commit/015985d96d4a2c99e6324429dd4af34c31d34713))
* **api:** resolve the rewrite_version doc link and gate a Linux-only test import ([09f4da6](https://github.com/fulviofreitas/rusteero/commit/09f4da693ca5c2492286d8d733dc0230ed998f42))
* **auth:** count a save cancelled before it reached the store as not persisted ([3a7bb92](https://github.com/fulviofreitas/rusteero/commit/3a7bb92bab5f639884dbbda94b53cb717dd16c8d))
* **auth:** log a failed chained clear once and keep the WARN for store task failures ([39c4a26](https://github.com/fulviofreitas/rusteero/commit/39c4a26d2776feca7a5d0d21e83dfc7b4c49dbff))
* **auth:** log a failed logout call with the wording eero-api uses ([7eda2bd](https://github.com/fulviofreitas/rusteero/commit/7eda2bda7fc9a420af122e9016e66cad52787692))
* **auth:** log each credential-store failure once ([b6f6cfa](https://github.com/fulviofreitas/rusteero/commit/b6f6cfa90e055a8971a64ec5cab02d49bec8b6b4))
* **auth:** make unpersisted credential saves visible and clean up storage ([965e21e](https://github.com/fulviofreitas/rusteero/commit/965e21e2f9806fb360bf869b9d35b403a1a1fed9))
* **blacklist:** mark add_to_blacklist as live-verified and drop its warning ([9388854](https://github.com/fulviofreitas/rusteero/commit/938885450e64d2fd6b078137e4c2fd6d64542988))
* **cache:** keep no entries when the cache TTL is zero ([5ece07c](https://github.com/fulviofreitas/rusteero/commit/5ece07ce04cd46a1c2c490b3499df0855bb0df7f))
* **cache:** reject cache fills from reads overtaken by invalidation ([66aaea5](https://github.com/fulviofreitas/rusteero/commit/66aaea5edcfb9615118933b431fd23e1d090d62e))
* **cache:** reject get_network and get_eeros fills overtaken by invalidation ([11e200c](https://github.com/fulviofreitas/rusteero/commit/11e200c3ae518701313b5cb93fcf65c7ef10551a))
* **client:** canonical device keys, session reset and in-flight read guard ([df6af0b](https://github.com/fulviofreitas/rusteero/commit/df6af0be6ef273dc8b817c316ef9dfbd4d96e230))
* **client:** pin support, forwards and routing reads to API 2.2 ([11dc6c8](https://github.com/fulviofreitas/rusteero/commit/11dc6c8ffbf2e9c2b35569f0073c9520f1c25cb5))
* **client:** reset the cache and discovered network before the credential-store hop ([0c4ad4d](https://github.com/fulviofreitas/rusteero/commit/0c4ad4d29833f47f90e4f31b5584ee2715f4e4d4))
* **client:** send the current eero iOS client version as the default User-Agent ([851c03e](https://github.com/fulviofreitas/rusteero/commit/851c03e7e7f5f75a014f532bc9dfa26f6e427b22)), closes [#135](https://github.com/fulviofreitas/rusteero/issues/135)
* **dhcp:** warn after resolving the URL ([01d70a5](https://github.com/fulviofreitas/rusteero/commit/01d70a54ccc091c64f509f6a38cd3a3cf54e4ee1))
* **logging:** keep URLs out of transport WARNING and ERROR lines ([6df0ce3](https://github.com/fulviofreitas/rusteero/commit/6df0ce30c58d6d31e902296b844129e679ca73b7))
* **logging:** log Python's generic API error line for non-success statuses ([4d2ba38](https://github.com/fulviofreitas/rusteero/commit/4d2ba38646ef933421b6e4a427fbd6098d0e6980))
* **logging:** redact top-level arrays and cap redaction depth ([c070065](https://github.com/fulviofreitas/rusteero/commit/c070065bb87893264b4a38bc1b1e316d39880d4f))
* **schedule:** trust published schedule envelopes and refuse bare schedule ids ([f9f8629](https://github.com/fulviofreitas/rusteero/commit/f9f8629ffc1ff42dad2ed24eecafa6aecab914c7))
* **security:** render set_mlo_mode's validation message with Python's repr ([0c678c2](https://github.com/fulviofreitas/rusteero/commit/0c678c27191c9f6db35889ceb2e1c14f76453f69))
* **thread:** make set_thread_enabled an alias of update_thread ([d490987](https://github.com/fulviofreitas/rusteero/commit/d4909872cde76ff9df2a6bbae581d98e458f50cf))

### ⚡ Performance

* **auth:** run credential-store I/O for session changes off the async runtime ([0927e89](https://github.com/fulviofreitas/rusteero/commit/0927e898bcfb808d5e0f7fec1e5086f6ba1bfeb3))

### 📚 Documentation

* **account:** re-cite the v8.0.4 line ranges and warn lines against eero-api v8.4.1 ([3b80e02](https://github.com/fulviofreitas/rusteero/commit/3b80e02bc4fec730915bf1c823ba24ad328f6aed))
* add the eero-api 8.4.1 parity report ([8605b3b](https://github.com/fulviofreitas/rusteero/commit/8605b3beab4f2b649114b384cb2ee41d0d0d4418))
* **api:** cite eero-api v8.4.1 in the networks, updates, thread and security modules ([a509188](https://github.com/fulviofreitas/rusteero/commit/a509188f826ff88dafd5791655f81032edc1dba3))
* **api:** describe the default timeouts as the transport now enforces them ([6ced53b](https://github.com/fulviofreitas/rusteero/commit/6ced53b18c0b1af2be1a1f8e6ba147789ad2d863))
* **auth:** cite eero-api v8.4.1 in the login flow and drop internal labels from storage docs ([f41f1bd](https://github.com/fulviofreitas/rusteero/commit/f41f1bd3c9882212471232a262573317df121037))
* **client:** drop the stale refresh-retry divergence and refresh token wording ([0b5eec7](https://github.com/fulviofreitas/rusteero/commit/0b5eec78fd11fbd6380a88ebc108273c5502fd75))
* **client:** say which device request an empty id is rejected before ([b570616](https://github.com/fulviofreitas/rusteero/commit/b570616e023c24a7daba454704078a0c889aecf8))
* correct README, wiki and PARITY claims that did not match the code ([b02b6ed](https://github.com/fulviofreitas/rusteero/commit/b02b6ed2926c1e8aff9b5828a76c770b790f6109))
* **data-usage:** cite eero-api v8.4.1 line ranges and describe the module as it is ([63d407a](https://github.com/fulviofreitas/rusteero/commit/63d407a420b05f811e421905725cb9db3ddb68f5))
* describe the eero-api 8.4.1 port in the README and wiki ([db9edc9](https://github.com/fulviofreitas/rusteero/commit/db9edc9f52a7a834c1bc845d3718cd2b82fc1863))
* describe zero-TTL caching, the trailing-slash join and the test doubles accurately ([446aa93](https://github.com/fulviofreitas/rusteero/commit/446aa936cb09c455721383347c47c5220aa8a12d))
* **dns-policies:** re-cite the v8.0.4 line ranges against eero-api v8.4.1 ([e8fc3d8](https://github.com/fulviofreitas/rusteero/commit/e8fc3d8109a7e31b4d7aa21633550e35363c0e9a))
* drop the publishing status notes; keep the install snippet only ([98d593d](https://github.com/fulviofreitas/rusteero/commit/98d593d84dc24fca4e4e1c9e3dc105da4cef61aa))
* **logging:** say what a container under a sensitive key shows ([a4688b3](https://github.com/fulviofreitas/rusteero/commit/a4688b3924361fc0eb761b7d9dd8b681168c3015))
* note that a failed chained clear logs once and store task failures always warn ([9c0621f](https://github.com/fulviofreitas/rusteero/commit/9c0621f101f002b3bc398d4b3b9af7a67fc1fe1d))
* **parity:** drop references to notes that are not part of this repository ([8a50075](https://github.com/fulviofreitas/rusteero/commit/8a500751dab3bc214323fe38aaa4a62c51234bf0))
* **parity:** note the trailing-slash join and the set_mlo_mode message ([3bac946](https://github.com/fulviofreitas/rusteero/commit/3bac94659438332f5cfbbb60dbad223e4232fac0))
* **parity:** rebaseline PARITY.md on eero-api v8.4.1 ([17c48e3](https://github.com/fulviofreitas/rusteero/commit/17c48e33318a7d7ead112597ab1a7afef0fd0c39))
* point the install instructions at the crates.io release ([98ea0b1](https://github.com/fulviofreitas/rusteero/commit/98ea0b1443803af69a64581cacf5d9eb2ce1eeb4))
* record single-line store failure logging, the generic API error line and pre-hop cache resets ([e923040](https://github.com/fulviofreitas/rusteero/commit/e923040ed247dbf4ee803caaaa9ee444707b086f))
* record the third CI run and the log-capture fix in the parity report ([b3db4ca](https://github.com/fulviofreitas/rusteero/commit/b3db4cac819305ab02163ea72e01c9dd068bccfd))
* state the reasons in code comments instead of pointing at planning notes ([ee8c27c](https://github.com/fulviofreitas/rusteero/commit/ee8c27cfd188e1dfdbb26896259023966c200773))
* **test:** bring the schedule and live suite docs up to v8.4.1 ([aa7d6b1](https://github.com/fulviofreitas/rusteero/commit/aa7d6b1f09404dfc76aeadfc7e3b9190f2139b61))

### ♻️ Refactoring

* **api:** drop params::require_schedule_family ([d75a2bd](https://github.com/fulviofreitas/rusteero/commit/d75a2bd60365d7c6684783b7b0f83bd6a1e45ad7))
* **api:** express RETRY_AFTER_MAX in hours ([05453b7](https://github.com/fulviofreitas/rusteero/commit/05453b715658bd918397632de805f008b373fd3e))
* **api:** keep origin_mismatch under rusteero::links instead of the crate root ([c171b67](https://github.com/fulviofreitas/rusteero/commit/c171b67e4b1a80db6adda1fa54690fe75d32a235))
* **auth:** recognise a not-retained credential write by its StorageError variant ([298d4b7](https://github.com/fulviofreitas/rusteero/commit/298d4b7b059ac4e41525be6830feb39b2edb0ca1))
* **client:** derive the blacklist MAC cache key before the write ([216d747](https://github.com/fulviofreitas/rusteero/commit/216d7474dd3df31bc6b1310f885e0c3ac896c86c))
* **client:** drop stale dead-code allowances on the parent helpers ([dd27fdb](https://github.com/fulviofreitas/rusteero/commit/dd27fdb7de5f1d15d9653550471c484fb89fc472))
* **client:** take zero or more device ids when invalidating the device cache ([b9d2103](https://github.com/fulviofreitas/rusteero/commit/b9d2103822577d295b4df77cea0237c2207fa767))
* **networks:** deprecate NetworksApi::get_account ([f198444](https://github.com/fulviofreitas/rusteero/commit/f198444c73232e190aa4ac06ddf67252ad27f450))
* **schedule:** check caller schedule paths with links::require_family_path ([b88d6b1](https://github.com/fulviofreitas/rusteero/commit/b88d6b13e5ce764acdd529687a5a420e40600ebb))

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
