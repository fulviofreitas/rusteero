//! Every write that is not live-verified warns before it writes; every live-verified write stays
//! silent.
//!
//! This is the Rust form of the intent of `eero-api`'s `tests/api/test_write_warnings.py` at
//! v8.4.1. Python walks every `AuthenticatedAPI` write method with placeholder arguments and
//! checks the log; Rust has no runtime reflection, so the guard is split in three:
//!
//! 1. **A source scan** reads `src/` and collects the operation text of every
//!    `warn_uncharacterised_write(..)` call. A call site is resolved to the text that reaches the
//!    log: a string literal is taken as is, a `format!` template keeps its `{placeholder}`s, a
//!    named constant is replaced by its value, and an operation handed to a private helper
//!    (`fn put_settings_class(.., operation: &str)`) is expanded once per caller of that helper.
//!    The sorted multiset is compared with the table of call sites in eero-api v8.4.1
//!    ([`PYTHON_SITES`], one row per call, `py:N` being its line in the Python module of the same
//!    name). Adding, removing or rewording a warning on either side fails the comparison, and
//!    the only deliberate difference is recorded in [`EXTRA_PATHS`] with its reason.
//! 2. **A body scan** proves the live-verified writes ([`VERIFIED_WRITES`], the Rust form of
//!    Python's `_VERIFIED_WRITE_ALLOWLIST`) contain no `warn_uncharacterised_write` call, and that
//!    the two device wrappers still delegate to the blacklist writes they inherit that from.
//! 3. **Runtime checks** drive each verified write against wiremock under
//!    [`common::capture_logs`], pinning verb, path, content type and body, and assert no `WARN`
//!    event containing `Issuing write` is logged. Each assertion is paired with a positive
//!    control (the same capture does see a warning when one is emitted), so none can pass
//!    vacuously. A few unverified writes are driven too, to prove the text the scan resolves is
//!    the text that is logged for the helper, `format!` and double-warning shapes.
//!
//! The warning's operation text names the operation and the kind of resource only: identifiers
//! never belong in a `WARN` line. [`operations_never_interpolate_a_caller_supplied_identifier`]
//! pins that for every resolved text.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusteero::endpoints::blacklist::BlacklistApi;
use rusteero::endpoints::burst_reporters::BurstReportersApi;
use rusteero::endpoints::devices::DevicesApi;
use rusteero::endpoints::eeros::EerosApi;
use rusteero::endpoints::networks::NetworksApi;
use rusteero::endpoints::profiles::ProfilesApi;
use rusteero::endpoints::security::SecurityApi;
use rusteero::transport::Transport;
use rusteero::{Envelope, Error};
use serde_json::{Value, json};
use tracing::Level;
use wiremock::matchers::{any, body_bytes, body_json, body_string, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{LogCapture, MockEero, TEST_TOKEN, capture_logs, session_cookie, user_token_header};

// ===================== the expected call sites =====================

/// The suffix eero-api appends to a write that goes through the same `settings` endpoint as the
/// DNS write that is confirmed to reboot every eero.
macro_rules! risky {
    ($operation:literal) => {
        concat!(
            $operation,
            " -- may reboot every eero, like the confirmed DNS write path"
        )
    };
}

/// Every `warn_uncharacterised_write` call in eero-api v8.4.1, as `(module, operation)`.
///
/// Derived from `git grep warn_uncharacterised_write v8.4.1 -- src/` with Python's `f"..."`
/// strings rendered the way the Rust call renders them: `{action!r}` becomes `'{action}'` (the
/// action is validated against a closed vocabulary first, so `repr` and `'..'` agree) and `{key}`
/// stays `{key}`. `py:N` is the line of the call in `src/eero/api/<module>.py`.
const PYTHON_SITES: &[(&str, &str)] = &[
    ("account", "set account name"),               // py:93
    ("account", "set account email"),              // py:118
    ("account", "verify account email"),           // py:142
    ("account", "set account phone"),              // py:167
    ("account", "verify account phone"),           // py:191
    ("account", "set account consents"),           // py:215
    ("backup", "set backup internet for network"), // py:102
    (
        "backup_access_points",
        "add backup access point for network",
    ), // py:116
    (
        "backup_access_points",
        "update backup access point for network",
    ), // py:191
    (
        "backup_access_points",
        "delete backup access point for network",
    ), // py:225
    (
        "backup_access_points",
        "rearrange backup access points for network",
    ), // py:258
    ("backup_access_points", "start SSID discovery for network"), // py:311
    (
        "backup_access_points",
        "start backup connectivity check for network",
    ), // py:343
    (
        "burst_reporters",
        "request live-usage burst with a deprecated body for network",
    ), // py:106
    ("data_usage", "set data usage report settings for network"), // py:712
    ("ddns", "enable DDNS for network"),           // py:79
    ("ddns", "disable DDNS for network"),          // py:121
    ("devices", "update_device_via_link"),         // py:330
    ("devices", "set_device_labels"),              // py:437
    ("dhcp", risky!("set DHCP configuration for network")), // py:180
    ("dhcp", risky!("set connection mode for network")), // py:237
    ("dhcp", risky!("set NAT port randomization for network")), // py:285
    ("dhcp", "encrypt PPPoE credentials for eero"), // py:331
    ("diagnostics", "run diagnostics for network"), // py:123
    ("dns", "write DNS settings for network — reboots every eero"), // py:208
    ("dns_policies", "allow domain for network"),  // py:236
    ("dns_policies", "allow CNAMEs for network"),  // py:278
    ("dns_policies", "block domain for network"),  // py:330
    ("dns_policies", "allow domain for profiles on network"), // py:395
    ("dns_policies", "allow CNAMEs for profiles on network"), // py:438
    ("dns_policies", "block domain for profiles on network"), // py:494
    (
        "dns_policies",
        "set blocked applications for profile on network",
    ), // py:553
    ("dns_policies", "set network ad block for network"), // py:648
    ("dns_policies", "set network DNS policy {key} for network"), // py:693
    ("eeros", "set location for eero"),            // py:440
    ("eeros", "set nightlight for eero"),          // py:548
    (
        "eeros",
        concat!(
            "perform node action '{action}' on eero -- power-cycles ports and, ",
            "for POWER_CYCLE_ALL_PORTS_AND_REBOOT, reboots the eero",
        ),
    ), // py:688
    ("eeros", "perform port action '{action}' on eero port"), // py:741
    ("eeros", "cycle LED for eero"),               // py:783
    ("eeros", "override nightlight preview for eero"), // py:829
    ("forwards", "create forward for network"),    // py:157
    ("forwards", "update_forward"),                // py:190
    ("forwards", "delete forward for network"),    // py:213
    ("members", "create invite for network"),      // py:137
    ("members", "update invite for network"),      // py:165
    ("members", "delete invite for network"),      // py:189
    ("members", "respond to invite for network"),  // py:242
    ("members", "cancel pending admin invites for network"), // py:269
    ("members", "promote member for network"),     // py:299
    ("members", "remove admin from network"),      // py:323
    ("members", "query invite by code"),           // py:348
    ("networks", "reboot network"),                // py:181
    ("networks", "set network name for network"),  // py:313
    ("networks", "set network password for network"), // py:353
    ("networks", "clear network password for network"), // py:388
    ("networks", "hide 5 GHz for network"),        // py:595
    ("networks", "clear hide 5 GHz for network"),  // py:597
    ("notifications", "set notification settings for network"), // py:110
    ("notifications", "mark notifications read for network"), // py:175
    ("notifications", "set account push settings"), // py:249
    ("power_saving", "set power saving for network"), // py:99
    ("power_saving", "create power saving schedule for network"), // py:178
    ("power_saving", "update power saving schedule for network"), // py:250
    ("power_saving", "delete power saving schedule for network"), // py:282
    ("profiles", "update profile"),                // py:154
    ("profiles", "set devices for profile"),       // py:241
    ("profiles", "create profile for network"),    // py:300
    ("profiles", "delete profile"),                // py:351
    ("reservations", "create reservation for network"), // py:150
    ("reservations", "update_reservation"),        // py:183
    ("reservations", "delete reservation for network"), // py:220
    ("schedule", "create_schedule"),               // py:211
    ("schedule", "update_schedule"),               // py:270
    ("schedule", "delete_schedule"),               // py:296
    ("security", risky!("set WPA3 for network")),  // py:126
    ("security", risky!("set band steering for network")), // py:180
    ("security", risky!("set UPnP for network")),  // py:235
    ("security", risky!("set IPv6 for network")),  // py:289
    ("security", risky!("set IPv6 upstream for network")), // py:349
    (
        "security",
        risky!("configure security settings for network"),
    ), // py:429
    ("security", risky!("set MLO mode for network")), // py:484
    ("security", "set fast transition for network"), // py:565
    ("security", "set Passpoint enabled for network"), // py:604
    ("security", "set proxied nodes for network"), // py:644
    ("sqm", "set SQM for network"),                // py:117
    ("subnets", "set subnet configuration for network"), // py:110
    ("subnets", "delete subnet for network"),      // py:140
    ("subnets", "set subnet content filters for network"), // py:178
    ("support", "request support for network"),    // py:104
    ("thread", "update thread config for network"), // py:154
    ("thread", "regenerate thread credentials for network"), // py:188
    ("updates", "apply update for network — reboots every node"), // py:102
    ("updates", "set preferred update hour for network"), // py:136
    ("wan", "set multi-static-IP configuration for network"), // py:126
    ("wan", risky!("set secondary WAN configuration for network")), // py:173
    (
        "wan",
        risky!("set secondary WAN access for device on network"),
    ), // py:222
    ("wpa3", "set per-band WPA3 mode for network"), // py:158
];

/// Deliberate differences between the Rust warning paths and the Python call sites, as
/// `(module, operation, extra paths, reason)`. The expected multiset is [`PYTHON_SITES`] plus
/// `extra paths` further copies of each row here.
///
/// A "path" is one resolved call: a helper that takes the operation as a parameter counts once
/// per caller, because that is how many times the text can reach the log.
const EXTRA_PATHS: &[(&str, &str, usize, &str)] = &[(
    "profiles",
    "update profile",
    2,
    concat!(
        "Python has one `_update_profile` call site, reached by pause_profile, rename_profile and ",
        "set_profile_devices (which warns \"set devices for profile\" first, so it warns twice). ",
        "Rust reaches the text through three paths: pause_profile and rename_profile call the ",
        "`profile_write` helper with it, and set_profile_devices calls the warning directly. ",
        "Three paths against one site is a difference of two; each of the three writes still ",
        "logs it exactly once, as in Python.",
    ),
)];

/// The prefixes that make a public coroutine of `eero-api` write-shaped, as Python's
/// `_WRITE_PREFIXES` lists them.
const WRITE_PREFIXES: &[&str] = &[
    "set_",
    "create_",
    "update_",
    "delete_",
    "add_",
    "remove_",
    "block_",
    "unblock_",
    "apply_",
    "reboot_",
    "pause_",
    "rename_",
    "run_",
    "regenerate_",
    "enable_",
    "disable_",
    "mark_",
    "allow_",
    "rearrange_",
    "start_",
    "promote_",
    "cancel_",
    "respond_",
    "verify_",
    "node_",
    "port_",
    "led_",
    "nightlight_",
    "clear_",
];

/// The placeholders an operation text may still contain after resolution: both are filled from a
/// closed vocabulary that is validated before the warning, never from a caller-supplied
/// identifier.
const VOCABULARY_PLACEHOLDERS: &[&str] = &["{action}", "{key}"];

/// A write that eero-api documents as live-verified, so it carries no warning:
/// `(module, function, delegate, evidence)`. `delegate` is the call a wrapper inherits its silence
/// from. The Rust form of Python's `_VERIFIED_WRITE_ALLOWLIST`, minus `create_burst_reporter`,
/// which still warns on its deprecated non-empty-body path (see [`burst_reporter_body_paths`]).
const VERIFIED_WRITES: &[(&str, &str, Option<&str>, &str)] = &[
    (
        "devices",
        "pause_device",
        None,
        "eero-api issue #102: targets the 2.3 endpoint, where the mutation persists",
    ),
    (
        "devices",
        "set_device_nickname",
        None,
        "eero-api issue #102: targets the 2.3 endpoint, where the mutation persists",
    ),
    (
        "devices",
        "unblock_device",
        Some("remove_from_blacklist"),
        "delegates to the live-verified blacklist removal",
    ),
    (
        "devices",
        "block_device",
        Some("add_to_blacklist"),
        "delegates to the live-verified blacklist add",
    ),
    (
        "devices",
        "set_device_type",
        None,
        "live-verified 2026-09-20: the value persisted and read back",
    ),
    (
        "blacklist",
        "remove_from_blacklist",
        None,
        "eero-api issue #109: a blacklisted entry's id is the MAC with colons stripped",
    ),
    (
        "blacklist",
        "add_to_blacklist",
        None,
        "live-verified 2026-10-06 against API 2.2 with the form-encoded `mac` shape",
    ),
    (
        "eeros",
        "set_led",
        None,
        "live-verified 2026-09-20: a node's light went off and on, no reboot",
    ),
    (
        "eeros",
        "set_led_brightness",
        None,
        "live-verified 2026-09-20: brightness changed and read back, no reboot",
    ),
    (
        "eeros",
        "reboot_eero",
        None,
        "live-verified 2026-09-20: only the targeted node rebooted",
    ),
    (
        "networks",
        "set_guest_network",
        None,
        "live-verified 2026-09-20: enable, disable and restore read back, name unchanged",
    ),
    (
        "networks",
        "set_guest_password",
        None,
        "live-verified 2026-09-20: set, clear and restore read back",
    ),
    (
        "networks",
        "clear_guest_password",
        None,
        "live-verified 2026-09-20: set, clear and restore read back",
    ),
    (
        "networks",
        "run_speed_test",
        None,
        "live-verified 2026-09-20: returns 202 with null data, a result appears a minute later",
    ),
];

// ===================== reading the sources =====================

/// The production code of one file under `src/`.
struct SourceFile {
    /// Path relative to `src/`, always `/`-separated.
    path: String,
    /// The code before the first top-level `#[cfg(test)]`, with every comment blanked out.
    code: String,
}

/// One `warn_uncharacterised_write(..)` call.
struct CallSite {
    /// The file the call is in, relative to `src/`.
    path: String,
    /// The name of the function the call is in.
    function: String,
    /// The call's argument expression, as written.
    argument: String,
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The index just past the string literal whose opening quote is at `open`.
fn string_end(bytes: &[u8], open: usize) -> usize {
    let mut at = open + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// The index just past the raw string literal starting at `at`, if one starts there.
fn raw_string_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes[at] != b'r' || (at > 0 && is_ident(bytes[at - 1])) {
        return None;
    }
    let hashes = bytes[at + 1..].iter().take_while(|&&b| b == b'#').count();
    if bytes.get(at + 1 + hashes) != Some(&b'"') {
        return None;
    }
    let mut closing = vec![b'"'];
    closing.extend(std::iter::repeat_n(b'#', hashes));
    let body = at + 2 + hashes;
    Some(
        bytes[body..]
            .windows(closing.len())
            .position(|window| window == closing)
            .map_or(bytes.len(), |found| body + found + closing.len()),
    )
}

/// The index just past the char literal whose quote is at `quote`; `None` for a lifetime.
fn char_end(bytes: &[u8], quote: usize) -> Option<usize> {
    let first = *bytes.get(quote + 1)?;
    if first == b'\\' {
        let close = bytes.get(quote + 3..)?.iter().position(|&b| b == b'\'')?;
        return Some(quote + 3 + close + 1);
    }
    let width = match first {
        0..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    };
    (bytes.get(quote + 1 + width) == Some(&b'\'')).then_some(quote + 2 + width)
}

/// The index just past the string, raw string or char literal that starts at `at`, if any.
fn literal_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b'"' => Some(string_end(bytes, at)),
        b'r' => raw_string_end(bytes, at),
        b'\'' => char_end(bytes, at),
        _ => None,
    }
}

/// `source` with every comment replaced by spaces (newlines kept) and every literal untouched, so
/// a search for code never matches commentary and a literal's text is never mistaken for a
/// comment.
fn blank_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"//") {
            let end = bytes[at..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |found| at + found);
            out.resize(out.len() + (end - at), b' ');
            at = end;
        } else if bytes[at..].starts_with(b"/*") {
            let start = at;
            let mut depth = 0_usize;
            while at < bytes.len() {
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    at += 1;
                }
            }
            out.extend(
                bytes[start..at]
                    .iter()
                    .map(|&b| if b == b'\n' { b'\n' } else { b' ' }),
            );
        } else if let Some(end) = literal_end(bytes, at) {
            out.extend_from_slice(&bytes[at..end]);
            at = end;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).expect("only whole comments are replaced, and by ASCII spaces")
}

/// The index of the bracket that closes the one opened at `open`.
fn matching_close(code: &str, open: usize) -> usize {
    let bytes = code.as_bytes();
    let mut depth = 0_usize;
    let mut at = open;
    while at < bytes.len() {
        if let Some(end) = literal_end(bytes, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
        at += 1;
    }
    panic!("no bracket closes the one at byte {open}");
}

/// `text` split at its top-level commas (not those inside brackets or literals), trimmed, with a
/// trailing empty part dropped.
fn split_top_level(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut at = 0;
    while at < bytes.len() {
        if let Some(end) = literal_end(bytes, at) {
            at = end;
            continue;
        }
        match bytes[at] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(text[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// The value of the string literal that `text` starts with, escapes decoded (a backslash before a
/// newline swallows the newline and the indentation after it, as in Rust source).
fn decode_literal(text: &str) -> String {
    let rest = text
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("not a string literal: {text:?}"));
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next().expect("an escape follows a backslash") {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '\'' => out.push('\''),
                '\n' => {
                    let skipped = chars.as_str().len() - chars.as_str().trim_start().len();
                    chars = chars.as_str()[skipped..].chars();
                }
                'u' => {
                    let digits: String = chars.by_ref().skip(1).take_while(|&c| c != '}').collect();
                    let scalar = u32::from_str_radix(&digits, 16).expect("a hex escape");
                    out.push(char::from_u32(scalar).expect("a valid scalar"));
                }
                other => panic!("unsupported escape \\{other} in {text:?}"),
            },
            c => out.push(c),
        }
    }
    panic!("unterminated string literal: {text:?}");
}

/// Reads every `.rs` file under `dir` (sorted, recursively).
fn read_sources(dir: &Path, root: &Path, files: &mut Vec<SourceFile>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            read_sources(&entry, root, files);
        } else if entry.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&entry)
                .unwrap_or_else(|err| panic!("cannot read {}: {err}", entry.display()));
            let production = text
                .find("\n#[cfg(test)]")
                .map_or(text.as_str(), |cut| &text[..cut]);
            let relative = entry
                .strip_prefix(root)
                .expect("a file under the root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.push(SourceFile {
                path: relative,
                code: blank_comments(production),
            });
        }
    }
}

/// Every source file under this crate's `src/`.
fn sources() -> Vec<SourceFile> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    read_sources(&root, &root, &mut files);
    files
}

// ===================== resolving a call site =====================

const WARN_FN: &str = "warn_uncharacterised_write";

/// Whether `name` occurs at `at` as a whole identifier.
fn is_word_at(code: &str, at: usize, name: &str) -> bool {
    let bytes = code.as_bytes();
    code[at..].starts_with(name)
        && (at == 0 || !is_ident(bytes[at - 1]))
        && bytes.get(at + name.len()).is_none_or(|&b| !is_ident(b))
}

/// The offset of the `(` of every call to `name` in `code` (not its definition).
fn call_opens(code: &str, name: &str) -> Vec<usize> {
    code.match_indices(name)
        .filter(|&(at, _)| is_word_at(code, at, name))
        .filter(|&(at, _)| !code[..at].trim_end().ends_with("fn"))
        .filter_map(|(at, _)| {
            let after = at + name.len();
            let open = after + (code[after..].len() - code[after..].trim_start().len());
            (code.as_bytes().get(open) == Some(&b'(')).then_some(open)
        })
        .collect()
}

/// The name of the function that `offset` is in.
fn function_at(code: &str, offset: usize) -> String {
    let (at, _) = code
        .rmatch_indices("fn ")
        .find(|&(at, _)| at < offset && is_word_at(code, at, "fn"))
        .unwrap_or_else(|| panic!("byte {offset} is not inside a function"));
    code[at + 3..]
        .chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
        .collect()
}

/// The offsets of the opening `(` and closing `)` of function `name`'s parameter list.
fn parameter_list(code: &str, name: &str) -> (usize, usize) {
    let header = format!("fn {name}");
    let at = code
        .match_indices(&header)
        .map(|(at, _)| at)
        .find(|&at| {
            code.as_bytes()
                .get(at + header.len())
                .is_none_or(|&b| !is_ident(b))
        })
        .unwrap_or_else(|| panic!("no function named {name}"));
    let open = at + code[at..].find('(').expect("a parameter list");
    (open, matching_close(code, open))
}

/// The names of function `name`'s parameters other than the receiver.
fn parameters(code: &str, name: &str) -> Vec<String> {
    let (open, close) = parameter_list(code, name);
    split_top_level(&code[open + 1..close])
        .into_iter()
        .filter_map(|part| part.split_once(':'))
        .map(|(param, _)| param.trim().trim_start_matches("mut ").to_owned())
        .collect()
}

/// What a call's argument expression is.
enum Operation {
    /// `"text"`.
    Literal(String),
    /// `&format!("template {placeholder}")`.
    Template(String),
    /// A bare name: a parameter of the enclosing function or a constant.
    Name(String),
}

fn classify(argument: &str) -> Operation {
    let argument = argument.trim();
    let argument = argument.strip_prefix('&').unwrap_or(argument).trim_start();
    if argument.starts_with('"') {
        return Operation::Literal(decode_literal(argument));
    }
    if let Some(rest) = argument.strip_prefix("format!(") {
        return Operation::Template(decode_literal(rest.trim_start()));
    }
    assert!(
        !argument.is_empty() && argument.bytes().all(is_ident),
        "cannot resolve the warning argument {argument:?}: teach `classify` this shape"
    );
    Operation::Name(argument.to_owned())
}

/// The value of the string constant `name`.
fn constant(code: &str, name: &str) -> String {
    let header = format!("const {name}:");
    let at = code
        .find(&header)
        .unwrap_or_else(|| panic!("`{name}` is neither a parameter nor a constant in this file"));
    let quote = at + code[at..].find('"').expect("a string constant");
    decode_literal(&code[quote..])
}

/// The texts that `argument`, written in function `function`, can log.
fn resolve_argument(code: &str, function: &str, argument: &str) -> Vec<String> {
    match classify(argument) {
        Operation::Literal(text) => vec![text],
        Operation::Template(template) if template.contains("{operation}") => {
            resolve_parameter(code, function, "operation")
                .into_iter()
                .map(|operation| template.replace("{operation}", &operation))
                .collect()
        }
        Operation::Template(template) => vec![template],
        Operation::Name(name) if parameters(code, function).contains(&name) => {
            resolve_parameter(code, function, &name)
        }
        Operation::Name(name) => vec![constant(code, &name)],
    }
}

/// The texts that parameter `parameter` of private helper `function` takes across all its callers.
fn resolve_parameter(code: &str, function: &str, parameter: &str) -> Vec<String> {
    let index = parameters(code, function)
        .iter()
        .position(|name| name == parameter)
        .unwrap_or_else(|| panic!("`{function}` has no parameter `{parameter}`"));
    let callers = call_opens(code, function);
    assert!(
        !callers.is_empty(),
        "`{function}` takes the warning operation but nothing in its file calls it"
    );
    callers
        .into_iter()
        .flat_map(|open| {
            let close = matching_close(code, open);
            let arguments = split_top_level(&code[open + 1..close]);
            resolve_argument(code, &function_at(code, open), arguments[index])
        })
        .collect()
}

/// Every `warn_uncharacterised_write(..)` call in `files`.
fn call_sites(files: &[SourceFile]) -> Vec<CallSite> {
    files
        .iter()
        .flat_map(|file| {
            call_opens(&file.code, WARN_FN).into_iter().map(|open| {
                let close = matching_close(&file.code, open);
                CallSite {
                    path: file.path.clone(),
                    function: function_at(&file.code, open),
                    argument: file.code[open + 1..close]
                        .trim()
                        .trim_end_matches(',')
                        .to_owned(),
                }
            })
        })
        .collect()
}

/// The domain module a file under `src/` is, if it is `endpoints/<module>.rs`.
fn module_of(path: &str) -> Option<&str> {
    path.strip_prefix("endpoints/")
        .and_then(|name| name.strip_suffix(".rs"))
}

/// The `(module, operation)` pairs that the Rust sources can log, sorted.
fn actual_operations(files: &[SourceFile]) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for site in call_sites(files) {
        let module = module_of(&site.path).unwrap_or_else(|| {
            panic!(
                "`{WARN_FN}` is called from src/{}, outside src/endpoints/: add the call to the \
                 expected table with its Python counterpart",
                site.path
            )
        });
        let file = files
            .iter()
            .find(|file| file.path == site.path)
            .expect("the call site's file");
        for operation in resolve_argument(&file.code, &site.function, &site.argument) {
            rows.push((module.to_owned(), operation));
        }
    }
    rows.sort();
    rows
}

/// The `(module, operation)` pairs that eero-api v8.4.1 plus the recorded differences expect,
/// sorted.
fn expected_operations() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = PYTHON_SITES
        .iter()
        .map(|&(module, operation)| (module.to_owned(), operation.to_owned()))
        .collect();
    for &(module, operation, extra, _) in EXTRA_PATHS {
        rows.extend(std::iter::repeat_n(
            (module.to_owned(), operation.to_owned()),
            extra,
        ));
    }
    rows.sort();
    rows
}

/// The rows that appear a different number of times in `expected` and `actual`, rendered one per
/// line.
fn multiset_differences(expected: &[(String, String)], actual: &[(String, String)]) -> Vec<String> {
    let mut counts: BTreeMap<&(String, String), (usize, usize)> = BTreeMap::new();
    for row in expected {
        counts.entry(row).or_default().0 += 1;
    }
    for row in actual {
        counts.entry(row).or_default().1 += 1;
    }
    counts
        .into_iter()
        .filter(|(_, (want, found))| want != found)
        .map(|((module, operation), (want, found))| {
            format!("  {module}: {operation:?}: expected {want}, found {found}")
        })
        .collect()
}

/// The source of `src/endpoints/<module>.rs`.
fn endpoint_source<'a>(files: &'a [SourceFile], module: &str) -> &'a SourceFile {
    let wanted = format!("endpoints/{module}.rs");
    files
        .iter()
        .find(|file| file.path == wanted)
        .unwrap_or_else(|| panic!("no src/{wanted}"))
}

/// The body (braces included) of function `name`.
fn function_body<'a>(code: &'a str, name: &str) -> &'a str {
    let (_, close) = parameter_list(code, name);
    let open = close + code[close..].find('{').expect("a function body");
    &code[open..=matching_close(code, open)]
}

/// The identifier `text` starts with.
fn identifier_at(text: &str) -> String {
    text.chars()
        .take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
        .collect()
}

/// The name of every `pub async fn` in `code`, in source order.
fn public_async_functions(code: &str) -> Vec<String> {
    code.match_indices("pub async fn ")
        .filter(|&(at, _)| at == 0 || !is_ident(code.as_bytes()[at - 1]))
        .map(|(at, marker)| identifier_at(&code[at + marker.len()..]))
        .collect()
}

/// The name of every function defined in `code`.
fn defined_functions(code: &str) -> BTreeSet<String> {
    code.match_indices("fn ")
        .filter(|&(at, _)| is_word_at(code, at, "fn"))
        .map(|(at, _)| identifier_at(&code[at + 3..]))
        .filter(|name| !name.is_empty())
        .collect()
}

/// The functions of `code` that log a write warning: those that call `warn_uncharacterised_write`
/// and, transitively, those that call one of them. A name is matched wherever it is called, so a
/// call on another type that happens to share a function's name counts too; nothing in the
/// endpoint modules does.
fn warning_functions(code: &str) -> BTreeSet<String> {
    let bodies: Vec<(String, &str)> = defined_functions(code)
        .into_iter()
        .map(|name| {
            let body = function_body(code, &name);
            (name, body)
        })
        .collect();
    let mut warning: BTreeSet<String> = BTreeSet::new();
    loop {
        let known = warning.len();
        for (name, body) in &bodies {
            let warns = !call_opens(body, WARN_FN).is_empty()
                || warning
                    .iter()
                    .any(|callee| !call_opens(body, callee).is_empty());
            if warns {
                warning.insert(name.clone());
            }
        }
        if warning.len() == known {
            return warning;
        }
    }
}

// ===================== the source scans =====================

/// The scan must not agree with the table vacuously: it finds a call in every module the table
/// names, and in no other.
#[test]
fn the_scan_finds_warning_calls_in_every_module_the_table_names() {
    let files = sources();
    let sites = call_sites(&files);
    let scanned: BTreeSet<&str> = sites
        .iter()
        .filter_map(|site| module_of(&site.path))
        .collect();
    let named: BTreeSet<&str> = PYTHON_SITES.iter().map(|&(module, _)| module).collect();
    assert_eq!(scanned, named);
    assert!(sites.len() >= named.len(), "every module has a call");
}

#[test]
fn the_helper_is_defined_once_and_called_only_from_endpoint_modules() {
    let files = sources();
    let defining: Vec<_> = files
        .iter()
        .filter(|file| file.code.contains(&format!("fn {WARN_FN}")))
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(defining, ["links.rs"], "the helper must be defined once");
    let outside: Vec<_> = call_sites(&files)
        .into_iter()
        .filter(|site| !site.path.starts_with("endpoints/"))
        .map(|site| format!("src/{} in `{}`", site.path, site.function))
        .collect();
    assert!(
        outside.is_empty(),
        "warning calls outside src/endpoints/: {outside:?}"
    );
}

/// The warnings the Rust sources can log are exactly eero-api v8.4.1's, plus the recorded
/// differences.
#[test]
fn warning_operations_match_the_python_call_sites() {
    let problems = multiset_differences(&expected_operations(), &actual_operations(&sources()));
    assert!(
        problems.is_empty(),
        "the warning operations in src/ differ from eero-api v8.4.1:\n{}\n\
         A Rust-only difference needs a row in EXTRA_PATHS with its reason; anything else is a \
         parity bug.",
        problems.join("\n")
    );
}

/// Negative control for the comparison: a row present a different number of times on the two
/// sides is reported with both counts, and equal multisets report nothing.
#[test]
fn the_comparison_reports_a_count_difference() {
    let row = |operation: &str| ("profiles".to_owned(), operation.to_owned());
    let expected = vec![
        row("update profile"),
        row("update profile"),
        row("delete profile"),
    ];
    let actual = vec![row("update profile"), row("delete profile"), row("extra")];
    assert_eq!(
        multiset_differences(&expected, &actual),
        [
            "  profiles: \"extra\": expected 0, found 1",
            "  profiles: \"update profile\": expected 2, found 1",
        ]
    );
    assert!(multiset_differences(&expected, &expected).is_empty());
}

/// An entry of `EXTRA_PATHS` must name a real Python call site and explain itself.
#[test]
fn recorded_differences_name_real_python_sites() {
    for &(module, operation, extra, reason) in EXTRA_PATHS {
        assert!(
            PYTHON_SITES.contains(&(module, operation)),
            "EXTRA_PATHS names {module}: {operation:?}, which is not a Python call site"
        );
        assert!(extra > 0, "{module}: {operation:?} records no difference");
        assert!(
            reason.len() > 40,
            "{module}: {operation:?} needs a real reason"
        );
    }
}

/// Python's `test_write_emits_uncharacterised_write_warning` fails a warning that interpolates
/// the placeholder identifier it was called with. Here no resolved text may carry a placeholder
/// other than the two that are filled from a validated vocabulary.
#[test]
fn operations_never_interpolate_a_caller_supplied_identifier() {
    for (module, operation) in actual_operations(&sources()) {
        let mut rest = operation.as_str();
        while let Some(start) = rest.find('{') {
            let end = rest[start..].find('}').map_or_else(
                || panic!("unclosed `{{` in {operation:?}"),
                |found| start + found,
            );
            let placeholder = &rest[start..=end];
            assert!(
                VOCABULARY_PLACEHOLDERS.contains(&placeholder),
                "{module}: {operation:?} interpolates {placeholder}, which is not a validated \
                 vocabulary value"
            );
            rest = &rest[end + 1..];
        }
    }
}

/// The live-verified writes issue no warning from their own bodies, and the two device wrappers
/// still delegate to the blacklist writes they inherit that from.
#[test]
fn verified_writes_have_no_warning_call() {
    let files = sources();
    for &(module, function, delegate, evidence) in VERIFIED_WRITES {
        let file = endpoint_source(&files, module);
        let body = function_body(&file.code, function);
        assert!(
            call_opens(body, WARN_FN).is_empty(),
            "{module}::{function} is documented as live-verified ({evidence}) but calls {WARN_FN}"
        );
        if let Some(delegate) = delegate {
            assert!(
                body.contains(&format!("{delegate}(")),
                "{module}::{function} must delegate to {delegate}, which is what keeps it silent"
            );
        }
    }
}

/// The blacklist module carries no warning at all, and `add_to_blacklist` stays a plain form
/// `POST`.
#[test]
fn the_blacklist_module_issues_no_warning() {
    let files = sources();
    let blacklist = endpoint_source(&files, "blacklist");
    assert!(call_opens(&blacklist.code, WARN_FN).is_empty());
    assert!(
        function_body(&blacklist.code, "add_to_blacklist").contains("RequestBody::Form"),
        "add_to_blacklist is verified with the form-encoded `mac` shape"
    );
}

/// `create_burst_reporter` is verified with the default `""` body and warns only when a caller
/// still supplies the deprecated body: the module has exactly one warning, reached through the
/// constant that names that case.
#[test]
fn burst_reporter_body_paths() {
    let files = sources();
    let burst = endpoint_source(&files, "burst_reporters");
    let operations = actual_operations(&files)
        .into_iter()
        .filter(|(module, _)| module == "burst_reporters")
        .map(|(_, operation)| operation)
        .collect::<Vec<_>>();
    assert_eq!(
        operations,
        ["request live-usage burst with a deprecated body for network"]
    );
    let body = function_body(&burst.code, "create_burst_reporter");
    assert_eq!(call_opens(body, WARN_FN).len(), 1);
    assert!(
        body.contains("RequestBody::EmptyJsonString"),
        "the default body is the literal two-byte `\"\"`, sent without a warning"
    );
}

/// The Rust form of Python's `test_write_emits_uncharacterised_write_warning`: every write-shaped
/// public method of an endpoint module warns, directly or through a helper or sibling method it
/// calls, unless it is one of [`VERIFIED_WRITES`] (or the default-body `create_burst_reporter`,
/// which warns only on its deprecated body). A write added without its warning fails here even
/// though it adds no row to the call-site table.
#[test]
fn every_write_shaped_method_warns_or_is_verified() {
    let files = sources();
    let mut inspected = 0;
    let mut silent = Vec::new();
    for file in &files {
        let Some(module) = module_of(&file.path) else {
            continue;
        };
        let warning = warning_functions(&file.code);
        for name in public_async_functions(&file.code) {
            if !WRITE_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
                continue;
            }
            inspected += 1;
            let verified = VERIFIED_WRITES
                .iter()
                .any(|&(verified_module, function, ..)| {
                    verified_module == module && function == name
                });
            if !verified && !warning.contains(&name) {
                silent.push(format!("{module}::{name}"));
            }
        }
    }
    assert!(
        inspected >= PYTHON_SITES.len(),
        "the scan inspected only {inspected} write-shaped methods"
    );
    assert!(
        silent.is_empty(),
        "these write-shaped methods issue no uncharacterised-write warning: {silent:?}. Add a \
         `links::warn_uncharacterised_write` call before the request, or, only with live \
         verification evidence, add the method to VERIFIED_WRITES."
    );
}

/// Negative control for the method scan: on a synthetic module it separates a write that warns
/// through a helper from one that never reaches a warning, and ignores a call in a comment.
#[test]
fn the_method_scan_tells_a_warning_write_from_a_silent_one() {
    let code = blank_comments(
        r#"
        impl Api {
            pub async fn set_via_helper(&self) { self.guarded().await }
            pub async fn set_directly(&self) { links::warn_uncharacterised_write("x"); }
            pub async fn set_silently(&self) {
                // links::warn_uncharacterised_write("not code");
                self.transport.put().await
            }
            async fn guarded(&self) { links::warn_uncharacterised_write("y"); }
        }
        "#,
    );
    assert_eq!(
        public_async_functions(&code),
        ["set_via_helper", "set_directly", "set_silently"]
    );
    let warning = warning_functions(&code);
    assert!(warning.contains("set_via_helper"));
    assert!(warning.contains("set_directly"));
    assert!(!warning.contains("set_silently"));
}

// ===================== runtime: the verified writes stay silent =====================

/// How a verified write's body must look on the wire.
enum Wire {
    /// No body at all.
    Empty,
    /// `Content-Type: application/json` and exactly this JSON.
    Json(Value),
    /// `Content-Type: application/x-www-form-urlencoded` and exactly this string.
    Form(&'static str),
    /// `Content-Type: application/json` and the literal two bytes `""`.
    EmptyJsonString,
}

/// The envelope every mock answers with.
fn ok_envelope() -> Value {
    json!({ "meta": { "code": 200 }, "data": {} })
}

/// Mounts the one request `verb route` with `wire` as its body, authenticated with both
/// credentials, expecting it exactly once.
async fn mount_write(mock: &MockEero, verb: &str, route: &str, wire: Wire) {
    let builder = Mock::given(method(verb))
        .and(path(route))
        .and(session_cookie())
        .and(user_token_header());
    let builder = match wire {
        Wire::Empty => builder.and(body_bytes(Vec::new())),
        Wire::Json(value) => builder
            .and(header("content-type", "application/json"))
            .and(body_json(value)),
        Wire::Form(encoded) => builder
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string(encoded)),
        Wire::EmptyJsonString => builder
            .and(header("content-type", "application/json"))
            .and(body_string("\"\"")),
    };
    builder
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// Asserts the capture has seen no `Issuing write` warning, after proving it would see one.
fn assert_no_write_warning(logs: &LogCapture) {
    assert_eq!(
        logs.count(Level::WARN, "Issuing write"),
        0,
        "a live-verified write must not warn: {:?}",
        logs.warnings()
    );
    rusteero::links::warn_uncharacterised_write("probe");
    assert_eq!(
        logs.count(Level::WARN, "Issuing write"),
        1,
        "the capture must see a warning when one is emitted"
    );
}

/// Drives `call` against a mock that accepts only `verb route` with `wire`, and asserts it logs
/// no write warning.
async fn assert_silent_write<F, Fut>(
    verb: &str,
    route: &str,
    wire: Wire,
    call: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Arc<Transport>) -> Fut,
    Fut: Future<Output = Result<Envelope, Error>>,
{
    let mock = MockEero::start().await;
    mount_write(&mock, verb, route, wire).await;
    let (_guard, logs) = capture_logs();
    call(Arc::new(mock.transport_with_token(TEST_TOKEN))).await?;
    assert_no_write_warning(&logs);
    Ok(())
}

#[tokio::test]
async fn pause_device_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.3/networks/network-0001/devices/device-0001",
        Wire::Json(json!({ "paused": true })),
        |transport| async move {
            DevicesApi::new(transport)
                .pause_device("network-0001", "device-0001", true)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_device_nickname_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.3/networks/network-0001/devices/device-0001",
        Wire::Json(json!({ "nickname": "Living Room TV" })),
        |transport| async move {
            DevicesApi::new(transport)
                .set_device_nickname("network-0001", "device-0001", "Living Room TV")
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_device_type_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.2/networks/network-0001/devices/device-0001",
        Wire::Json(json!({ "device_type": "computer" })),
        |transport| async move {
            DevicesApi::new(transport)
                .set_device_type("network-0001", "device-0001", "computer")
                .await
        },
    )
    .await
}

#[tokio::test]
async fn block_device_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "POST",
        "/2.2/networks/network-0001/blacklist",
        Wire::Form("mac=aa%3Abb%3Acc%3A00%3A00%3A01"),
        |transport| async move {
            DevicesApi::new(transport)
                .block_device("network-0001", "aa:bb:cc:00:00:01")
                .await
        },
    )
    .await
}

#[tokio::test]
async fn unblock_device_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "DELETE",
        "/2.2/networks/network-0001/blacklist/device-0001",
        Wire::Empty,
        |transport| async move {
            DevicesApi::new(transport)
                .unblock_device("network-0001", "device-0001")
                .await
        },
    )
    .await
}

#[tokio::test]
async fn add_to_blacklist_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "POST",
        "/2.2/networks/network-0001/blacklist",
        Wire::Form("mac=aa%3Abb%3Acc%3A00%3A00%3A01"),
        |transport| async move {
            BlacklistApi::new(transport)
                .add_to_blacklist("network-0001", "aa:bb:cc:00:00:01", None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn remove_from_blacklist_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "DELETE",
        "/2.2/networks/network-0001/blacklist/aabbcc000001",
        Wire::Empty,
        |transport| async move {
            BlacklistApi::new(transport)
                .remove_from_blacklist("network-0001", "aabbcc000001", None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_led_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.2/eeros/eero-0001/led",
        Wire::Form("led_on=true"),
        |transport| async move {
            EerosApi::new(transport)
                .set_led("eero-0001", true, None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_led_brightness_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.2/eeros/eero-0001/led",
        Wire::Form("led_brightness=42"),
        |transport| async move {
            EerosApi::new(transport)
                .set_led_brightness("eero-0001", 42, None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn reboot_eero_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "POST",
        "/2.2/eeros/eero-0001/reboot",
        Wire::EmptyJsonString,
        |transport| async move {
            EerosApi::new(transport)
                .reboot_eero("eero-0001", None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_guest_network_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.2/networks/network-0001/guestnetwork",
        Wire::Form("enabled=true&name=Guest"),
        |transport| async move {
            NetworksApi::new(transport)
                .set_guest_network("network-0001", true, Some("Guest"), None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn set_guest_password_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "PUT",
        "/2.2/networks/network-0001/guestnetwork/password",
        Wire::Form("password=guestpass"),
        |transport| async move {
            NetworksApi::new(transport)
                .set_guest_password("network-0001", "guestpass", None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn clear_guest_password_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "DELETE",
        "/2.2/networks/network-0001/guestnetwork/password",
        Wire::Empty,
        |transport| async move {
            NetworksApi::new(transport)
                .clear_guest_password("network-0001", None)
                .await
        },
    )
    .await
}

#[tokio::test]
async fn run_speed_test_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "POST",
        "/2.2/networks/network-0001/speedtest",
        Wire::EmptyJsonString,
        |transport| async move {
            NetworksApi::new(transport)
                .run_speed_test("network-0001", None)
                .await
        },
    )
    .await
}

/// The default-body `create_burst_reporter` is the live-verified write: one `POST` of `""`, no
/// warning.
#[tokio::test]
async fn create_burst_reporter_with_the_default_body_logs_no_write_warning() -> anyhow::Result<()> {
    assert_silent_write(
        "POST",
        "/2.2/networks/network-0001/burst_reporters",
        Wire::EmptyJsonString,
        |transport| async move {
            BurstReportersApi::new(transport)
                .create_burst_reporter("network-0001", None, None)
                .await
        },
    )
    .await
}

// ===================== runtime: the logged text is the scanned text =====================

/// The operation texts of every `Issuing write (..)` warning in `logs`, oldest first.
fn warned_operations(logs: &LogCapture) -> Vec<String> {
    logs.messages(Level::WARN)
        .iter()
        .filter_map(|message| message.strip_prefix("Issuing write ("))
        .filter_map(|rest| rest.split_once("): its side effects"))
        .map(|(operation, _)| operation.to_owned())
        .collect()
}

/// Mounts a catch-all mock that accepts any request once.
async fn mount_any(mock: &MockEero) {
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_envelope()))
        .expect(1)
        .mount(&mock.server)
        .await;
}

/// The deprecated non-empty body is the one case where `create_burst_reporter` warns.
#[tokio::test]
async fn create_burst_reporter_with_a_deprecated_body_warns_once() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_any(&mock).await;
    let (_guard, logs) = capture_logs();
    BurstReportersApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
        .create_burst_reporter("network-0001", Some(json!({ "type": "burst" })), None)
        .await?;
    assert_eq!(
        warned_operations(&logs),
        ["request live-usage burst with a deprecated body for network"]
    );
    Ok(())
}

/// A write that reaches its warning through a helper's `operation` parameter logs the expanded
/// text, the one the scan resolves for it.
#[tokio::test]
async fn a_helper_operation_logs_the_expanded_text() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_any(&mock).await;
    let (_guard, logs) = capture_logs();
    SecurityApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
        .set_wpa3("network-0001", true, None)
        .await?;
    assert_eq!(
        warned_operations(&logs),
        [concat!(
            "set WPA3 for network",
            " -- may reboot every eero, like the confirmed DNS write path"
        )]
    );
    assert!(
        operation_is_scanned("security", &warned_operations(&logs)[0]),
        "the logged text must be one the scan resolves"
    );
    Ok(())
}

/// A `format!` operation logs the validated action, and never the eero id.
#[tokio::test]
async fn a_format_operation_logs_the_action_and_not_the_eero_id() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_any(&mock).await;
    let (_guard, logs) = capture_logs();
    EerosApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
        .node_action("eero-0001", "POWER_CYCLE_ALL_PORTS", None)
        .await?;
    let operations = warned_operations(&logs);
    assert_eq!(operations.len(), 1, "{operations:?}");
    assert!(operations[0].starts_with("perform node action 'POWER_CYCLE_ALL_PORTS' on eero --"));
    assert!(!operations[0].contains("eero-0001"));
    assert!(
        operation_is_scanned(
            "eeros",
            &operations[0].replace("'POWER_CYCLE_ALL_PORTS'", "'{action}'")
        ),
        "the logged text must match a template the scan resolves"
    );
    Ok(())
}

/// `set_profile_devices` warns twice, exactly as `profiles.py` does, which is why `EXTRA_PATHS`
/// records an extra "update profile" path.
#[tokio::test]
async fn set_profile_devices_warns_twice() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    mount_any(&mock).await;
    let (_guard, logs) = capture_logs();
    ProfilesApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
        .set_profile_devices("network-0001", "profile-0001", &[], None)
        .await?;
    assert_eq!(
        warned_operations(&logs),
        ["set devices for profile", "update profile"]
    );
    Ok(())
}

/// Whether the scan resolves `operation` for `module`.
fn operation_is_scanned(module: &str, operation: &str) -> bool {
    actual_operations(&sources())
        .iter()
        .any(|(found_module, found)| found_module == module && found == operation)
}
