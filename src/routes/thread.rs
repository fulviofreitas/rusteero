//! Thread (smart-home mesh) routes (`ThreadAPI`).

// ----------------------------------- thread (`ThreadAPI`) -------------------------------

use super::{ApiVersion, Route};
use reqwest::Method;

/// `GET /2.2/networks/{network_id}/thread` — Thread (smart-home mesh) status.
///
/// Ported from `eero-api src/eero/api/thread.py:33` (`ThreadAPI.get_thread`).
pub const GET_THREAD: Route = Route {
    method: Method::GET,
    version: ApiVersion::V2_2,
    path: "networks/{network_id}/thread",
};
