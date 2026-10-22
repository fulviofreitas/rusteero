//! `Client` methods for the `AccountAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/account.py` (v8.4.1) by way of `EeroClient`'s own
//! `account`-scoped wrappers in `client.py:3441-3484`. None of these seven methods take a
//! `network_id` at all — every one is account-scoped, not network-scoped. (The cached read,
//! [`Client::get_account`], lives next to the other cached getters in `client/mod.rs`.)

use super::Client;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Sets the account's display name — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_account_name()` (`client.py:3450-3454` at `v8.4.1`). On success, resets
    /// the `account` cache entry through `_invalidate_account_cache` (`client.py:3441-3448`) —
    /// equivalent to invalidating [`crate::cache::CacheKey::Account`] — so a `get_account` that was already in
    /// flight cannot put the pre-write account back.
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_name(&self, name: &str) -> Result<Envelope, Error> {
        let response = self.api.account().set_name(name).await?;
        self.invalidate_account_cache();
        Ok(response)
    }

    /// Starts an account email change — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_account_email()` (`client.py:3456-3458` at `v8.4.1`). **Not
    /// invalidated** — only [`Client::verify_account_email`] resets the `account` cache entry,
    /// matching Python exactly (`client.py:3456-3458` has no cache reset).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_email(&self, email: &str) -> Result<Envelope, Error> {
        self.api.account().set_email(email).await
    }

    /// Verifies a pending account email change — returns the raw Eero API response (unverified
    /// write).
    ///
    /// Ported from `verify_account_email()` (`client.py:3460-3464` at `v8.4.1`). On success,
    /// resets the `account` cache entry (`client.py:3463`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn verify_account_email(&self, code: &str) -> Result<Envelope, Error> {
        let response = self.api.account().verify_email(code).await?;
        self.invalidate_account_cache();
        Ok(response)
    }

    /// Starts an account phone number change — returns the raw Eero API response (unverified
    /// write).
    ///
    /// Ported from `set_account_phone()` (`client.py:3466-3468` at `v8.4.1`). Not invalidated —
    /// see [`Client::set_account_email`].
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_phone(&self, phone: &str) -> Result<Envelope, Error> {
        self.api.account().set_phone(phone).await
    }

    /// Verifies a pending account phone number change — returns the raw Eero API response
    /// (unverified write).
    ///
    /// Ported from `verify_account_phone()` (`client.py:3470-3474` at `v8.4.1`). On success,
    /// resets the `account` cache entry (`client.py:3473`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn verify_account_phone(&self, code: &str) -> Result<Envelope, Error> {
        let response = self.api.account().verify_phone(code).await?;
        self.invalidate_account_cache();
        Ok(response)
    }

    /// Sets the account's marketing-email consent — returns the raw Eero API response
    /// (unverified write).
    ///
    /// Ported from `set_account_consents()` (`client.py:3476-3480` at `v8.4.1`). On success,
    /// resets the `account` cache entry (`client.py:3479`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_consents(&self, marketing_emails: bool) -> Result<Envelope, Error> {
        let response = self.api.account().set_consents(marketing_emails).await?;
        self.invalidate_account_cache();
        Ok(response)
    }

    /// Gets the SMS country-code catalogue — returns the raw Eero API response.
    ///
    /// Ported from `get_sms_countries()` (`client.py:3482-3484` at `v8.4.1`). Never cached, no
    /// `network_id`.
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn get_sms_countries(&self) -> Result<Envelope, Error> {
        self.api.account().get_sms_countries().await
    }
}
