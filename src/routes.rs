//! Every wire endpoint as a [`Route`] constant.
//!
//! Endpoint modules never build URLs by hand; upstream drift is a one-line fix here.
//!
//! There is no Python original for this module: `eero-api` builds each URL inline, string by
//! string, in every `api/*.py` file (e.g. `f"{API_ENDPOINT}/networks/{network_id}/devices"`).
//! `rusteero` centralises the same information as data — a [`Route`] pairs an HTTP
//! [`Method`], an [`ApiVersion`] (which selects the base host), and a path template — so that a
//! server-side path rename or version bump is a single-line edit with one wiremock test to
//! match, instead of a grep-and-replace across two dozen endpoint modules (port plan §7.1).
//!
//! ## Path templates and rendering
//!
//! A path template is a `'static` string of `/`-separated segments. A segment written as
//! `{name}` is a placeholder that [`Route::render`] fills in from the caller-supplied
//! `params` at request time; every other segment is a literal, written by us, and passed
//! through unchanged (aside from the same percent-encoding pass every segment gets — see
//! below).
//!
//! [`Route::render`] is total: for any `path` and any `params`, it returns either `Ok(Url)` or
//! `Err(RenderError)`, and never panics. Design choices worth calling out:
//!
//! - **Missing placeholder → `Err`, not left unsubstituted.** If a template segment is
//!   `{name}` and `params` has no entry for `name`, rendering fails with
//!   `RenderError::MissingPlaceholder`. Silently emitting the literal text `{name}` into a
//!   request URL would send a malformed, likely-404 request to the real API with no compile-time
//!   or type-level signal that a caller forgot a parameter; failing fast is safer and matches
//!   this crate's "every fallible operation returns `Result`" convention.
//! - **Every substituted value is validated before it reaches the encoder**, by
//!   `validate_segment` — see the "Path traversal" section below for why this check exists
//!   and exactly what it rejects.
//! - **Percent-encoding uses only the `url` crate's own segment-builder**, [`Url::path_segments_mut`],
//!   never hand-rolled string concatenation. Each segment (literal or substituted) that passes
//!   validation is pushed through [`url::PathSegmentsMut::push`], which percent-encodes it for
//!   the path-segment position — including `%`, `/`, and `?`, none of which can therefore ever
//!   terminate the segment early or introduce a new path segment, query string, or fragment.
//!
//! ## Path traversal (formerly documented, incorrectly, as impossible)
//!
//! An earlier revision of this module claimed that a `".."` placeholder value "does not error
//! and does not escape into the parent path" because [`Url::path_segments_mut`] drops a segment
//! that is *exactly* `"."` or `".."` rather than appending it. That claim was false for any
//! value that is not already, byte-for-byte, `"."` or `".."`.
//!
//! `url` 2.5.8's path parser strips every ASCII tab, carriage return, and line feed from a
//! segment **before** applying dot-segment removal. A value such as `"..\n"` is therefore not
//! `".."` when `validate_segment` (or, pre-fix, nothing at all) sees it, but *is* `".."` by the
//! time the encoder's dot-segment check runs — so it collapses the rendered path exactly as a
//! literal `".."` would, silently walking a destructive verb (a `DELETE` or a `/2.3` `PUT`) onto
//! the parent collection instead of the one item the caller named. A stray trailing newline on
//! an id read from a file, an environment variable, or `$(cat …)` is enough to trigger this —
//! no attacker input is required. A second, related gap: an **empty** substituted value collapses
//! a per-item route onto its collection while keeping the same verb (`DELETE
//! .../blacklist/{id}` with `id = ""` renders `DELETE .../blacklist/`), which — if the server
//! treats that path as "the collection" — turns a "remove one" into "remove all".
//!
//! What is actually guaranteed today: `validate_segment` runs on every substituted value
//! before it is pushed onto the URL, and rejects it outright — the request is never sent — if
//! the value is empty, contains any ASCII control character (`0x00`-`0x1F` or `0x7F`, which
//! already covers every byte the encoder's tab/CR/LF-stripping pass would otherwise remove), or
//! is exactly `"."` or `".."` after that stripping. [`Route::render`] and `Transport`'s own
//! renderer (`transport.rs`) share this exact rule by calling the same function, so the
//! guarantee holds identically for both. A value containing an *already-encoded* dot segment
//! (e.g. `"%2e%2e"`), a slash (`"a/b"`), or non-ASCII look-alike dots (e.g. fullwidth `"．．"`)
//! is unaffected by this check and continues to be percent-encoded into a single opaque segment
//! as before — none of those can reach the parser's dot-segment logic at all.

use crate::consts;
use reqwest::Method;
use url::Url;

/// Selects which Eero cloud API host/version a [`Route`] targets.
///
/// Ported from the fact — not the code shape — of `const.py:7,13`: `eero-api` hardcodes two
/// base URLs (`API_ENDPOINT` for almost everything, `DEVICE_UPDATE_ENDPOINT` for device
/// nickname/pause writes only). `rusteero` keeps the same two hosts but names them as an enum so
/// every [`Route`] states its version explicitly instead of picking a base-URL constant by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiVersion {
    /// API version 2.2 — the default for almost every endpoint.
    ///
    /// Base URL: [`consts::API_BASE_22`].
    V2_2,
    /// API version 2.3 — required for device-mutation writes (nickname, pause); see
    /// [`consts::API_BASE_23`] for why 2.2 cannot be used for those instead.
    V2_3,
}

impl ApiVersion {
    /// Returns the base URL for this API version.
    #[must_use]
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::V2_2 => consts::API_BASE_22,
            Self::V2_3 => consts::API_BASE_23,
        }
    }
}

/// A single wire endpoint: an HTTP method, an API version (which selects the base host), and a
/// `'static` path template.
///
/// `Route` values are plain data — constructing one never fails and never touches the network.
/// Every field is `pub` so a `Route` can be built as a `const` (see the constants below) or
/// assembled ad hoc (e.g. in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// HTTP verb this endpoint expects.
    pub method: Method,
    /// Which API version (and therefore base host) this endpoint is served from.
    pub version: ApiVersion,
    /// `/`-separated path template, relative to [`ApiVersion::base_url`]. Segments written as
    /// `{name}` are placeholders filled in by [`Route::render`]; every other segment is a
    /// literal.
    pub path: &'static str,
}

/// Why `validate_segment` rejected a value before it could be substituted into a rendered
/// path segment.
///
/// This is the single validation rule shared by [`Route::render`] and `Transport`'s own
/// renderer (`transport.rs`'s `render_url`) — see the module docs' "Path traversal" section for
/// the vulnerability this closes and exactly what each variant means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentError {
    /// The value is empty.
    ///
    /// An empty substituted value would otherwise render as *no segment at all*, collapsing a
    /// per-item route onto its parent collection while keeping the same (potentially
    /// destructive) HTTP verb.
    Empty,
    /// The value contains an ASCII control character (`0x00`-`0x1F` or `0x7F`).
    ///
    /// This already covers every ASCII tab, carriage return, and line feed — the exact bytes
    /// `url` 2.5.8 strips from a path segment before applying dot-segment removal, which is what
    /// makes a value such as `"..\n"` behave as `".."` once it reaches the encoder.
    ControlCharacter,
    /// The value, with every ASCII tab, carriage return, and line feed removed, is exactly `"."`
    /// or `".."`.
    ///
    /// A value that already contains one of those bytes is rejected earlier, as
    /// [`SegmentError::ControlCharacter`]; this variant catches the remaining case — a literal
    /// `"."` or `".."` with no whitespace at all — which the `url` crate would otherwise drop
    /// silently instead of treating as an error.
    DotSegment,
}

impl std::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "must not be empty"),
            Self::ControlCharacter => {
                write!(f, "must not contain an ASCII control character")
            }
            Self::DotSegment => write!(f, "must not be a \".\" or \"..\" path segment"),
        }
    }
}

impl std::error::Error for SegmentError {}

/// Validates that `value` is safe to substitute into a single rendered path segment.
///
/// The one place a caller-supplied value is checked before either renderer in this crate
/// ([`Route::render`] and `Transport`'s own `render_url`) hands it to
/// [`Url::path_segments_mut`]. Validating the *raw* value here, rather than trusting the `url`
/// crate's own encoder to make any value safe, is deliberate: the encoder strips ASCII
/// tab/CR/LF bytes from a segment *before* percent-encoding and dot-segment removal run, so a
/// value that is not literally `".."` can still be treated as `".."` by the time it is written
/// into the URL. See the module docs' "Path traversal" section for the full mechanism and a
/// concrete example.
///
/// # Errors
///
/// Returns [`SegmentError::Empty`] for an empty value, [`SegmentError::ControlCharacter`] for a
/// value containing any ASCII control character (`0x00`-`0x1F` or `0x7F`), or
/// [`SegmentError::DotSegment`] if the value is exactly `"."` or `".."` once every ASCII tab,
/// carriage return, and line feed has been removed from it. A value that does not trip any of
/// these checks is otherwise unrestricted — including one containing `/`, `%`, `?`, `#`, or a
/// non-ASCII character — and is percent-encoded into a single opaque segment exactly as before.
pub(crate) fn validate_segment(value: &str) -> Result<(), SegmentError> {
    if value.is_empty() {
        return Err(SegmentError::Empty);
    }
    if value.bytes().any(|b| matches!(b, 0x00..=0x1F | 0x7F)) {
        return Err(SegmentError::ControlCharacter);
    }
    // Reached only when `value` contains no ASCII tab/CR/LF at all (the loop above already
    // rejected any value that does, since those bytes fall inside `0x00..=0x1F`), so this
    // strip is a no-op in practice — kept explicit so the rule matches, byte for byte, the
    // "tab/CR/LF-stripped form" wording this check is documented (and audited) against.
    let stripped: String = value
        .chars()
        .filter(|&c| c != '\t' && c != '\r' && c != '\n')
        .collect();
    if stripped == "." || stripped == ".." {
        return Err(SegmentError::DotSegment);
    }
    Ok(())
}

/// Error produced by [`Route::render`] when a path template cannot be turned into a request
/// [`Url`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// A `{name}` placeholder in the route's path template had no corresponding entry in the
    /// `params` passed to [`Route::render`].
    ///
    /// Carries the placeholder's name, without the surrounding braces.
    MissingPlaceholder(String),
    /// A `{name}` placeholder's substituted value was rejected by `validate_segment` before it
    /// could be written into the rendered path.
    InvalidSegment {
        /// The placeholder's name, without the surrounding braces.
        name: String,
        /// Why the value was rejected.
        reason: SegmentError,
    },
    /// The route's base URL ([`ApiVersion::base_url`]) failed to parse as a [`Url`].
    ///
    /// Unreachable for every [`Route`] defined in this module — both base URLs are fixed,
    /// well-formed `https://` literals — but kept as a real error variant (rather than a panic
    /// or `unwrap`) so [`Route::render`] stays total even if a future base URL is ever
    /// misconfigured.
    InvalidBaseUrl,
    /// The route's base URL cannot be used as a base for additional path segments (for example,
    /// a `cannot-be-a-base` URL such as `mailto:` or `data:`).
    ///
    /// Unreachable for every [`Route`] defined in this module, for the same reason as
    /// [`RenderError::InvalidBaseUrl`].
    CannotExtendBase,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPlaceholder(name) => {
                write!(
                    f,
                    "route path is missing a value for placeholder `{{{name}}}`"
                )
            }
            Self::InvalidSegment { name, reason } => {
                write!(f, "value for placeholder `{{{name}}}` is invalid: {reason}")
            }
            Self::InvalidBaseUrl => write!(f, "route base URL failed to parse"),
            Self::CannotExtendBase => write!(f, "route base URL cannot be used as a path base"),
        }
    }
}

impl std::error::Error for RenderError {}

impl Route {
    /// Renders this route's path template against `params`, substituting each `{name}`
    /// placeholder with the value from the matching `(name, value)` pair, and returns the full
    /// request [`Url`] (base host + rendered path).
    ///
    /// `params` is searched linearly; with the handful of placeholders any real route template
    /// has, this is both simpler and faster than building a map. Every substituted value is
    /// first checked by `validate_segment` — see the module docs' "Path traversal" section for
    /// exactly what that guarantees. A value that passes validation is percent-encoded for the
    /// path-segment position via [`Url::path_segments_mut`], same as every literal segment.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::MissingPlaceholder`] if the template references a name absent from
    /// `params`. Returns [`RenderError::InvalidSegment`] if a substituted value fails
    /// `validate_segment`. Returns [`RenderError::InvalidBaseUrl`] or
    /// [`RenderError::CannotExtendBase`] only if [`ApiVersion::base_url`] itself is malformed,
    /// which cannot happen for any `Route` defined in this module.
    ///
    /// This function never panics for any `path` or `params` value.
    pub fn render(&self, params: &[(&str, &str)]) -> Result<Url, RenderError> {
        let mut url =
            Url::parse(self.version.base_url()).map_err(|_| RenderError::InvalidBaseUrl)?;

        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|()| RenderError::CannotExtendBase)?;

            for part in self.path.split('/') {
                if part.is_empty() {
                    // Leading/trailing/doubled slashes in a template contribute no segment.
                    continue;
                }

                if let Some(name) = part.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    let value = params
                        .iter()
                        .find(|(key, _)| *key == name)
                        .map(|(_, value)| *value)
                        .ok_or_else(|| RenderError::MissingPlaceholder(name.to_owned()))?;
                    validate_segment(value).map_err(|reason| RenderError::InvalidSegment {
                        name: name.to_owned(),
                        reason,
                    })?;
                    segments.push(value);
                } else {
                    segments.push(part);
                }
            }
        }

        Ok(url)
    }
}

// =====================================================================================
// Auth + account routes (phase 1). Every other domain (networks, devices, eeros,
// profiles, ...) follows below (phase 3).
// =====================================================================================

/// `POST /2.2/login` — start the email/SMS login handshake.
///
/// Ported from `const.py:14` (`LOGIN_ENDPOINT`); see `api/auth.py:87-143`.
pub const LOGIN: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login",
};

/// `POST /2.2/login/verify` — complete the login handshake with the emailed/texted code.
///
/// Ported from `const.py:15` (`LOGIN_VERIFY_ENDPOINT`); see `api/auth.py:145-201`.
pub const LOGIN_VERIFY: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/verify",
};

/// `POST /2.2/login/resend` — ask the server to resend the verification code.
///
/// Ported from `const.py:14` + `auth.py:114,224` (`f"{LOGIN_ENDPOINT}/resend"`); given its own
/// `Route` here rather than being built by string concatenation at call time.
pub const LOGIN_RESEND: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/resend",
};

/// `POST /2.2/logout` — end the current session.
///
/// Ported from `const.py:16` (`LOGOUT_ENDPOINT`); see `api/auth.py:239-277`.
pub const LOGOUT: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "logout",
};

/// `POST /2.2/login/refresh` — first of the two session-refresh routes the client tries, in
/// order.
///
/// Ported from `const.py:22` (`LOGIN_REFRESH_ENDPOINT`); see `api/auth.py:299` (`REFRESH_ENDPOINTS`
/// iteration) and the port plan §7.2 (neither refresh route is confirmed to return a token; a
/// normal login never yields a refresh token, so this path is expected to be a practical no-op).
pub const LOGIN_REFRESH: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "login/refresh",
};

/// `POST /2.2/account/refresh` — second of the two session-refresh routes the client tries.
///
/// Ported from `const.py:23` (`ACCOUNT_REFRESH_ENDPOINT`); see [`LOGIN_REFRESH`] for the retry
/// order and caveats.
pub const ACCOUNT_REFRESH: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "account/refresh",
};

/// `GET /2.2/account` — fetch the account resource.
///
/// Ported from `const.py:17` (`ACCOUNT_ENDPOINT`) — a constant the Python source itself never
/// imports outside `const.py` (dead in `eero-api`), but the endpoint is real and used by
/// `rusteero`'s `Client::get_account` and the `get_networks` `/account` fallback (port plan
/// §1.6).
pub const ACCOUNT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "account",
};

// =====================================================================================
// Domain routes (phase 3). Every remaining wire endpoint from the Python `eero-api`
// package gets one `Route` constant per unique (verb, version, path) combination.
// Banners below follow the module order of the port plan's endpoint catalogue (§1.7).
// Several Python methods across different modules turn out to hit the exact same wire
// endpoint (same verb, version and path) — those are modelled as an alias `Route`
// constant (`pub const ALIAS: Route = CANONICAL;`) rather than a second literal, so a
// server-side path change is still a one-line fix.
//
// Deliberately NOT ported here (see the port plan §1.7 and this crate's lessons
// learned in `CLAUDE.md`):
// - `ActivityAPI` (`activity.py`): all five methods (`get_activity`,
//   `get_activity_clients`, `get_activity_for_device`, `get_activity_history`,
//   `get_activity_categories`) target `networks/{id}/activity*` paths that return 404 on
//   both API versions — confirmed dead against a live account (`eero-api` issue #107).
// - `DevicesAPI.set_device_priority` (`devices.py:219`): PUTs the same 2.3 device URL as
//   `SET_DEVICE_NICKNAME`/`PAUSE_DEVICE`, but the server silently ignores the
//   `prioritized`/`priority_duration` fields — a confirmed no-op (`eero-api` issue #111).
//   Giving it its own route would misleadingly suggest a working endpoint.
// =====================================================================================

// ------------------------------ ac_compat (`ACCompatAPI`) ------------------------------

/// `GET /2.2/networks/{network_id}/ac_compat` — AC compatibility information for a network.
///
/// Ported from `eero-api src/eero/api/ac_compat.py:33` (`ACCompatAPI.get_ac_compat`).
pub const GET_AC_COMPAT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ac_compat",
};

// -------------------------------- backup (`BackupAPI`) ---------------------------------

/// `GET /2.2/networks/{network_id}/backup` — backup-network (Eero Plus) configuration.
///
/// Ported from `eero-api src/eero/api/backup.py:37` (`BackupAPI.get_backup_network`).
pub const GET_BACKUP_NETWORK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup",
};

/// `GET /2.2/networks/{network_id}/backup/status` — current backup-network status.
///
/// Ported from `eero-api src/eero/api/backup.py:57` (`BackupAPI.get_backup_status`).
pub const GET_BACKUP_STATUS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup/status",
};

/// `PUT /2.2/networks/{network_id}/backup` — enable/disable or configure the backup network.
///
/// Ported from `eero-api src/eero/api/backup.py:80` (`BackupAPI.set_backup_network`). Also
/// the target of `BackupAPI.configure_backup_network` (`backup.py:114`; see
/// `CONFIGURE_BACKUP_NETWORK`) — same endpoint, a subset payload.
pub const SET_BACKUP_NETWORK: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/backup",
};

/// Alias of `SET_BACKUP_NETWORK`: `BackupAPI.configure_backup_network` PUTs the same
/// `networks/{network_id}/backup` resource with a partial `{enabled?, phone_number?}` body.
///
/// Ported from `eero-api src/eero/api/backup.py:114`.
pub const CONFIGURE_BACKUP_NETWORK: Route = SET_BACKUP_NETWORK;

// ------------------------------ blacklist (`BlacklistAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/blacklist` — list blacklisted (blocked) devices.
///
/// Ported from `eero-api src/eero/api/blacklist.py:33` (`BlacklistAPI.get_blacklist`).
pub const GET_BLACKLIST: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist",
};

/// `POST /2.2/networks/{network_id}/blacklist` — add a device (by MAC) to the blacklist.
///
/// Ported from `eero-api src/eero/api/blacklist.py:56` (`BlacklistAPI.add_to_blacklist`).
/// Also the first of the two round-trips behind `DevicesAPI.block_device(blocked=true)`
/// (`devices.py:136-186`; `eero-api` issue #109) — no separate route is needed there.
pub const ADD_TO_BLACKLIST: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist",
};

/// `DELETE /2.2/networks/{network_id}/blacklist/{mac_or_device_id}` — remove a device from
/// the blacklist.
///
/// `mac_or_device_id` accepts either a colon-separated MAC or Eero's blacklist `device_id`
/// (the same MAC with colons stripped). Ported from
/// `eero-api src/eero/api/blacklist.py:81` (`BlacklistAPI.remove_from_blacklist`). Also
/// what `DevicesAPI.block_device(blocked=false)` calls (`devices.py:136-186`; `eero-api`
/// issue #109) — no separate route is needed there.
pub const REMOVE_FROM_BLACKLIST: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/blacklist/{mac_or_device_id}",
};

// --------------------------- burst_reporters (`BurstReportersAPI`) ---------------------

/// `GET /2.2/networks/{network_id}/burst_reporters` — list burst reporters.
///
/// Ported from `eero-api src/eero/api/burst_reporters.py:33`
/// (`BurstReportersAPI.get_burst_reporters`).
pub const GET_BURST_REPORTERS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/burst_reporters",
};

/// `POST /2.2/networks/{network_id}/burst_reporters` — create a burst reporter.
///
/// Ported from `eero-api src/eero/api/burst_reporters.py:56`
/// (`BurstReportersAPI.create_burst_reporter`).
pub const CREATE_BURST_REPORTER: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/burst_reporters",
};

// ------------------------------- data_usage (`DataUsageAPI`) ---------------------------

/// `GET /2.2/networks/{network_id}/data_usage` — data-usage statistics for the whole network.
///
/// Unusually for a `GET`, the Eero cloud API expects a JSON body on this request (timezone /
/// period filters); the request body is supplied by the caller at call time, not by this
/// route. Ported from `eero-api src/eero/api/data_usage.py:33` (`DataUsageAPI.get_data_usage`
/// called with `resource=None`).
pub const GET_DATA_USAGE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/data_usage",
};

/// `GET /2.2/networks/{network_id}/data_usage/{resource}` — data-usage statistics scoped to
/// one resource (e.g. `"devices"`, `"eeros"`).
///
/// Same JSON-body-on-GET caveat as `GET_DATA_USAGE`. Ported from
/// `eero-api src/eero/api/data_usage.py:33` (`DataUsageAPI.get_data_usage` called with a
/// non-`None` `resource`).
pub const GET_DATA_USAGE_RESOURCE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/data_usage/{resource}",
};

// --------------------------------- devices (`DevicesAPI`) ------------------------------

/// `GET /2.2/networks/{network_id}/devices` — list devices connected to a network.
///
/// Ported from `eero-api src/eero/api/devices.py:67` (`DevicesAPI.get_devices`).
pub const GET_DEVICES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices",
};

/// `GET /2.2/networks/{network_id}/devices/{device_id}` — a single device's details.
///
/// Ported from `eero-api src/eero/api/devices.py:87` (`DevicesAPI.get_device`). Also used by
/// `DevicesAPI.block_device` (`devices.py:136-186`) to resolve the device's MAC before
/// blacklisting it.
pub const GET_DEVICE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices/{device_id}",
};

/// `PUT /2.3/networks/{network_id}/devices/{device_id}` — mutate a device's nickname.
///
/// **Must** use API version 2.3: version 2.2 accepts this `PUT`, returns `200 OK`, and
/// silently drops the write (`eero-api` issue #102; see `ApiVersion::V2_3`). Ported from
/// `eero-api src/eero/api/devices.py:44-65,111` (`DevicesAPI._update_device` via
/// `DevicesAPI.set_device_nickname`), which builds this exact URL against
/// `DEVICE_UPDATE_ENDPOINT` rather than the module's own (2.2) base.
pub const SET_DEVICE_NICKNAME: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_3,
    path: "networks/{network_id}/devices/{device_id}",
};

/// Alias of `SET_DEVICE_NICKNAME`: `DevicesAPI.pause_device` PUTs the same 2.3 device URL
/// with a `{"paused": bool}` body instead of `{"nickname": str}`.
///
/// Ported from `eero-api src/eero/api/devices.py:44-65,188` (`DevicesAPI.pause_device` via
/// `DevicesAPI._update_device`). See `SET_DEVICE_NICKNAME` for why 2.3 is required.
pub const PAUSE_DEVICE: Route = SET_DEVICE_NICKNAME;

// ------------------------------- diagnostics (`DiagnosticsAPI`) ------------------------

/// `GET /2.2/networks/{network_id}/diagnostics` — network diagnostics information.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:33` (`DiagnosticsAPI.get_diagnostics`).
pub const GET_DIAGNOSTICS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/diagnostics",
};

/// `POST /2.2/networks/{network_id}/diagnostics` — run network diagnostics.
///
/// Ported from `eero-api src/eero/api/diagnostics.py:56` (`DiagnosticsAPI.run_diagnostics`).
pub const RUN_DIAGNOSTICS: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/diagnostics",
};

// ------------------------------------ dns (`DnsAPI`) ------------------------------------

/// Alias of `GET_NETWORK`: `DnsAPI.get_dns_settings` reads DNS fields (`dns_caching`,
/// `custom_dns`, `ipv6_upstream`, ...) out of the full network object — same wire call.
///
/// Ported from `eero-api src/eero/api/dns.py:36` (`DnsAPI.get_dns_settings`).
pub const GET_DNS_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_dns_caching` PUTs `{"dns_caching": bool}`.
///
/// Ported from `eero-api src/eero/api/dns.py:59` (`DnsAPI.set_dns_caching`).
pub const SET_DNS_CACHING: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_custom_dns` PUTs `{"custom_dns": [..]}`
/// (truncated to at most 2 servers). Also the target of `DnsAPI.clear_custom_dns`
/// (`dns.py:126`), which delegates to `set_custom_dns([])` — no separate route needed there.
///
/// Ported from `eero-api src/eero/api/dns.py:89` (`DnsAPI.set_custom_dns`).
pub const SET_CUSTOM_DNS: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_dns_mode` PUTs a `custom_dns` list resolved
/// from a named preset (`"cloudflare"`, `"google"`, `"opendns"`, `"custom"`, `"auto"`).
///
/// Ported from `eero-api src/eero/api/dns.py:137` (`DnsAPI.set_dns_mode`).
pub const SET_DNS_MODE: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `DnsAPI.set_ipv6_dns` PUTs `{"ipv6_upstream": bool}`.
///
/// Ported from `eero-api src/eero/api/dns.py:186` (`DnsAPI.set_ipv6_dns`).
pub const SET_IPV6_DNS: Route = PUT_NETWORK_SETTINGS;

// ----------------------------------- eeros (`EerosAPI`) ---------------------------------

/// `GET /2.2/networks/{network_id}/eeros` — list Eero devices (nodes) on a network.
///
/// Ported from `eero-api src/eero/api/eeros.py:33` (`EerosAPI.get_eeros`).
pub const GET_EEROS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/eeros",
};

/// `GET /2.2/eeros/{eero_id}` — a single Eero node's details.
///
/// Not nested under `networks/` — `EerosAPI` addresses nodes by their own top-level
/// resource; `network_id` is accepted by the Python method but unused. Ported from
/// `eero-api src/eero/api/eeros.py:53` (`EerosAPI.get_eero`).
pub const GET_EERO: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}",
};

/// Alias of `GET_EERO`: `EerosAPI.get_led_status` reads `led_on`/`led_brightness` out of the
/// same Eero node object.
///
/// Ported from `eero-api src/eero/api/eeros.py:95` (`EerosAPI.get_led_status`).
pub const GET_LED_STATUS: Route = GET_EERO;

/// Alias of `GET_EERO`: `EerosAPI.get_nightlight` reads the `nightlight` field out of the
/// same Eero node object (Beacon devices only).
///
/// Ported from `eero-api src/eero/api/eeros.py:185` (`EerosAPI.get_nightlight`).
pub const GET_NIGHTLIGHT: Route = GET_EERO;

/// `POST /2.2/eeros/{eero_id}/reboot` — reboot a single Eero node.
///
/// Ported from `eero-api src/eero/api/eeros.py:74` (`EerosAPI.reboot_eero`).
pub const REBOOT_EERO: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}/reboot",
};

/// `PUT /2.2/eeros/{eero_id}` — mutate a single Eero node (LED, brightness, nightlight).
///
/// Unlike device nickname/pause writes, this stays on API version 2.2 — `EerosAPI` never
/// switches to `DEVICE_UPDATE_ENDPOINT`; only `DevicesAPI` does (`eero-api` issue #102 is
/// device-specific). Ported from `eero-api src/eero/api/eeros.py:118` (`EerosAPI.set_led`).
pub const SET_LED: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "eeros/{eero_id}",
};

/// Alias of `SET_LED`: `EerosAPI.set_led_brightness` PUTs the same node URL with
/// `{"led_brightness": 0..=100}` (clamped). Also the target of
/// `EerosAPI.set_nightlight_brightness` (`eeros.py:282`), which delegates through
/// `set_nightlight` — no separate route needed there.
///
/// Ported from `eero-api src/eero/api/eeros.py:150` (`EerosAPI.set_led_brightness`).
pub const SET_LED_BRIGHTNESS: Route = SET_LED;

/// Alias of `SET_LED`: `EerosAPI.set_nightlight` PUTs the same node URL with a nested
/// `{"nightlight": {...}}` body. Also the target of `EerosAPI.set_nightlight_schedule`
/// (`eeros.py:302`), which delegates through `set_nightlight` — no separate route needed
/// there.
///
/// Ported from `eero-api src/eero/api/eeros.py:209` (`EerosAPI.set_nightlight`).
pub const SET_NIGHTLIGHT: Route = SET_LED;

// ---------------------------------- forwards (`ForwardsAPI`) ----------------------------

/// `GET /2.2/networks/{network_id}/forwards` — list port forwards.
///
/// Ported from `eero-api src/eero/api/forwards.py:33` (`ForwardsAPI.get_forwards`).
pub const GET_FORWARDS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards",
};

/// `POST /2.2/networks/{network_id}/forwards` — create a port forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:56` (`ForwardsAPI.create_forward`).
pub const CREATE_FORWARD: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards",
};

/// `DELETE /2.2/networks/{network_id}/forwards/{forward_id}` — delete a port forward.
///
/// Ported from `eero-api src/eero/api/forwards.py:81` (`ForwardsAPI.delete_forward`).
pub const DELETE_FORWARD: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/forwards/{forward_id}",
};

// ---------------------------------- insights (`InsightsAPI`) ----------------------------

/// `GET /2.2/networks/{network_id}/insights` — query insights time-series data.
///
/// The four query parameters (`start`, `end`, `insight_type`, `cadence`) are all required by
/// the server, but they are query-string parameters, not part of the path template — the
/// caller attaches them to the request at call time (e.g. via `reqwest::RequestBuilder::query`),
/// not through `Route::render`. Ported from `eero-api src/eero/api/insights.py:36`
/// (`InsightsAPI.get_insights`).
pub const GET_INSIGHTS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/insights",
};

/// `POST /2.2/networks/{network_id}/insights` — run insights analysis.
///
/// Ported from `eero-api src/eero/api/insights.py:115` (`InsightsAPI.run_insights`).
pub const RUN_INSIGHTS: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/insights",
};

// --------------------------------- networks (`NetworksAPI`) -----------------------------

/// `GET /2.2/networks` — list every network on the account.
///
/// Ported from `eero-api src/eero/api/networks.py:33` (`NetworksAPI.get_networks`).
pub const GET_NETWORKS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks",
};

/// `GET /2.2/networks/{network_id}` — a single network's full object.
///
/// Shared by four Python methods that all read fields out of the same full network object
/// rather than a dedicated sub-resource: `NetworksAPI.get_network`,
/// `DnsAPI.get_dns_settings` (`GET_DNS_SETTINGS`), `SecurityAPI.get_security_settings`
/// (`GET_SECURITY_SETTINGS`), `SqmAPI.get_sqm_settings` (`GET_SQM_SETTINGS`), and
/// `NetworksAPI.get_premium_status` (`GET_PREMIUM_STATUS`). Ported from
/// `eero-api src/eero/api/networks.py:51` (`NetworksAPI.get_network`).
pub const GET_NETWORK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}",
};

/// Alias of `GET_NETWORK`: `NetworksAPI.get_premium_status` reads Eero Plus/Secure fields
/// out of the same full network object.
///
/// Ported from `eero-api src/eero/api/networks.py:159` (`NetworksAPI.get_premium_status`).
pub const GET_PREMIUM_STATUS: Route = GET_NETWORK;

/// `PUT /2.2/networks/{network_id}/guestnetwork` — enable/disable/configure the guest
/// network.
///
/// Ported from `eero-api src/eero/api/networks.py:71` (`NetworksAPI.set_guest_network`).
pub const SET_GUEST_NETWORK: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/guestnetwork",
};

/// `POST /2.2/networks/{network_id}/speedtest` — run a speed test on the network.
///
/// Ported from `eero-api src/eero/api/networks.py:111` (`NetworksAPI.run_speed_test`).
pub const RUN_SPEED_TEST: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/speedtest",
};

/// `POST /2.2/networks/{network_id}/reboot` — reboot every Eero node on the network.
///
/// Ported from `eero-api src/eero/api/networks.py:134` (`NetworksAPI.reboot_network`).
pub const REBOOT_NETWORK: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reboot",
};

/// `PUT /2.2/networks/{network_id}/settings` — the network-wide settings resource.
///
/// The single wire endpoint behind well over a dozen Python setters spread across four
/// modules (`NetworksAPI.set_network_name` via `SET_NETWORK_NAME`; `DnsAPI`'s
/// `SET_DNS_CACHING`/`SET_CUSTOM_DNS`/`SET_DNS_MODE`/`SET_IPV6_DNS`; `SecurityAPI`'s
/// `SET_WPA3`/`SET_BAND_STEERING`/`SET_UPNP`/`SET_IPV6`/`SET_THREAD`/`CONFIGURE_SECURITY`;
/// `SqmAPI`'s `SET_SQM_ENABLED`/`SET_SQM_BANDWIDTH`/`CONFIGURE_SQM`/`SET_SQM_AUTO`) — each
/// PUTs a different JSON key onto the same settings object. Distinct from `GET_SETTINGS`,
/// which reads this same resource via `SettingsAPI.get_settings`. Ported from
/// `eero-api src/eero/api/networks.py:182` (`NetworksAPI.set_network_name`), the first
/// setter of this resource in the port plan's endpoint catalogue.
pub const PUT_NETWORK_SETTINGS: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/settings",
};

/// Alias of `PUT_NETWORK_SETTINGS`: `NetworksAPI.set_network_name` PUTs `{"name": str}`.
///
/// Ported from `eero-api src/eero/api/networks.py:182` (`NetworksAPI.set_network_name`).
pub const SET_NETWORK_NAME: Route = PUT_NETWORK_SETTINGS;

// --------------------------------- ouicheck (`OUICheckAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/ouicheck` — OUI (vendor MAC prefix) check results.
///
/// Ported from `eero-api src/eero/api/ouicheck.py:33` (`OUICheckAPI.get_ouicheck`).
pub const GET_OUICHECK: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ouicheck",
};

/// `POST /2.2/networks/{network_id}/ouicheck` — run an OUI check.
///
/// Ported from `eero-api src/eero/api/ouicheck.py:56` (`OUICheckAPI.run_ouicheck`).
pub const RUN_OUICHECK: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/ouicheck",
};

// --------------------------------- password (`PasswordAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/password` — the network's Wi-Fi password.
///
/// Sensitive: callers must route the response through the same secure-logging discipline as
/// `eero-api`'s `get_secure_logger` (`password.py:14`) — never log the raw envelope at `debug`
/// or below. Ported from `eero-api src/eero/api/password.py:33` (`PasswordAPI.get_password`).
pub const GET_PASSWORD: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/password",
};

// --------------------------------- profiles (`ProfilesAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/profiles` — list profiles on a network.
///
/// Ported from `eero-api src/eero/api/profiles.py:33` (`ProfilesAPI.get_profiles`).
pub const GET_PROFILES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles",
};

/// `POST /2.2/networks/{network_id}/profiles` — create a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:336` (`ProfilesAPI.create_profile`).
pub const CREATE_PROFILE: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles",
};

/// `GET /2.2/networks/{network_id}/profiles/{profile_id}` — a single profile's full object.
///
/// Shared by four Python methods that all read fields out of the same full profile object:
/// `ProfilesAPI.get_profile`, `ProfilesAPI.get_profile_devices` (`GET_PROFILE_DEVICES`),
/// `ProfilesAPI.get_blocked_applications` (`GET_BLOCKED_APPLICATIONS`), and
/// `ScheduleAPI.get_profile_schedule` (`GET_PROFILE_SCHEDULE`). Ported from
/// `eero-api src/eero/api/profiles.py:53` (`ProfilesAPI.get_profile`).
pub const GET_PROFILE: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};

/// `PUT /2.2/networks/{network_id}/profiles/{profile_id}` — the profile mutation endpoint.
///
/// The single wire endpoint behind seven Python setters across `ProfilesAPI` and
/// `ScheduleAPI` (`PAUSE_PROFILE`, `SET_PROFILE_DEVICES`, `UPDATE_PROFILE_CONTENT_FILTER`,
/// `UPDATE_PROFILE_BLOCK_LIST`, `SET_BLOCKED_APPLICATIONS`, `RENAME_PROFILE`,
/// `SET_PROFILE_SCHEDULE`) — each PUTs a different JSON key onto the same profile object.
/// Ported from `eero-api src/eero/api/profiles.py:77` (`ProfilesAPI.pause_profile`), the
/// first setter of this resource in the port plan's endpoint catalogue.
pub const PUT_PROFILE: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};

/// Alias of `PUT_PROFILE`: `ProfilesAPI.pause_profile` PUTs `{"paused": bool}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:77` (`ProfilesAPI.pause_profile`).
pub const PAUSE_PROFILE: Route = PUT_PROFILE;

/// Alias of `GET_PROFILE`: `ProfilesAPI.get_profile_devices` reads the `devices` field out
/// of the same full profile object.
///
/// Ported from `eero-api src/eero/api/profiles.py:104` (`ProfilesAPI.get_profile_devices`).
pub const GET_PROFILE_DEVICES: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.set_profile_devices` PUTs
/// `{"devices": [{"url": ...}, ...]}`, replacing all device assignments.
///
/// Ported from `eero-api src/eero/api/profiles.py:130` (`ProfilesAPI.set_profile_devices`).
pub const SET_PROFILE_DEVICES: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.update_profile_content_filter` PUTs
/// `{"content_filter": {...}}` (server-side key allowlist applied client-side first).
///
/// Ported from `eero-api src/eero/api/profiles.py:179`
/// (`ProfilesAPI.update_profile_content_filter`).
pub const UPDATE_PROFILE_CONTENT_FILTER: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.update_profile_block_list` PUTs
/// `{"custom_block_list": [..]}` or `{"custom_allow_list": [..]}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:227`
/// (`ProfilesAPI.update_profile_block_list`).
pub const UPDATE_PROFILE_BLOCK_LIST: Route = PUT_PROFILE;

/// Alias of `GET_PROFILE`: `ProfilesAPI.get_blocked_applications` reads the
/// `blocked_applications` (or `premium_dns.blocked_applications`) field out of the same full
/// profile object.
///
/// Ported from `eero-api src/eero/api/profiles.py:267`
/// (`ProfilesAPI.get_blocked_applications`).
pub const GET_BLOCKED_APPLICATIONS: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.set_blocked_applications` PUTs
/// `{"blocked_applications": [..]}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:294`
/// (`ProfilesAPI.set_blocked_applications`).
pub const SET_BLOCKED_APPLICATIONS: Route = PUT_PROFILE;

/// Alias of `PUT_PROFILE`: `ProfilesAPI.rename_profile` PUTs `{"name": str}`.
///
/// Ported from `eero-api src/eero/api/profiles.py:364` (`ProfilesAPI.rename_profile`).
pub const RENAME_PROFILE: Route = PUT_PROFILE;

/// `DELETE /2.2/networks/{network_id}/profiles/{profile_id}` — delete a profile.
///
/// Ported from `eero-api src/eero/api/profiles.py:391` (`ProfilesAPI.delete_profile`).
pub const DELETE_PROFILE: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/profiles/{profile_id}",
};

// ------------------------------ reservations (`ReservationsAPI`) ------------------------

/// `GET /2.2/networks/{network_id}/reservations` — list DHCP reservations.
///
/// Ported from `eero-api src/eero/api/reservations.py:33`
/// (`ReservationsAPI.get_reservations`).
pub const GET_RESERVATIONS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations",
};

/// `POST /2.2/networks/{network_id}/reservations` — create a DHCP reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:56`
/// (`ReservationsAPI.create_reservation`).
pub const CREATE_RESERVATION: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations",
};

/// `PUT /2.2/networks/{network_id}/reservations/{reservation_id}` — update a DHCP
/// reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:83`
/// (`ReservationsAPI.update_reservation`).
pub const UPDATE_RESERVATION: Route = Route {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations/{reservation_id}",
};

/// `DELETE /2.2/networks/{network_id}/reservations/{reservation_id}` — delete a DHCP
/// reservation.
///
/// Ported from `eero-api src/eero/api/reservations.py:116`
/// (`ReservationsAPI.delete_reservation`).
pub const DELETE_RESERVATION: Route = Route {
    method: Method::DELETE,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/reservations/{reservation_id}",
};

// ----------------------------------- routing (`RoutingAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/routing` — routing information for a network.
///
/// Ported from `eero-api src/eero/api/routing.py:33` (`RoutingAPI.get_routing`).
pub const GET_ROUTING: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/routing",
};

// ---------------------------------- schedule (`ScheduleAPI`) ----------------------------

/// Alias of `GET_PROFILE`: `ScheduleAPI.get_profile_schedule` reads the `schedule` field out
/// of the same full profile object.
///
/// Ported from `eero-api src/eero/api/schedule.py:36` (`ScheduleAPI.get_profile_schedule`).
pub const GET_PROFILE_SCHEDULE: Route = GET_PROFILE;

/// Alias of `PUT_PROFILE`: `ScheduleAPI.set_profile_schedule` PUTs
/// `{"schedule": [time_block, ...]}`. Also the target of `ScheduleAPI.clear_profile_schedule`
/// (`schedule.py:101`, delegates with `[]`), `ScheduleAPI.enable_bedtime` (`schedule.py:113`,
/// delegates with a single `{"type": "bedtime", ...}` block),
/// `ScheduleAPI.set_weekday_bedtime` (`schedule.py:169`) and
/// `ScheduleAPI.set_weekend_bedtime` (`schedule.py:190`) — all four delegate to
/// `set_profile_schedule`, so none needs a separate route.
///
/// Ported from `eero-api src/eero/api/schedule.py:62` (`ScheduleAPI.set_profile_schedule`).
pub const SET_PROFILE_SCHEDULE: Route = PUT_PROFILE;

// ---------------------------------- security (`SecurityAPI`) ----------------------------

/// Alias of `GET_NETWORK`: `SecurityAPI.get_security_settings` reads security fields
/// (`wpa3`, `band_steering`, `upnp`, `ipv6_upstream`, `ipv6_downstream`, `thread`, ...) out
/// of the same full network object.
///
/// Ported from `eero-api src/eero/api/security.py:36`
/// (`SecurityAPI.get_security_settings`).
pub const GET_SECURITY_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_wpa3` PUTs `{"wpa3": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:59` (`SecurityAPI.set_wpa3`).
pub const SET_WPA3: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_band_steering` PUTs
/// `{"band_steering": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:92` (`SecurityAPI.set_band_steering`).
pub const SET_BAND_STEERING: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_upnp` PUTs `{"upnp": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:125` (`SecurityAPI.set_upnp`).
pub const SET_UPNP: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_ipv6` PUTs
/// `{"ipv6_upstream": bool, "ipv6_downstream": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:158` (`SecurityAPI.set_ipv6`).
pub const SET_IPV6: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.set_thread` PUTs `{"thread": bool}`.
///
/// Ported from `eero-api src/eero/api/security.py:191` (`SecurityAPI.set_thread`).
pub const SET_THREAD: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SecurityAPI.configure_security` PUTs a partial union of
/// `wpa3`/`band_steering`/`upnp`/`ipv6_upstream`+`ipv6_downstream`/`thread`.
///
/// Ported from `eero-api src/eero/api/security.py:224` (`SecurityAPI.configure_security`).
pub const CONFIGURE_SECURITY: Route = PUT_NETWORK_SETTINGS;

// ---------------------------------- settings (`SettingsAPI`) ----------------------------

/// `GET /2.2/networks/{network_id}/settings` — read the network-wide settings resource.
///
/// Distinct from `PUT_NETWORK_SETTINGS`: same resource path, opposite verb. Ported from
/// `eero-api src/eero/api/settings.py:33` (`SettingsAPI.get_settings`).
pub const GET_SETTINGS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/settings",
};

// -------------------------------------- sqm (`SqmAPI`) -----------------------------------

/// Alias of `GET_NETWORK`: `SqmAPI.get_sqm_settings` reads SQM/QoS fields out of the same
/// full network object.
///
/// Ported from `eero-api src/eero/api/sqm.py:36` (`SqmAPI.get_sqm_settings`).
pub const GET_SQM_SETTINGS: Route = GET_NETWORK;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_enabled` PUTs `{"sqm": bool}` (flat,
/// unlike the other SQM setters below).
///
/// Ported from `eero-api src/eero/api/sqm.py:59` (`SqmAPI.set_sqm_enabled`).
pub const SET_SQM_ENABLED: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_bandwidth` PUTs a nested
/// `{"sqm": {"enabled": true, "upload_bandwidth"?, "download_bandwidth"?}}` — payload shape
/// unverified against a live account (`sqm.py:128` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:89` (`SqmAPI.set_sqm_bandwidth`).
pub const SET_SQM_BANDWIDTH: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.configure_sqm` PUTs a nested
/// `{"sqm": {"enabled": bool, ...}}` — payload shape unverified against a live account
/// (`sqm.py:173` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:137` (`SqmAPI.configure_sqm`).
pub const CONFIGURE_SQM: Route = PUT_NETWORK_SETTINGS;

/// Alias of `PUT_NETWORK_SETTINGS`: `SqmAPI.set_sqm_auto` PUTs
/// `{"sqm": {"enabled": true, "mode": "auto"}}` — payload shape unverified against a live
/// account (`sqm.py:197` `TODO`).
///
/// Ported from `eero-api src/eero/api/sqm.py:182` (`SqmAPI.set_sqm_auto`).
pub const SET_SQM_AUTO: Route = PUT_NETWORK_SETTINGS;

// --------------------------------- support (`SupportAPI`) -------------------------------

/// `GET /2.2/networks/{network_id}/support` — support information for a network.
///
/// Ported from `eero-api src/eero/api/support.py:33` (`SupportAPI.get_support`).
pub const GET_SUPPORT: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/support",
};

/// `POST /2.2/networks/{network_id}/support` — file a support request.
///
/// Ported from `eero-api src/eero/api/support.py:56` (`SupportAPI.request_support`).
pub const REQUEST_SUPPORT: Route = Route {
    method: Method::POST,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/support",
};

// ----------------------------------- thread (`ThreadAPI`) -------------------------------

/// `GET /2.2/networks/{network_id}/thread` — Thread (smart-home mesh) status.
///
/// Ported from `eero-api src/eero/api/thread.py:33` (`ThreadAPI.get_thread`).
pub const GET_THREAD: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/thread",
};

// ---------------------------------- transfer (`TransferAPI`) ----------------------------

/// `GET /2.2/networks/{network_id}/transfer` — network-wide transfer statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:33` (`TransferAPI.get_transfer_stats`
/// called with `device_id=None`).
pub const GET_TRANSFER_STATS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/transfer",
};

/// `GET /2.2/networks/{network_id}/devices/{device_id}/transfer` — one device's transfer
/// statistics.
///
/// Ported from `eero-api src/eero/api/transfer.py:33` (`TransferAPI.get_transfer_stats`
/// called with a non-`None` `device_id`).
pub const GET_DEVICE_TRANSFER_STATS: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/devices/{device_id}/transfer",
};

// ----------------------------------- updates (`UpdatesAPI`) -----------------------------

/// `GET /2.2/networks/{network_id}/updates` — available firmware/software updates.
///
/// Ported from `eero-api src/eero/api/updates.py:33` (`UpdatesAPI.get_updates`).
pub const GET_UPDATES: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/updates",
};

#[cfg(test)]
mod tests {
    use super::{ApiVersion, RenderError, Route};
    use reqwest::Method;

    // ----------------------------- ApiVersion::base_url -----------------------------

    #[test]
    fn v2_2_base_url_is_the_2_2_host() {
        assert_eq!(ApiVersion::V2_2.base_url(), "https://api-user.e2ro.com/2.2");
    }

    #[test]
    fn v2_3_base_url_is_the_2_3_host() {
        assert_eq!(ApiVersion::V2_3.base_url(), "https://api-user.e2ro.com/2.3");
    }

    // ------------------------------- required routes ---------------------------------

    #[test]
    fn required_auth_and_account_routes_have_the_expected_verb_version_and_path() {
        use super::{
            ACCOUNT, ACCOUNT_REFRESH, LOGIN, LOGIN_REFRESH, LOGIN_RESEND, LOGIN_VERIFY, LOGOUT,
        };

        assert_eq!(LOGIN.method, Method::POST);
        assert_eq!(LOGIN.version, ApiVersion::V2_2);
        assert_eq!(LOGIN.path, "login");

        assert_eq!(LOGIN_VERIFY.method, Method::POST);
        assert_eq!(LOGIN_VERIFY.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_VERIFY.path, "login/verify");

        assert_eq!(LOGIN_RESEND.method, Method::POST);
        assert_eq!(LOGIN_RESEND.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_RESEND.path, "login/resend");

        assert_eq!(LOGOUT.method, Method::POST);
        assert_eq!(LOGOUT.version, ApiVersion::V2_2);
        assert_eq!(LOGOUT.path, "logout");

        assert_eq!(LOGIN_REFRESH.method, Method::POST);
        assert_eq!(LOGIN_REFRESH.version, ApiVersion::V2_2);
        assert_eq!(LOGIN_REFRESH.path, "login/refresh");

        assert_eq!(ACCOUNT_REFRESH.method, Method::POST);
        assert_eq!(ACCOUNT_REFRESH.version, ApiVersion::V2_2);
        assert_eq!(ACCOUNT_REFRESH.path, "account/refresh");

        assert_eq!(ACCOUNT.method, Method::GET);
        assert_eq!(ACCOUNT.version, ApiVersion::V2_2);
        assert_eq!(ACCOUNT.path, "account");
    }

    #[test]
    fn login_and_verify_render_to_the_expected_absolute_urls() {
        use super::{LOGIN, LOGIN_VERIFY};

        assert_eq!(
            LOGIN.render(&[]).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login"
        );
        assert_eq!(
            LOGIN_VERIFY.render(&[]).unwrap().as_str(),
            "https://api-user.e2ro.com/2.2/login/verify"
        );
    }

    // ------------------------------ placeholder rendering ------------------------------

    #[test]
    fn placeholder_is_substituted_with_the_supplied_value() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let url = route
            .render(&[("network_id", "123")])
            .expect("all placeholders supplied");
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.2/networks/123/devices"
        );
    }

    #[test]
    fn multiple_placeholders_are_each_substituted_independently() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_3,
            path: "networks/{network_id}/devices/{device_id}",
        };

        let url = route
            .render(&[("network_id", "123"), ("device_id", "ab:cd")])
            .expect("all placeholders supplied");
        assert_eq!(
            url.as_str(),
            "https://api-user.e2ro.com/2.3/networks/123/devices/ab:cd"
        );
    }

    #[test]
    fn missing_placeholder_value_is_a_render_error_not_a_panic_or_silent_gap() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let err = route.render(&[]).unwrap_err();
        assert_eq!(
            err,
            RenderError::MissingPlaceholder("network_id".to_owned())
        );
    }

    #[test]
    fn extra_unused_params_are_ignored() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "account",
        };

        let url = route
            .render(&[("network_id", "123"), ("unused", "x")])
            .expect("no placeholders required, extra params are harmless");
        assert_eq!(url.as_str(), "https://api-user.e2ro.com/2.2/account");
    }

    // ------------------------------- percent-encoding -----------------------------------

    #[test]
    fn slash_and_question_mark_in_a_placeholder_value_cannot_escape_their_segment() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/devices",
        };

        let url = route
            .render(&[("network_id", "abc/def?x=1")])
            .expect("value is escaped, not rejected");

        // The hostile value is confined to a single, opaque path segment: no extra "/devices"
        // path segment was introduced, and no query string was introduced either.
        assert_eq!(url.path(), "/2.2/networks/abc%2Fdef%3Fx=1/devices");
        assert_eq!(url.query(), None);
        let segments: Vec<&str> = url.path_segments().unwrap().collect();
        assert_eq!(segments, ["2.2", "networks", "abc%2Fdef%3Fx=1", "devices"]);
    }

    #[test]
    fn percent_sign_in_a_placeholder_value_is_itself_escaped() {
        let route = Route {
            method: Method::GET,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}",
        };

        let url = route.render(&[("network_id", "100%")]).unwrap();
        assert_eq!(url.path(), "/2.2/networks/100%25");
    }

    // -------------------------- exhaustive route inventory -----------------------------

    use super::{
        ACCOUNT, ACCOUNT_REFRESH, ADD_TO_BLACKLIST, CONFIGURE_BACKUP_NETWORK, CONFIGURE_SECURITY,
        CONFIGURE_SQM, CREATE_BURST_REPORTER, CREATE_FORWARD, CREATE_PROFILE, CREATE_RESERVATION,
        DELETE_FORWARD, DELETE_PROFILE, DELETE_RESERVATION, GET_AC_COMPAT, GET_BACKUP_NETWORK,
        GET_BACKUP_STATUS, GET_BLACKLIST, GET_BLOCKED_APPLICATIONS, GET_BURST_REPORTERS,
        GET_DATA_USAGE, GET_DATA_USAGE_RESOURCE, GET_DEVICE, GET_DEVICE_TRANSFER_STATS,
        GET_DEVICES, GET_DIAGNOSTICS, GET_DNS_SETTINGS, GET_EERO, GET_EEROS, GET_FORWARDS,
        GET_INSIGHTS, GET_LED_STATUS, GET_NETWORK, GET_NETWORKS, GET_NIGHTLIGHT, GET_OUICHECK,
        GET_PASSWORD, GET_PREMIUM_STATUS, GET_PROFILE, GET_PROFILE_DEVICES, GET_PROFILE_SCHEDULE,
        GET_PROFILES, GET_RESERVATIONS, GET_ROUTING, GET_SECURITY_SETTINGS, GET_SETTINGS,
        GET_SQM_SETTINGS, GET_SUPPORT, GET_THREAD, GET_TRANSFER_STATS, GET_UPDATES, LOGIN,
        LOGIN_REFRESH, LOGIN_RESEND, LOGIN_VERIFY, LOGOUT, PAUSE_DEVICE, PAUSE_PROFILE,
        PUT_NETWORK_SETTINGS, PUT_PROFILE, REBOOT_EERO, REBOOT_NETWORK, REMOVE_FROM_BLACKLIST,
        RENAME_PROFILE, REQUEST_SUPPORT, RUN_DIAGNOSTICS, RUN_INSIGHTS, RUN_OUICHECK,
        RUN_SPEED_TEST, SET_BACKUP_NETWORK, SET_BAND_STEERING, SET_BLOCKED_APPLICATIONS,
        SET_CUSTOM_DNS, SET_DEVICE_NICKNAME, SET_DNS_CACHING, SET_DNS_MODE, SET_GUEST_NETWORK,
        SET_IPV6, SET_IPV6_DNS, SET_LED, SET_LED_BRIGHTNESS, SET_NETWORK_NAME, SET_NIGHTLIGHT,
        SET_PROFILE_DEVICES, SET_PROFILE_SCHEDULE, SET_SQM_AUTO, SET_SQM_BANDWIDTH,
        SET_SQM_ENABLED, SET_THREAD, SET_UPNP, SET_WPA3, UPDATE_PROFILE_BLOCK_LIST,
        UPDATE_PROFILE_CONTENT_FILTER, UPDATE_RESERVATION,
    };
    /// One row per `Route` constant declared in this module. This is the drift tripwire for
    /// the whole crate: if Eero ever moves an endpoint, exactly one row here should fail and
    /// point straight at the constant to fix.
    struct Case {
        name: &'static str,
        route: &'static Route,
        method: Method,
        version: ApiVersion,
        params: &'static [(&'static str, &'static str)],
        rendered: &'static str,
    }

    const CASES: &[Case] = &[
        Case {
            name: "LOGIN",
            route: &LOGIN,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login",
        },
        Case {
            name: "LOGIN_VERIFY",
            route: &LOGIN_VERIFY,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/verify",
        },
        Case {
            name: "LOGIN_RESEND",
            route: &LOGIN_RESEND,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/resend",
        },
        Case {
            name: "LOGOUT",
            route: &LOGOUT,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/logout",
        },
        Case {
            name: "LOGIN_REFRESH",
            route: &LOGIN_REFRESH,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/login/refresh",
        },
        Case {
            name: "ACCOUNT_REFRESH",
            route: &ACCOUNT_REFRESH,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/account/refresh",
        },
        Case {
            name: "ACCOUNT",
            route: &ACCOUNT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/account",
        },
        Case {
            name: "GET_AC_COMPAT",
            route: &GET_AC_COMPAT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ac_compat",
        },
        Case {
            name: "GET_BACKUP_NETWORK",
            route: &GET_BACKUP_NETWORK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "GET_BACKUP_STATUS",
            route: &GET_BACKUP_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup/status",
        },
        Case {
            name: "SET_BACKUP_NETWORK",
            route: &SET_BACKUP_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "CONFIGURE_BACKUP_NETWORK",
            route: &CONFIGURE_BACKUP_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/backup",
        },
        Case {
            name: "GET_BLACKLIST",
            route: &GET_BLACKLIST,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist",
        },
        Case {
            name: "ADD_TO_BLACKLIST",
            route: &ADD_TO_BLACKLIST,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist",
        },
        Case {
            name: "REMOVE_FROM_BLACKLIST",
            route: &REMOVE_FROM_BLACKLIST,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("mac_or_device_id", "aabbccddeeff")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/blacklist/aabbccddeeff",
        },
        Case {
            name: "GET_BURST_REPORTERS",
            route: &GET_BURST_REPORTERS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/burst_reporters",
        },
        Case {
            name: "CREATE_BURST_REPORTER",
            route: &CREATE_BURST_REPORTER,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/burst_reporters",
        },
        Case {
            name: "GET_DATA_USAGE",
            route: &GET_DATA_USAGE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/data_usage",
        },
        Case {
            name: "GET_DATA_USAGE_RESOURCE",
            route: &GET_DATA_USAGE_RESOURCE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("resource", "devices")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/data_usage/devices",
        },
        Case {
            name: "GET_DEVICES",
            route: &GET_DEVICES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices",
        },
        Case {
            name: "GET_DEVICE",
            route: &GET_DEVICE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices/dev1",
        },
        Case {
            name: "SET_DEVICE_NICKNAME",
            route: &SET_DEVICE_NICKNAME,
            method: Method::PUT,
            version: ApiVersion::V2_3,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.3/networks/100/devices/dev1",
        },
        Case {
            name: "PAUSE_DEVICE",
            route: &PAUSE_DEVICE,
            method: Method::PUT,
            version: ApiVersion::V2_3,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.3/networks/100/devices/dev1",
        },
        Case {
            name: "GET_DIAGNOSTICS",
            route: &GET_DIAGNOSTICS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/diagnostics",
        },
        Case {
            name: "RUN_DIAGNOSTICS",
            route: &RUN_DIAGNOSTICS,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/diagnostics",
        },
        Case {
            name: "GET_DNS_SETTINGS",
            route: &GET_DNS_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_DNS_CACHING",
            route: &SET_DNS_CACHING,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_CUSTOM_DNS",
            route: &SET_CUSTOM_DNS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_DNS_MODE",
            route: &SET_DNS_MODE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_IPV6_DNS",
            route: &SET_IPV6_DNS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_EEROS",
            route: &GET_EEROS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/eeros",
        },
        Case {
            name: "GET_EERO",
            route: &GET_EERO,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_LED_STATUS",
            route: &GET_LED_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_NIGHTLIGHT",
            route: &GET_NIGHTLIGHT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "REBOOT_EERO",
            route: &REBOOT_EERO,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1/reboot",
        },
        Case {
            name: "SET_LED",
            route: &SET_LED,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "SET_LED_BRIGHTNESS",
            route: &SET_LED_BRIGHTNESS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "SET_NIGHTLIGHT",
            route: &SET_NIGHTLIGHT,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("eero_id", "eero1")],
            rendered: "https://api-user.e2ro.com/2.2/eeros/eero1",
        },
        Case {
            name: "GET_FORWARDS",
            route: &GET_FORWARDS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards",
        },
        Case {
            name: "CREATE_FORWARD",
            route: &CREATE_FORWARD,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards",
        },
        Case {
            name: "DELETE_FORWARD",
            route: &DELETE_FORWARD,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("forward_id", "fwd1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/forwards/fwd1",
        },
        Case {
            name: "GET_INSIGHTS",
            route: &GET_INSIGHTS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/insights",
        },
        Case {
            name: "RUN_INSIGHTS",
            route: &RUN_INSIGHTS,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/insights",
        },
        Case {
            name: "GET_NETWORKS",
            route: &GET_NETWORKS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[],
            rendered: "https://api-user.e2ro.com/2.2/networks",
        },
        Case {
            name: "GET_NETWORK",
            route: &GET_NETWORK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "GET_PREMIUM_STATUS",
            route: &GET_PREMIUM_STATUS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_GUEST_NETWORK",
            route: &SET_GUEST_NETWORK,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/guestnetwork",
        },
        Case {
            name: "RUN_SPEED_TEST",
            route: &RUN_SPEED_TEST,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/speedtest",
        },
        Case {
            name: "REBOOT_NETWORK",
            route: &REBOOT_NETWORK,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reboot",
        },
        Case {
            name: "PUT_NETWORK_SETTINGS",
            route: &PUT_NETWORK_SETTINGS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_NETWORK_NAME",
            route: &SET_NETWORK_NAME,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_OUICHECK",
            route: &GET_OUICHECK,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ouicheck",
        },
        Case {
            name: "RUN_OUICHECK",
            route: &RUN_OUICHECK,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/ouicheck",
        },
        Case {
            name: "GET_PASSWORD",
            route: &GET_PASSWORD,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/password",
        },
        Case {
            name: "GET_PROFILES",
            route: &GET_PROFILES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles",
        },
        Case {
            name: "CREATE_PROFILE",
            route: &CREATE_PROFILE,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles",
        },
        Case {
            name: "GET_PROFILE",
            route: &GET_PROFILE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "PUT_PROFILE",
            route: &PUT_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "PAUSE_PROFILE",
            route: &PAUSE_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_PROFILE_DEVICES",
            route: &GET_PROFILE_DEVICES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_PROFILE_DEVICES",
            route: &SET_PROFILE_DEVICES,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "UPDATE_PROFILE_CONTENT_FILTER",
            route: &UPDATE_PROFILE_CONTENT_FILTER,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "UPDATE_PROFILE_BLOCK_LIST",
            route: &UPDATE_PROFILE_BLOCK_LIST,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_BLOCKED_APPLICATIONS",
            route: &GET_BLOCKED_APPLICATIONS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_BLOCKED_APPLICATIONS",
            route: &SET_BLOCKED_APPLICATIONS,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "RENAME_PROFILE",
            route: &RENAME_PROFILE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "DELETE_PROFILE",
            route: &DELETE_PROFILE,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_RESERVATIONS",
            route: &GET_RESERVATIONS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations",
        },
        Case {
            name: "CREATE_RESERVATION",
            route: &CREATE_RESERVATION,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations",
        },
        Case {
            name: "UPDATE_RESERVATION",
            route: &UPDATE_RESERVATION,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("reservation_id", "res1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations/res1",
        },
        Case {
            name: "DELETE_RESERVATION",
            route: &DELETE_RESERVATION,
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("reservation_id", "res1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/reservations/res1",
        },
        Case {
            name: "GET_ROUTING",
            route: &GET_ROUTING,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/routing",
        },
        Case {
            name: "GET_PROFILE_SCHEDULE",
            route: &GET_PROFILE_SCHEDULE,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "SET_PROFILE_SCHEDULE",
            route: &SET_PROFILE_SCHEDULE,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("profile_id", "prof1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/profiles/prof1",
        },
        Case {
            name: "GET_SECURITY_SETTINGS",
            route: &GET_SECURITY_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_WPA3",
            route: &SET_WPA3,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_BAND_STEERING",
            route: &SET_BAND_STEERING,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_UPNP",
            route: &SET_UPNP,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_IPV6",
            route: &SET_IPV6,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_THREAD",
            route: &SET_THREAD,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "CONFIGURE_SECURITY",
            route: &CONFIGURE_SECURITY,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_SETTINGS",
            route: &GET_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_SQM_SETTINGS",
            route: &GET_SQM_SETTINGS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100",
        },
        Case {
            name: "SET_SQM_ENABLED",
            route: &SET_SQM_ENABLED,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_SQM_BANDWIDTH",
            route: &SET_SQM_BANDWIDTH,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "CONFIGURE_SQM",
            route: &CONFIGURE_SQM,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "SET_SQM_AUTO",
            route: &SET_SQM_AUTO,
            method: Method::PUT,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/settings",
        },
        Case {
            name: "GET_SUPPORT",
            route: &GET_SUPPORT,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/support",
        },
        Case {
            name: "REQUEST_SUPPORT",
            route: &REQUEST_SUPPORT,
            method: Method::POST,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/support",
        },
        Case {
            name: "GET_THREAD",
            route: &GET_THREAD,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/thread",
        },
        Case {
            name: "GET_TRANSFER_STATS",
            route: &GET_TRANSFER_STATS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/transfer",
        },
        Case {
            name: "GET_DEVICE_TRANSFER_STATS",
            route: &GET_DEVICE_TRANSFER_STATS,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100"), ("device_id", "dev1")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/devices/dev1/transfer",
        },
        Case {
            name: "GET_UPDATES",
            route: &GET_UPDATES,
            method: Method::GET,
            version: ApiVersion::V2_2,
            params: &[("network_id", "100")],
            rendered: "https://api-user.e2ro.com/2.2/networks/100/updates",
        },
    ];

    #[test]
    fn every_domain_route_constant_has_the_expected_verb_version_and_rendered_path() {
        for case in CASES {
            assert_eq!(
                case.route.method, case.method,
                "{}: unexpected method",
                case.name
            );
            assert_eq!(
                case.route.version, case.version,
                "{}: unexpected version",
                case.name
            );
            let url = case
                .route
                .render(case.params)
                .unwrap_or_else(|e| panic!("{}: failed to render: {e}", case.name));
            assert_eq!(
                url.as_str(),
                case.rendered,
                "{}: unexpected rendered URL",
                case.name
            );
        }
    }

    // ------------------------------- API-version invariants -----------------------------

    #[test]
    fn device_mutation_routes_are_v2_3_while_a_representative_read_route_is_v2_2() {
        use super::{GET_DEVICES, PAUSE_DEVICE, SET_DEVICE_NICKNAME};

        // Device nickname/pause writes must go to 2.3 — 2.2 accepts them, returns 200, and
        // silently drops the change (eero-api issue #102). Getting this wrong is silent data
        // loss, so this is asserted explicitly rather than only covered by the table above.
        assert_eq!(SET_DEVICE_NICKNAME.version, ApiVersion::V2_3);
        assert_eq!(PAUSE_DEVICE.version, ApiVersion::V2_3);

        // A representative read (device list) stays on the 2.2 default.
        assert_eq!(GET_DEVICES.version, ApiVersion::V2_2);
    }

    // ------------------------------ path template hygiene -------------------------------

    #[test]
    fn no_path_template_is_absolute_or_hardcodes_a_version_or_host() {
        // Every `Route::path` must be a bare, relative, `/`-separated template: the leading
        // slash, the "2.2"/"2.3" version segment and the host all come from
        // `ApiVersion::base_url`, never from the template itself. A template that embedded any
        // of these would double up on render (e.g. "//2.2/...") or silently ignore the
        // `ApiVersion` the constant declares.
        for case in CASES {
            let path = case.route.path;
            assert!(
                !path.starts_with('/'),
                "{}: path template must not start with '/' (leading slash comes from the base URL)",
                case.name
            );
            assert!(
                !path.contains("2.2") && !path.contains("2.3"),
                "{}: path template must not hardcode an API version (that's `ApiVersion`'s job)",
                case.name
            );
            assert!(
                !path.contains("e2ro.com") && !path.contains("http"),
                "{}: path template must not hardcode the host (that's `ApiVersion::base_url`'s job)",
                case.name
            );
        }
    }

    // ------------------------- validate_segment / traversal rejection -------------------

    use super::{SegmentError, validate_segment};

    #[test]
    fn validate_segment_rejects_empty_value() {
        assert_eq!(validate_segment(""), Err(SegmentError::Empty));
    }

    #[test]
    fn validate_segment_rejects_control_characters() {
        assert_eq!(
            validate_segment("..\n"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment(".\t."),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("\t.."),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("..\t"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("..\r\n"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("\u{0}"),
            Err(SegmentError::ControlCharacter)
        );
        assert_eq!(
            validate_segment("a\u{7f}b"),
            Err(SegmentError::ControlCharacter)
        );
    }

    #[test]
    fn validate_segment_rejects_literal_dot_segments() {
        assert_eq!(validate_segment("."), Err(SegmentError::DotSegment));
        assert_eq!(validate_segment(".."), Err(SegmentError::DotSegment));
    }

    #[test]
    fn validate_segment_accepts_legitimate_and_already_safe_values() {
        for value in [
            "device-0001",
            "aa:bb:cc:00:00:01",
            "a/b",
            "%2e%2e",
            "..%2f..",
            "．．", // fullwidth dots (U+FF0E) — not ASCII '.', never trips dot-segment removal
            "a?b",
            "a#b",
        ] {
            assert_eq!(
                validate_segment(value),
                Ok(()),
                "value {value:?} must be accepted"
            );
        }
    }

    #[test]
    fn render_rejects_a_hostile_placeholder_value_before_building_the_url() {
        // The concrete reproduction from the security finding: a trailing-newline id must not
        // collapse `.../networks/{network_id}/blacklist/{mac_or_device_id}` onto
        // `.../networks/{network_id}/blacklist/`.
        let route = Route {
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/blacklist/{mac_or_device_id}",
        };

        let err = route
            .render(&[("network_id", "100"), ("mac_or_device_id", "..\n")])
            .expect_err("a value that becomes \"..\" after tab/CR/LF stripping must be rejected");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::ControlCharacter,
            }
        );
    }

    #[test]
    fn render_rejects_an_empty_placeholder_value() {
        let route = Route {
            method: Method::DELETE,
            version: ApiVersion::V2_2,
            path: "networks/{network_id}/profiles/{profile_id}",
        };

        let err = route
            .render(&[("network_id", "100"), ("profile_id", "")])
            .expect_err("an empty value must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "profile_id".to_owned(),
                reason: SegmentError::Empty,
            }
        );
    }

    /// Security review finding F1: a real destructive route
    /// ([`super::REMOVE_FROM_BLACKLIST`]), not a hand-built fixture, must reject a literal
    /// `".."` placeholder value rather than silently rendering
    /// `DELETE /2.2/networks/100/blacklist` (the whole collection) instead of one item. See
    /// `tests/path_safety.rs` for the same guarantee proven end to end, with a mock server
    /// verifying zero requests are ever sent.
    #[test]
    fn remove_from_blacklist_rejects_a_literal_dot_dot_id() {
        let err = super::REMOVE_FROM_BLACKLIST
            .render(&[("network_id", "100"), ("mac_or_device_id", "..")])
            .expect_err("a literal \"..\" id must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::DotSegment,
            }
        );
    }

    /// Same as [`remove_from_blacklist_rejects_a_literal_dot_dot_id`], for a bare `"."`.
    #[test]
    fn remove_from_blacklist_rejects_a_literal_single_dot_id() {
        let err = super::REMOVE_FROM_BLACKLIST
            .render(&[("network_id", "100"), ("mac_or_device_id", ".")])
            .expect_err("a literal \".\" id must not collapse the route onto its collection");
        assert_eq!(
            err,
            RenderError::InvalidSegment {
                name: "mac_or_device_id".to_owned(),
                reason: SegmentError::DotSegment,
            }
        );
    }
}
