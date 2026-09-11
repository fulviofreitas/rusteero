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
