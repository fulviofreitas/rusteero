//! `AccountApi`: `account` endpoints (`eero-api src/eero/api/account.py`, new in v8.0.0).
//!
//! Manages the caller's own account profile: display name, email, phone, marketing consent, and
//! the SMS country-code catalogue. Every write in this module is unverified against a live
//! account: each logs one `WARNING` via [`warn_uncharacterised_write`] before it is issued.
//! Identifier values (email, phone, verification codes) are never logged, including at DEBUG —
//! this module logs only the fact a call was made, never an argument's value.
//!
//! Account deletion is deliberately not exposed by this module (a destructive, irreversible
//! operation, excluded by product decision — `account.py:16-18`).

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::error::Error;
use crate::links::warn_uncharacterised_write;
use crate::routes;
use crate::transport::{RequestBody, Transport};

/// `eero-api`'s `AccountAPI` (`src/eero/api/account.py`, new in v8.0.0).
///
/// Build one with [`AccountApi::new`], wrapping a [`Transport`] already shared with the rest of
/// the [`crate::api::EeroApi`] aggregator — `AccountApi` never constructs or owns a `Transport`
/// itself.
#[derive(Debug)]
pub struct AccountApi {
    transport: Arc<Transport>,
}

impl AccountApi {
    /// Wraps `transport` as an `AccountApi`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }

    /// Sets the account's display name — returns the raw Eero API response.
    ///
    /// Ported from `AccountAPI.set_name` (`account.py:51-72`). Sends a form-encoded body
    /// (`name=<value>`) to [`crate::routes::account::SET_ACCOUNT_NAME`]. Unverified: logs one
    /// `WARNING` via [`warn_uncharacterised_write`] before issuing the request (`account.py:70`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authentication`] if no valid session is configured, or whatever
    /// status-mapped [`Error`] the request produces otherwise.
    pub async fn set_name(&self, name: &str) -> Result<Envelope, Error> {
        warn_uncharacterised_write("set account name");
        self.transport
            .resource(
                &routes::account::SET_ACCOUNT_NAME,
                "",
                None,
                &[],
                RequestBody::Form(vec![("name".to_owned(), name.to_owned())]),
            )
            .await
    }

    /// Sets the account's email address — returns the raw Eero API response.
    ///
    /// Ported from `AccountAPI.set_email` (`account.py:74-98`). Sends a form-encoded body
    /// (`email=<value>`) to [`crate::routes::account::SET_ACCOUNT_EMAIL`]. The new address is
    /// not active until confirmed via [`AccountApi::verify_email`]. `email` is never logged.
    /// Unverified: logs one `WARNING` via [`warn_uncharacterised_write`] (`account.py:96`).
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn set_email(&self, email: &str) -> Result<Envelope, Error> {
        warn_uncharacterised_write("set account email");
        self.transport
            .resource(
                &routes::account::SET_ACCOUNT_EMAIL,
                "",
                None,
                &[],
                RequestBody::Form(vec![("email".to_owned(), email.to_owned())]),
            )
            .await
    }

    /// Confirms a pending email change with its verification code — returns the raw Eero API
    /// response.
    ///
    /// Ported from `AccountAPI.verify_email` (`account.py:99-122`). Sends a form-encoded body
    /// (`code=<value>`) to [`crate::routes::account::VERIFY_ACCOUNT_EMAIL`]. `code` is never
    /// logged. Unverified: logs one `WARNING` via [`warn_uncharacterised_write`]
    /// (`account.py:121`).
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn verify_email(&self, code: &str) -> Result<Envelope, Error> {
        warn_uncharacterised_write("verify account email");
        self.transport
            .resource(
                &routes::account::VERIFY_ACCOUNT_EMAIL,
                "",
                None,
                &[],
                RequestBody::Form(vec![("code".to_owned(), code.to_owned())]),
            )
            .await
    }

    /// Sets the account's phone number — returns the raw Eero API response.
    ///
    /// Ported from `AccountAPI.set_phone` (`account.py:123-145`). Sends a form-encoded body
    /// (`phone=<value>`) to [`crate::routes::account::SET_ACCOUNT_PHONE`]. The new number is not
    /// active until confirmed via [`AccountApi::verify_phone`]. `phone` is never logged.
    /// Unverified: logs one `WARNING` via [`warn_uncharacterised_write`] (`account.py:144`).
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn set_phone(&self, phone: &str) -> Result<Envelope, Error> {
        warn_uncharacterised_write("set account phone");
        self.transport
            .resource(
                &routes::account::SET_ACCOUNT_PHONE,
                "",
                None,
                &[],
                RequestBody::Form(vec![("phone".to_owned(), phone.to_owned())]),
            )
            .await
    }

    /// Confirms a pending phone number change with its verification code — returns the raw Eero
    /// API response.
    ///
    /// Ported from `AccountAPI.verify_phone` (`account.py:148-171`). Sends a form-encoded body
    /// (`code=<value>`) to [`crate::routes::account::VERIFY_ACCOUNT_PHONE`]. `code` is never
    /// logged. Unverified: logs one `WARNING` via [`warn_uncharacterised_write`]
    /// (`account.py:170`).
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn verify_phone(&self, code: &str) -> Result<Envelope, Error> {
        warn_uncharacterised_write("verify account phone");
        self.transport
            .resource(
                &routes::account::VERIFY_ACCOUNT_PHONE,
                "",
                None,
                &[],
                RequestBody::Form(vec![("code".to_owned(), code.to_owned())]),
            )
            .await
    }

    /// Sets the account's marketing-email consent — returns the raw Eero API response.
    ///
    /// Ported from `AccountAPI.set_consents` (`account.py:172-199`). Sends a form-encoded body
    /// (`marketing_emails=<"true"|"false">`) to
    /// [`crate::routes::account::SET_ACCOUNT_CONSENTS`]. Unverified: logs one `WARNING` via
    /// [`warn_uncharacterised_write`] (`account.py:194`).
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn set_consents(&self, marketing_emails: bool) -> Result<Envelope, Error> {
        warn_uncharacterised_write("set account consents");
        let value = if marketing_emails { "true" } else { "false" };
        self.transport
            .resource(
                &routes::account::SET_ACCOUNT_CONSENTS,
                "",
                None,
                &[],
                RequestBody::Form(vec![("marketing_emails".to_owned(), value.to_owned())]),
            )
            .await
    }

    /// Gets the SMS country-code catalogue — returns the raw Eero API response.
    ///
    /// Ported from `AccountAPI.get_sms_countries` (`account.py:200-217`). Sends `GET`
    /// [`crate::routes::account::GET_SMS_COUNTRIES`].
    ///
    /// # Errors
    ///
    /// See [`AccountApi::set_name`].
    pub async fn get_sms_countries(&self) -> Result<Envelope, Error> {
        self.transport
            .resource(
                &routes::account::GET_SMS_COUNTRIES,
                "",
                None,
                &[],
                RequestBody::None,
            )
            .await
    }
}
