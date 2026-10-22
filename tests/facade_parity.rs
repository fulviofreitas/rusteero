//! Every domain-API method has a `Client` wrapper, and the wrappers are exactly the ones listed
//! here.
//!
//! This is the Rust form of `eero-api`'s `tests/api/test_facade_parity.py` at v8.4.1. Python
//! builds an `EeroAPI`, walks the public coroutines of every domain class, and asserts that the
//! source of `EeroClient` contains `self._api.<attribute>.<method>(` for each, unless the pair is
//! allowlisted because the facade reaches the capability some other way. The point is
//! reachability: `Client` is what most callers use, so a domain method without a wrapper is
//! effectively unreachable for them. Rust has no runtime reflection, so the guard is split in
//! two, and a table ties the halves together:
//!
//! 1. **A compile-time table.** [`facade!`] lists, grouped by domain module, every domain method
//!    that has a `Client` wrapper at v8.4.1, `method` when the wrapper has the same name and
//!    `method => wrapper` when it does not. The same invocation expands to one `#[test]` per
//!    domain in [`wrapper_exists`] that names each wrapper as a function item
//!    (`Client::set_account_name`), so a wrapper that is renamed or removed fails to compile, and
//!    to the [`FACADE`] rows the scans below read.
//! 2. **A source scan.** It reads `src/endpoints/*.rs` and `src/client/*.rs` and pins both
//!    directions:
//!    - every `pub async fn` of a domain module is either a [`FACADE`] row or an [`ALLOWLIST`]
//!      entry, and each row's wrapper is a `pub async fn` of `impl Client` whose body calls
//!      `.<domain>().<method>(` (Python's `self._api.<attribute>.<method>(` needle);
//!    - every call to a domain method from a public `Client` method is a [`FACADE`] row, so a
//!      wrapper added or renamed without being listed here fails too.
//!
//! The wrapper names are the Python names, and the scan finds exactly Python's wrapper for every
//! domain method: the renames are Python's (`AccountApi::set_name` is `Client::set_account_name`,
//! `NotificationsApi::get_settings` is `Client::get_notification_settings`, ...), and
//! `DevicesApi::get_device` has two wrappers, `get_device` and `get_device_priority`.
//!
//! # The allowlist
//!
//! [`ALLOWLIST`] holds the domain methods that no wrapper calls directly. Its two eero-api entries
//! are Python's whole allowlist (`EerosAPI.set_nightlight_brightness` and
//! `set_nightlight_schedule`: the wrapper of the same name delegates to `set_nightlight`); the
//! third is a Rust-only deprecated shim. An entry is legitimate only when the facade exposes the
//! capability through a delegate, and the scan pins the delegate. A wrapper that eero-api has and
//! this crate lacks is a mismatch to fix, never an entry.
//!
//! # What is out of scope
//!
//! `AuthApi` is not a domain module, as in Python. The session half of its facade counterparts is
//! in [`SESSION`]; Python's `login`, `verify` and `resend_verification_code` are
//! `LoginFlow::start`, `PendingLogin::verify` and `PendingLogin::resend` here
//! ([`login_handshake_lives_on_the_flow_types`]), and `EeroClient.close` has no counterpart
//! because the HTTP client is released on drop.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusteero::Client;
use rusteero::auth::Session;
use rusteero::auth::flow::{LoginFlow, PendingLogin};
use rusteero::endpoints::networks::NetworksApi;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use common::{MockEero, TEST_TOKEN, session_cookie, user_token_header};

// ===================== the compile-time table =====================

/// One facade row: `(domain module, domain method, Client wrapper)`.
type Row = (&'static str, &'static str, &'static str);

/// Names a function item, so a path that does not resolve to one fails to compile.
fn exists<F>(_function: F) {}

/// The `Client` path of a wrapper: the second name when the row renames it, else the first.
macro_rules! wrapper_path {
    ($method:ident) => {
        Client::$method
    };
    ($method:ident $wrapper:ident) => {
        Client::$wrapper
    };
}

/// The name of a wrapper, by the same rule as [`wrapper_path!`].
macro_rules! wrapper_name {
    ($method:ident) => {
        stringify!($method)
    };
    ($method:ident $wrapper:ident) => {
        stringify!($wrapper)
    };
}

/// Declares the facade rows and the compile-time reference to every wrapper they name.
macro_rules! facade {
    ($($domain:ident { $($method:ident $(=> $wrapper:ident)?),+ $(,)? })+) => {
        /// Every domain method that has a `Client` wrapper at eero-api v8.4.1, as
        /// `(domain module, domain method, wrapper)`, in Python's order.
        const FACADE: &[Row] = &[
            $($((stringify!($domain), stringify!($method), wrapper_name!($method $($wrapper)?)),)+)+
        ];

        /// One test per domain module: each wrapper the table names is a function item of
        /// `Client`. The assertion is that this module compiles.
        mod wrapper_exists {
            use super::{exists, Client};
            $(
                #[test]
                fn $domain() {
                    $(exists(wrapper_path!($method $($wrapper)?));)+
                }
            )+
        }
    };
}

facade! {
    ac_compat {
        get_ac_compat,
    }
    account {
        get_account,
        set_name => set_account_name,
        set_email => set_account_email,
        verify_email => verify_account_email,
        set_phone => set_account_phone,
        verify_phone => verify_account_phone,
        set_consents => set_account_consents,
        get_sms_countries,
    }
    backup {
        get_backup_internet,
        set_backup_internet,
        get_cellular_backup_usage,
        get_cellular_backup_events,
    }
    backup_access_points {
        list => list_backup_access_points,
        add => add_backup_access_point,
        update => update_backup_access_point,
        delete_backup_access_point,
        rearrange => rearrange_backup_access_points,
        discover_ssids => discover_backup_ssids,
        start_ssid_discovery => start_backup_ssid_discovery,
        connectivity_check => backup_connectivity_check,
    }
    blacklist {
        get_blacklist,
        add_to_blacklist,
        remove_from_blacklist,
    }
    burst_reporters {
        create_burst_reporter,
    }
    data_usage {
        get_data_usage,
        get_breakdown => get_data_usage_breakdown,
        get_devices_usage => get_devices_data_usage,
        get_device_usage => get_device_data_usage,
        get_eeros_usage => get_eeros_data_usage,
        get_eeros_summary => get_eeros_data_usage_summary,
        get_eero_usage => get_eero_data_usage,
        get_profile_usage => get_profile_data_usage,
        get_unprofiled_devices => get_unprofiled_devices_data_usage,
        get_unprofiled_summary => get_unprofiled_data_usage_summary,
        get_report_settings => get_data_usage_report_settings,
        set_report_settings => set_data_usage_report_settings,
    }
    ddns {
        enable => enable_ddns,
        disable => disable_ddns,
    }
    devices {
        get_devices,
        get_device,
        get_device => get_device_priority,
        set_device_nickname,
        pause_device,
        update_device_via_link,
        set_device_type,
        get_device_labels,
        set_device_labels,
        block_device,
        unblock_device,
    }
    dhcp {
        set_dhcp,
        set_connection_mode,
        set_nat_port_randomization,
        set_pppoe,
    }
    diagnostics {
        get_diagnostics,
        run_diagnostics,
    }
    dns {
        get_dns_settings,
        set_dns_caching,
        set_custom_dns,
        set_custom_dns_ipv4,
        set_custom_dns_ipv6,
        clear_custom_dns,
        set_dns_mode,
    }
    dns_policies {
        get_advanced_content_filter,
        allow_domain,
        allow_cnames,
        block_domain,
        allow_domain_for_profiles,
        allow_cnames_for_profiles,
        block_domain_for_profiles,
        get_profile_applications => get_dns_policy_applications,
        set_profile_blocked_applications,
        set_network_ad_block,
        set_network_block_malware,
    }
    eeros {
        get_eeros,
        get_eero,
        reboot_eero,
        get_led_status,
        set_led,
        set_led_brightness,
        set_location,
        get_nightlight,
        set_nightlight,
        get_connections,
        node_action,
        port_action,
        led_cycle,
        nightlight_override,
        get_eero_support,
    }
    entitlements {
        get_features => get_entitlement_features,
        get_upsell_features,
        get_model_capabilities,
        get_premium_customer,
    }
    events {
        get_app_events,
        get_network_scan,
        get_channel_utilization,
    }
    forwards {
        get_forwards,
        create_forward,
        update_forward,
        delete_forward,
    }
    insights {
        get_insights,
        get_devices_insights,
        get_device_insights,
        get_profiles_insights,
        get_profile_insights,
        get_profile_devices_insights,
    }
    members {
        get_members,
        get_invites,
        create_invite,
        update_invite,
        delete_invite,
        respond_to_invite,
        cancel_pending_admin,
        promote_member,
        remove_admin,
        query_invite,
    }
    networks {
        get_networks,
        get_network,
        get_premium_status,
        reboot_network,
        run_speed_test,
        get_speed_tests,
        set_network_name,
        set_network_password,
        clear_network_password,
        get_guest_network,
        set_guest_network,
        set_guest_password,
        clear_guest_password,
        set_hide_5g,
    }
    notifications {
        get_settings => get_notification_settings,
        set_settings => set_notification_settings,
        has_unread => has_unread_notifications,
        mark_read => mark_notifications_read,
        get_history => get_notification_history,
        set_push_settings,
    }
    ouicheck {
        get_ouicheck,
    }
    permissions {
        get_permissions,
    }
    power_saving {
        set_power_saving,
        get_schedules => get_power_saving_schedules,
        create_schedule => create_power_saving_schedule,
        update_schedule => update_power_saving_schedule,
        delete_schedule => delete_power_saving_schedule,
    }
    profiles {
        get_profiles,
        get_profile,
        pause_profile,
        get_profile_devices,
        set_profile_devices,
        create_profile,
        rename_profile,
        delete_profile,
    }
    reservations {
        get_reservations,
        create_reservation,
        update_reservation,
        delete_reservation,
    }
    routing {
        get_routing,
    }
    schedule {
        get_schedules,
        create_schedule,
        update_schedule,
        delete_schedule,
        clear_profile_schedule,
        enable_bedtime,
        set_weekday_bedtime,
        set_weekend_bedtime,
    }
    security {
        get_security_settings,
        set_wpa3,
        set_band_steering,
        set_upnp,
        set_ipv6,
        set_ipv6_upstream,
        configure_security,
        set_mlo_mode,
        get_fast_transition,
        set_fast_transition,
        set_passpoint_enabled,
        set_proxied_nodes,
    }
    sqm {
        get_sqm_settings,
        set_sqm,
    }
    subnets {
        get_config => get_subnets_config,
        set_config => set_subnets_config,
        delete_subnet,
        set_content_filters => set_subnet_content_filters,
        get_content_filters => get_subnet_content_filters,
    }
    support {
        get_support,
        request_support,
    }
    thread {
        get_thread,
        set_thread_enabled,
        update_thread,
        regenerate_thread_credentials,
    }
    transfer {
        get_transfer_stats,
    }
    updates {
        get_updates,
        apply_update,
        set_preferred_update_hour,
    }
    wan {
        get_multistaticip,
        set_multistaticip,
        set_secondary_wan_config,
        set_device_secondary_wan_access,
    }
    wpa3 {
        get_wpa3_per_band,
        set_wpa3_per_band,
    }
}

/// A domain method that no `Client` wrapper calls directly, and why that is right.
struct Delegated {
    /// The domain module the method is in.
    domain: &'static str,
    /// The domain method.
    method: &'static str,
    /// The `Client` method that exposes the capability.
    wrapper: &'static str,
    /// The call, with all whitespace removed, that `wrapper` makes instead.
    via: &'static str,
    /// Why the wrapper does not call `domain.method` itself.
    reason: &'static str,
}

/// The domain methods with no direct wrapper call. Python's allowlist is the first two entries
/// verbatim; the third is a deprecated Rust-only method with no Python counterpart.
const ALLOWLIST: &[Delegated] = &[
    Delegated {
        domain: "eeros",
        method: "set_nightlight_brightness",
        wrapper: "set_nightlight_brightness",
        via: "self.set_nightlight(",
        reason: "the wrapper of the same name is a convenience that delegates to \
                 `Client::set_nightlight(brightness_percentage)`, the one wrapper that calls \
                 `EerosApi::set_nightlight`",
    },
    Delegated {
        domain: "eeros",
        method: "set_nightlight_schedule",
        wrapper: "set_nightlight_schedule",
        via: "self.set_nightlight(",
        reason: "the wrapper of the same name delegates to `Client::set_nightlight(schedule)` \
                 in the same way",
    },
    Delegated {
        domain: "networks",
        method: "get_account",
        wrapper: "get_account",
        via: ".account().get_account(",
        reason: "a deprecated shim for `AccountApi::get_account`, kept for source compatibility \
                 and absent from eero-api: `Client::get_account` reads the same resource through \
                 `AccountApi`, as `EeroClient.get_account` does",
    },
];

/// The `Client` methods that wrap the session half of eero-api's `AuthAPI`: no domain module is
/// involved, so they are not [`FACADE`] rows.
const SESSION: &[&str] = &[
    "logout",
    "set_session_token",
    "clear_session_token",
    "clear_auth_data",
];

/// Names each allowlisted wrapper and each session method as a function item.
#[test]
fn delegating_and_session_wrappers_exist() {
    exists(Client::set_nightlight_brightness);
    exists(Client::set_nightlight_schedule);
    exists(Client::get_account);
    exists(Client::logout);
    exists(Client::set_session_token);
    exists(Client::clear_session_token);
    exists(Client::clear_auth_data);
}

/// Python's `login`, `verify` and `resend_verification_code` are the login handshake, which is
/// separable from `Client` here: `LoginFlow::start` begins it and `PendingLogin` finishes it.
#[test]
fn login_handshake_lives_on_the_flow_types() {
    exists(LoginFlow::start);
    exists(PendingLogin::resend);
    exists(PendingLogin::verify);
}

// ===================== reading the sources =====================

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
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
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

/// The index just past the comment or string, raw string or char literal that starts at `at`,
/// if any.
fn skipped_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes[at..].starts_with(b"//") {
        return Some(
            bytes[at..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |n| at + n),
        );
    }
    if bytes[at..].starts_with(b"/*") {
        let mut depth = 0_usize;
        let mut end = at;
        while end < bytes.len() {
            if bytes[end..].starts_with(b"/*") {
                depth += 1;
                end += 2;
            } else if bytes[end..].starts_with(b"*/") {
                depth -= 1;
                end += 2;
                if depth == 0 {
                    break;
                }
            } else {
                end += 1;
            }
        }
        return Some(end);
    }
    match bytes[at] {
        b'"' => Some(string_end(bytes, at)),
        b'r' => raw_string_end(bytes, at),
        b'\'' => char_end(bytes, at),
        _ => None,
    }
}

/// `source` with every comment and every string, raw string and char literal replaced by spaces
/// (newlines kept), so a search for code can match neither commentary nor text.
fn mask(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if let Some(end) = skipped_end(bytes, at) {
            out.extend(
                bytes[at..end]
                    .iter()
                    .map(|&b| if b == b'\n' { b'\n' } else { b' ' }),
            );
            at = end;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).expect("only whole comments and literals are replaced, by ASCII spaces")
}

/// The index of the brace that closes the one opened at `open`, in masked code.
fn matching_brace(code: &str, open: usize) -> usize {
    let mut depth = 0_usize;
    for (at, byte) in code.bytes().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return at;
                }
            }
            _ => {}
        }
    }
    panic!("no brace closes the one at byte {open}");
}

/// The masked production code of the file at `path`: everything before its first `#[cfg(test)]`
/// item.
fn production_code(path: &Path) -> String {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    production_text(&text)
}

/// The masked production code of a source file's `text`, with line endings normalised the way
/// rustc normalises them before lexing: a Windows checkout with `core.autocrlf` has CRLF.
fn production_text(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    let production = text
        .find("\n#[cfg(test)]")
        .map_or(text.as_str(), |cut| &text[..cut]);
    mask(production)
}

/// The `.rs` files directly under `src/<directory>`, sorted by name.
fn source_files(directory: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(directory);
    let mut files: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", root.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|file| file.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    files
}

/// One function found in masked code.
struct Function {
    name: String,
    /// Whether it is declared `pub async fn`.
    public_async: bool,
    /// Everything from its `fn` keyword to the next function's, with all whitespace removed, so a
    /// call chain that rustfmt broke over several lines reads as one string.
    text: String,
}

/// Whether `code` has an identifier character at `at`.
fn is_ident_at(code: &str, at: usize) -> bool {
    code.as_bytes()
        .get(at)
        .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The identifier `text` starts with.
fn identifier_at(text: &str) -> &str {
    let end = text
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || b == b'_'))
        .unwrap_or(text.len());
    &text[..end]
}

/// The functions defined in masked `code`, in source order. A function ends where the next one
/// starts, so a call belongs to the nearest `fn` before it.
fn functions(code: &str) -> Vec<Function> {
    let starts: Vec<(usize, &str)> = code
        .match_indices("fn")
        .filter(|&(at, _)| at == 0 || !is_ident_at(code, at - 1))
        .filter_map(|(at, _)| {
            let after = &code[at + 2..];
            let name = identifier_at(after.trim_start());
            (after.starts_with(char::is_whitespace) && !name.is_empty()).then_some((at, name))
        })
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(index, &(at, name))| {
            let end = starts.get(index + 1).map_or(code.len(), |&(next, _)| next);
            Function {
                name: name.to_owned(),
                public_async: code[..at].trim_end().ends_with("pub async"),
                text: code[at..end].split_whitespace().collect(),
            }
        })
        .collect()
}

/// The `.<accessor>().<method>(` call chains in whitespace-free code, as `(accessor, method)`.
fn chained_calls(squashed: &str) -> Vec<(&str, &str)> {
    squashed
        .match_indices("().")
        .filter_map(|(at, _)| {
            let head = &squashed[..at];
            let start = head
                .bytes()
                .rposition(|b| !(b.is_ascii_alphanumeric() || b == b'_'))
                .map_or(0, |found| found + 1);
            let accessor = &head[start..];
            let rest = &squashed[at + 3..];
            let method = identifier_at(rest);
            (!accessor.is_empty()
                && start > 0
                && head.as_bytes()[start - 1] == b'.'
                && !method.is_empty()
                && rest[method.len()..].starts_with('('))
            .then_some((accessor, method))
        })
        .collect()
}

/// The public async functions of each domain module under `src/endpoints/`, by module name.
fn domain_methods() -> BTreeMap<String, BTreeSet<String>> {
    source_files("endpoints")
        .into_iter()
        .filter(|file| file.file_stem().is_some_and(|stem| stem != "mod"))
        .map(|file| {
            let module = file
                .file_stem()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            let methods = functions(&production_code(&file))
                .into_iter()
                .filter(|function| function.public_async)
                .map(|function| function.name)
                .collect();
            (module, methods)
        })
        .collect()
}

/// The functions of every `impl Client { .. }` block under `src/client/`, by name. Private helpers
/// are included; builder methods are not, because they are in `impl ClientBuilder`.
fn client_functions() -> BTreeMap<String, Function> {
    let mut found = BTreeMap::new();
    for file in source_files("client") {
        let code = production_code(&file);
        for (at, header) in code.match_indices("impl Client {") {
            let open = at + header.len() - 1;
            let block = &code[open + 1..matching_brace(&code, open)];
            for function in functions(block) {
                let previous = found.insert(function.name.clone(), function);
                assert!(previous.is_none(), "two `Client` functions share a name");
            }
        }
    }
    found
}

/// The `(domain, method)` pairs that have a [`FACADE`] row.
fn listed_methods() -> BTreeSet<(&'static str, &'static str)> {
    FACADE
        .iter()
        .map(|&(domain, method, _)| (domain, method))
        .collect()
}

/// The `(domain, method)` pairs that have an [`ALLOWLIST`] entry.
fn allowlisted_methods() -> BTreeSet<(&'static str, &'static str)> {
    ALLOWLIST
        .iter()
        .map(|entry| (entry.domain, entry.method))
        .collect()
}

// ===================== the source scans =====================

/// The scan must not agree with the table vacuously: it finds a realistic surface, the same floor
/// Python's discovery test sets (30 domain classes, 150 methods).
#[test]
fn discovery_finds_a_realistic_surface() {
    let domains = domain_methods();
    let methods: usize = domains.values().map(BTreeSet::len).sum();
    assert!(
        domains.len() >= 30,
        "found only {} domain modules",
        domains.len()
    );
    assert!(methods >= 150, "found only {methods} domain methods");
    let public = client_functions()
        .values()
        .filter(|function| function.public_async)
        .count();
    assert!(
        public >= 150,
        "found only {public} public async `Client` methods"
    );
    assert_eq!(
        domains.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        FACADE
            .iter()
            .map(|&(domain, ..)| domain)
            .collect::<BTreeSet<_>>(),
        "the table must group exactly the domain modules there are"
    );
}

/// Every domain method has a wrapper that calls it, or is allowlisted; and the table names no
/// domain method that is not there.
#[test]
fn every_domain_method_has_a_client_wrapper() {
    let domains = domain_methods();
    let clients = client_functions();
    let (listed, allowlisted) = (listed_methods(), allowlisted_methods());
    let mut problems = Vec::new();
    for (domain, methods) in &domains {
        for method in methods {
            let key = (domain.as_str(), method.as_str());
            if allowlisted.contains(&key) {
                continue;
            }
            if !listed.contains(&key) {
                problems.push(format!(
                    "{domain}::{method} has no row in FACADE: add a wrapper and its row, or (only \
                     if the capability is reached through a delegate) an ALLOWLIST entry"
                ));
            }
        }
    }
    for &(domain, method, wrapper) in FACADE {
        if !domains
            .get(domain)
            .is_some_and(|methods| methods.contains(method))
        {
            problems.push(format!(
                "FACADE row {domain}::{method} names no domain method"
            ));
        }
        let needle = format!(".{domain}().{method}(");
        match clients.get(wrapper) {
            Some(function) if function.public_async && function.text.contains(&needle) => {}
            Some(function) if function.public_async => problems.push(format!(
                "Client::{wrapper} is listed as the wrapper of {domain}::{method} but never calls \
                 `{needle}`"
            )),
            _ => problems.push(format!(
                "Client::{wrapper} (the wrapper of {domain}::{method}) is not a `pub async fn` of \
                 `impl Client`"
            )),
        }
    }
    assert!(
        problems.is_empty(),
        "facade parity is broken:\n{}",
        problems.join("\n")
    );
}

/// The other direction: a public `Client` method that calls a domain method is listed as its
/// wrapper, so a wrapper added or renamed without a row fails.
#[test]
fn every_client_call_to_a_domain_method_is_a_table_row() {
    let domains = domain_methods();
    let rows: BTreeSet<Row> = FACADE.iter().copied().collect();
    let mut problems = Vec::new();
    for function in client_functions()
        .values()
        .filter(|function| function.public_async)
    {
        for (accessor, method) in chained_calls(&function.text) {
            let is_domain_method = domains
                .get(accessor)
                .is_some_and(|methods| methods.contains(method));
            if is_domain_method && !rows.contains(&(accessor, method, function.name.as_str())) {
                problems.push(format!(
                    "Client::{} calls {accessor}::{method} but FACADE has no such row",
                    function.name
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "unlisted wrappers:\n{}",
        problems.join("\n")
    );
}

/// An allowlist entry names a real domain method, whose wrapper is a public `Client` method that
/// reaches the capability through the recorded delegate and does not call the method itself (else
/// the entry is redundant), and it says why.
#[test]
fn allowlisted_methods_are_reached_through_their_delegate() {
    let domains = domain_methods();
    let clients = client_functions();
    for entry in ALLOWLIST {
        let Delegated {
            domain,
            method,
            wrapper,
            via,
            reason,
        } = *entry;
        assert!(
            domains
                .get(domain)
                .is_some_and(|methods| methods.contains(method)),
            "ALLOWLIST names {domain}::{method}, which is not a domain method"
        );
        let function = clients
            .get(wrapper)
            .filter(|function| function.public_async)
            .unwrap_or_else(|| {
                panic!("Client::{wrapper} is not a `pub async fn` of `impl Client`")
            });
        assert!(
            function.text.contains(via),
            "Client::{wrapper} must reach {domain}::{method} through `{via}`"
        );
        let direct = format!(".{domain}().{method}(");
        assert!(
            clients
                .values()
                .filter(|function| function.public_async)
                .all(|function| !function.text.contains(&direct)),
            "{domain}::{method} now has a wrapper that calls it: drop its ALLOWLIST entry"
        );
        assert!(reason.len() > 40, "{domain}::{method} needs a real reason");
    }
}

/// The session half of the facade is public `Client` methods.
#[test]
fn session_methods_are_public_async_client_methods() {
    let clients = client_functions();
    for name in SESSION {
        assert!(
            clients
                .get(*name)
                .is_some_and(|function| function.public_async),
            "Client::{name} is not a `pub async fn` of `impl Client`"
        );
    }
}

/// Negative control for the scanners: on a synthetic `impl Client` they ignore calls in comments
/// and strings, join a call chain that is split over lines, and attribute each call to the
/// nearest function before it.
#[test]
fn the_scanners_read_a_synthetic_client() {
    let code = mask(
        r#"
        impl Client {
            pub async fn get_thing(&self) -> Result<Envelope, Error> {
                // self.api.things().get_commented_out();
                let text = "self.api.things().get_in_a_string()";
                self.api
                    .things()
                    .get_thing(&text)
                    .await
            }
            async fn helper(&self) {
                self.api.things().get_private().await;
            }
            pub fn sync_one(&self) {}
        }
        "#,
    );
    let found = functions(&code);
    let names: Vec<_> = found
        .iter()
        .map(|function| (function.name.as_str(), function.public_async))
        .collect();
    assert_eq!(
        names,
        [("get_thing", true), ("helper", false), ("sync_one", false)]
    );
    assert_eq!(chained_calls(&found[0].text), [("things", "get_thing")]);
    assert_eq!(chained_calls(&found[1].text), [("things", "get_private")]);
}

/// A Windows checkout has CRLF line endings (`core.autocrlf`); the scan must read a file the way
/// rustc does, as if they were LF: same functions, same cut at the test module.
#[test]
fn the_scanners_read_crlf_source_like_lf_source() {
    let lf = concat!(
        "impl Client {\n",
        "    pub async fn get_thing(&self) -> Result<Envelope, Error> {\n",
        "        let text = \"first \\\n            second\";\n",
        "        self.api\n",
        "            .things()\n",
        "            .get_thing(&text)\n",
        "            .await\n",
        "    }\n",
        "}\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    fn only_in_tests() {}\n",
        "}\n",
    );
    let crlf = lf.replace('\n', "\r\n");
    assert!(
        crlf.contains("\\\r\n"),
        "the CRLF form must keep its continuation"
    );
    let from_lf = production_text(lf);
    let from_crlf = production_text(&crlf);
    assert_eq!(from_crlf, from_lf);
    assert!(!from_crlf.contains('\r'));
    assert!(!from_crlf.contains("only_in_tests"));
    let found = functions(&from_crlf);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "get_thing");
    assert_eq!(chained_calls(&found[0].text), [("things", "get_thing")]);
}

// ===================== runtime: the deprecated shim is the wrapper's resource =====================

/// The deprecated `NetworksApi::get_account` has no wrapper of its own, and the allowlist says
/// `Client::get_account` reads the same resource through `AccountApi`. Both send `GET
/// /2.2/account` and yield the same envelope.
#[tokio::test]
#[allow(deprecated)]
async fn the_deprecated_shim_and_the_wrapper_read_the_same_resource() -> anyhow::Result<()> {
    let mock = MockEero::start().await;
    Mock::given(method("GET"))
        .and(path("/2.2/account"))
        .and(session_cookie())
        .and(user_token_header())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "meta": { "code": 200 }, "data": { "name": "Account" } })),
        )
        .expect(2)
        .mount(&mock.server)
        .await;

    let shim = NetworksApi::new(Arc::new(mock.transport_with_token(TEST_TOKEN)))
        .get_account()
        .await?;
    let client = Client::builder()
        .base_url(mock.uri())
        .session(Some(Session::from_token(TEST_TOKEN)))
        .build()
        .await?;
    let wrapper = client.get_account(true).await?;

    assert_eq!(shim, wrapper);
    Ok(())
}
