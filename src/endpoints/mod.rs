//! One module per Python domain module in `eero-api`'s `src/eero/api/`.
//!
//! The 1:1 file map is deliberate: it keeps `PARITY.md` checkable row by row and makes an
//! upstream drift fix local to a single file. `activity` is not ported — every one of its
//! endpoints 404s on both API versions (eero-api #107). `settings` and `password` were removed
//! upstream in `eero-api` v8.0.0 (`SettingsAPI`/`PasswordAPI` no longer exist) and are not
//! ported here either. 37 domain modules total: the 25 originally ported, minus those two
//! removals, plus the 14 new-in-v8.0.0 modules (`account`, `backup_access_points`, `ddns`,
//! `dhcp`, `dns_policies`, `entitlements`, `events`, `members`, `notifications`, `permissions`,
//! `power_saving`, `subnets`, `wan`, `wpa3`).

pub mod ac_compat;
pub mod account;
pub mod backup;
pub mod backup_access_points;
pub mod blacklist;
pub mod burst_reporters;
pub mod data_usage;
pub mod ddns;
pub mod devices;
pub mod dhcp;
pub mod diagnostics;
pub mod dns;
pub mod dns_policies;
pub mod eeros;
pub mod entitlements;
pub mod events;
pub mod forwards;
pub mod insights;
pub mod members;
pub mod networks;
pub mod notifications;
pub mod ouicheck;
pub mod permissions;
pub mod power_saving;
pub mod profiles;
pub mod reservations;
pub mod routing;
pub mod schedule;
pub mod security;
pub mod sqm;
pub mod subnets;
pub mod support;
pub mod thread;
pub mod transfer;
pub mod updates;
pub mod wan;
pub mod wpa3;

// Re-exports so a consumer (and `crate::api`, the `EeroApi` aggregator) can write
// `endpoints::NetworksApi` instead of `endpoints::networks::NetworksApi`. The `pub mod`
// declarations above are kept as-is — these re-exports are additive, not a replacement for the
// 1:1 file map this module's own docs describe.
pub use ac_compat::ACCompatApi;
pub use account::AccountApi;
pub use backup::BackupApi;
pub use backup_access_points::BackupAccessPointsApi;
pub use blacklist::BlacklistApi;
pub use burst_reporters::BurstReportersApi;
pub use data_usage::DataUsageApi;
pub use ddns::DdnsApi;
pub use devices::DevicesApi;
pub use dhcp::DhcpApi;
pub use diagnostics::DiagnosticsApi;
pub use dns::DnsApi;
pub use dns_policies::DnsPoliciesApi;
pub use eeros::EerosApi;
pub use entitlements::EntitlementsApi;
pub use events::EventsApi;
pub use forwards::ForwardsApi;
pub use insights::InsightsApi;
pub use members::MembersApi;
pub use networks::NetworksApi;
pub use notifications::NotificationsApi;
pub use ouicheck::OUICheckApi;
pub use permissions::PermissionsApi;
pub use power_saving::PowerSavingApi;
pub use profiles::ProfilesApi;
pub use reservations::ReservationsApi;
pub use routing::RoutingApi;
pub use schedule::ScheduleApi;
pub use security::SecurityApi;
pub use sqm::SqmApi;
pub use subnets::SubnetsApi;
pub use support::SupportApi;
pub use thread::ThreadApi;
pub use transfer::TransferApi;
pub use updates::UpdatesApi;
pub use wan::WanApi;
pub use wpa3::Wpa3Api;
