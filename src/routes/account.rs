//! `account` routes (`AccountAPI`, new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/account.py` (v8.0.4). Every method is a fixed path (no
//! `{id}`, no `network_id` at all) — `Resource::resolve` ignores its `id_or_url` argument
//! entirely for a template with no placeholder, so every `AccountApi` method calls
//! `self.transport.resource(&ROUTE, "", None, ...)` with an unused, ignored `id_or_url`.

use super::{ApiVersion, Resource};
use reqwest::Method;

/// `PUT /2.2/account/name` — set the account's display name (form-encoded `name=`).
///
/// Ported from `AccountAPI.set_name` (`account.py:51-72`).
pub const SET_ACCOUNT_NAME: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "account/name",
    link: None,
};

/// `PUT /2.2/account/email` — start an account email change (form-encoded `email=`).
///
/// Ported from `AccountAPI.set_email` (`account.py:74-98`). `email` is never logged.
pub const SET_ACCOUNT_EMAIL: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "account/email",
    link: None,
};

/// `POST /2.2/account/email/verify` — confirm a pending email change (form-encoded `code=`).
///
/// Ported from `AccountAPI.verify_email` (`account.py:99-122`). `code` is never logged.
pub const VERIFY_ACCOUNT_EMAIL: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "account/email/verify",
    link: None,
};

/// `PUT /2.2/account/phone` — start an account phone number change (form-encoded `phone=`).
///
/// Ported from `AccountAPI.set_phone` (`account.py:123-145`). `phone` is never logged.
pub const SET_ACCOUNT_PHONE: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "account/phone",
    link: None,
};

/// `POST /2.2/account/phone/verify` — confirm a pending phone number change (form-encoded
/// `code=`).
///
/// Ported from `AccountAPI.verify_phone` (`account.py:148-171`). `code` is never logged.
pub const VERIFY_ACCOUNT_PHONE: Resource = Resource {
    method: Method::POST,
    version: ApiVersion::V2_2,
    template: "account/phone/verify",
    link: None,
};

/// `PUT /2.2/account/consents` — set marketing-email consent (form-encoded
/// `marketing_emails=<"true"|"false">`).
///
/// Ported from `AccountAPI.set_consents` (`account.py:172-199`).
pub const SET_ACCOUNT_CONSENTS: Resource = Resource {
    method: Method::PUT,
    version: ApiVersion::V2_2,
    template: "account/consents",
    link: None,
};

/// `GET /2.2/countries/sms` — the SMS country-code catalogue.
///
/// Ported from `AccountAPI.get_sms_countries` (`account.py:200-217`).
pub const GET_SMS_COUNTRIES: Resource = Resource {
    method: Method::GET,
    version: ApiVersion::V2_2,
    template: "countries/sms",
    link: None,
};
