//! One module per Python domain module in `eero-api`'s `src/eero/api/`.
//!
//! The 1:1 file map is deliberate: it keeps `PARITY.md` checkable row by row and makes an
//! upstream drift fix local to a single file. `activity` is not ported — every one of its
//! endpoints 404s on both API versions (eero-api #107).

pub mod ac_compat;
pub mod backup;
pub mod blacklist;
pub mod burst_reporters;
pub mod data_usage;
pub mod devices;
pub mod diagnostics;
pub mod dns;
pub mod eeros;
pub mod forwards;
pub mod insights;
pub mod networks;
pub mod ouicheck;
pub mod password;
pub mod profiles;
pub mod reservations;
pub mod routing;
pub mod schedule;
pub mod security;
pub mod settings;
pub mod sqm;
pub mod support;
pub mod thread;
pub mod transfer;
pub mod updates;

// Re-exports so a consumer (and `crate::api`, the `EeroApi` aggregator) can write
// `endpoints::NetworksApi` instead of `endpoints::networks::NetworksApi`. The `pub mod`
// declarations above are kept as-is — these re-exports are additive, not a replacement for the
// 1:1 file map this module's own docs describe.
pub use ac_compat::ACCompatApi;
pub use backup::BackupApi;
pub use blacklist::BlacklistApi;
pub use burst_reporters::BurstReportersApi;
pub use data_usage::DataUsageApi;
pub use devices::DevicesApi;
pub use diagnostics::DiagnosticsApi;
pub use dns::DnsApi;
pub use eeros::EerosApi;
pub use forwards::ForwardsApi;
pub use insights::InsightsApi;
pub use networks::NetworksApi;
pub use ouicheck::OUICheckApi;
pub use password::PasswordApi;
pub use profiles::ProfilesApi;
pub use reservations::ReservationsApi;
pub use routing::RoutingApi;
pub use schedule::ScheduleApi;
pub use security::SecurityApi;
pub use settings::SettingsApi;
pub use sqm::SqmApi;
pub use support::SupportApi;
pub use thread::ThreadApi;
pub use transfer::TransferApi;
pub use updates::UpdatesApi;
