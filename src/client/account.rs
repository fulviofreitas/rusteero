//! `Client` methods for the `AccountAPI` domain (new in v8.0.0).
//!
//! Ported from `eero-api src/eero/api/account.py` (v8.0.4) by way of `EeroClient`'s own
//! `account`-scoped wrappers in `client.py:2644-2680`. None of these seven methods take a
//! `network_id` at all — every one is account-scoped, not network-scoped.

use super::Client;
use crate::cache::CacheKey;
use crate::envelope::Envelope;
use crate::error::Error;

impl Client {
    /// Sets the account's display name — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_account_name()` (`client.py:2644-2648`). On success, resets the `account`
    /// cache bucket (`self._cache["account"] = {"data": None, "timestamp": 0}`,
    /// `client.py:2647`) — equivalent to invalidating [`CacheKey::Account`].
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_name(&self, name: &str) -> Result<Envelope, Error> {
        let response = self.api.account().set_name(name).await?;
        self.cache.invalidate(&CacheKey::Account);
        Ok(response)
    }

    /// Starts an account email change — returns the raw Eero API response (unverified write).
    ///
    /// Ported from `set_account_email()` (`client.py:2650-2652`). **Not invalidated** — only
    /// [`Client::verify_account_email`] resets the `account` cache bucket, matching Python
    /// exactly (`client.py:2651-2652` has no cache reset).
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
    /// Ported from `verify_account_email()` (`client.py:2654-2658`). On success, resets the
    /// `account` cache bucket (`client.py:2657`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn verify_account_email(&self, code: &str) -> Result<Envelope, Error> {
        let response = self.api.account().verify_email(code).await?;
        self.cache.invalidate(&CacheKey::Account);
        Ok(response)
    }

    /// Starts an account phone number change — returns the raw Eero API response (unverified
    /// write).
    ///
    /// Ported from `set_account_phone()` (`client.py:2660-2662`). Not invalidated — see
    /// [`Client::set_account_email`].
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
    /// Ported from `verify_account_phone()` (`client.py:2664-2668`). On success, resets the
    /// `account` cache bucket (`client.py:2667`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn verify_account_phone(&self, code: &str) -> Result<Envelope, Error> {
        let response = self.api.account().verify_phone(code).await?;
        self.cache.invalidate(&CacheKey::Account);
        Ok(response)
    }

    /// Sets the account's marketing-email consent — returns the raw Eero API response
    /// (unverified write).
    ///
    /// Ported from `set_account_consents()` (`client.py:2670-2673`). On success, resets the
    /// `account` cache bucket (`client.py:2672`).
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn set_account_consents(&self, marketing_emails: bool) -> Result<Envelope, Error> {
        let response = self.api.account().set_consents(marketing_emails).await?;
        self.cache.invalidate(&CacheKey::Account);
        Ok(response)
    }

    /// Gets the SMS country-code catalogue — returns the raw Eero API response.
    ///
    /// Ported from `get_sms_countries()` (`client.py:2676-2678`). Never cached, no `network_id`.
    ///
    /// # Errors
    ///
    /// Whatever status-mapped [`Error`] the request produces.
    pub async fn get_sms_countries(&self) -> Result<Envelope, Error> {
        self.api.account().get_sms_countries().await
    }
}
