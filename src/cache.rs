//! In-memory TTL cache backing `Client`'s cached getters.
//!
//! There is no single Python file this module is "ported from" — `eero-api`'s cache is a plain
//! `dict` mixed directly into `EeroClient` (`eero-api src/eero/client.py:99-259` at `v8.4.1`).
//! This module pulls that logic out into a standalone, HTTP-free, unit-testable type so the
//! `Client` facade can hold one `Cache` and call [`Cache::get`] / [`Cache::put`] /
//! [`Cache::invalidate`] / [`Cache::invalidate_bucket`] / [`Cache::clear`] around its endpoint
//! calls, exactly mirroring the read-then-write shape of every Python cached getter:
//!
//! ```python
//! if not refresh_cache and self._is_cache_valid("account"):
//!     cached = self._get_from_cache("account")
//!     if cached:
//!         return cached
//!
//! cache_epoch = self._cache_epoch
//! response = await self._api.account.get_account()
//! self._update_cache("account", None, response, epoch=cache_epoch)
//! ```
//! (`eero-api src/eero/client.py:590-597` at `v8.4.1`, one of eight call sites with this shape).
//!
//! # Buckets and key shapes
//!
//! Reproduced verbatim from `eero-api src/eero/client.py:54-63` and every `cache_key = f"..."`
//! line that follows it (`:378`, `:428`, `:453`, `:484`, `:596`, `:627`, `:661`, `:665`):
//!
//! | Bucket      | Key shape                              | [`CacheKey`] variant             |
//! |-------------|-----------------------------------------|-----------------------------------|
//! | `account`   | flat, no key                           | [`CacheKey::Account`]            |
//! | `networks`  | flat, no key                           | [`CacheKey::Networks`]           |
//! | `network`   | `{nid}`                                | [`CacheKey::Network`]            |
//! | `eeros`     | `{nid}_eeros`                          | [`CacheKey::Eeros`]              |
//! | `devices`   | `{nid}_devices`                        | [`CacheKey::Devices`]            |
//! | `devices`   | `{nid}_{did}`                          | [`CacheKey::Device`]             |
//! | `profiles`  | `{nid}_profiles`                       | [`CacheKey::Profiles`]           |
//! | `profiles`  | `{nid}_{pid}`                          | [`CacheKey::Profile`]            |
//!
//! # The falsy-value rule
//!
//! **This is a faithful port of a real Python quirk — do not "fix" it.** Falsy cached values are
//! never served. `eero-api` guards every cached read with `if cached:` (e.g. `client.py:249-250,
//! 275-276, 352-353, 381-382, 456-457, 487-488, 599-600, 630-631`) — a Python truthiness check — so
//! a cached value that is falsy is treated as a cache miss and silently refetched, even though a
//! timestamp says the entry is still "fresh". Python truthiness maps onto the JSON shapes this
//! crate stores as: `None` → JSON `null`, `False` → JSON `false`, `0`/`0.0` → a numeric zero, `""`
//! → an empty string, `[]` → an empty array, `{}` → an empty object. [`Cache::get`] reproduces
//! every one of these via the private `is_falsy` helper: an [`Envelope`] whose wire value
//! (`Envelope::as_value`) is any of the six degenerate shapes above is treated as a miss. The stale
//! entry is left in place (Python doesn't delete it either); it is simply overwritten the next time
//! the caller writes a fresh value via [`Cache::put`].
//!
//! # `refresh_cache` and `cache_timeout = 0`
//!
//! Neither of these is cache-internal state — both are caller decisions, exactly as in Python:
//!
//! - `refresh_cache=True` (`client.py:247` et al.) skips the `_is_cache_valid` **read** but still
//!   calls `_update_cache` on the fresh response — i.e. it bypasses [`Cache::get`] for one call
//!   but still calls [`Cache::put`]. `Cache` has no `refresh_cache` parameter; the caller simply
//!   does not call [`Cache::get`] on that call.
//! - `cache_timeout = 0` (constructor default is `60`, `client.py:41`) disables reads: the
//!   validity check `(now - timestamp) < cache_timeout` (`client.py:92-93`) can never be true
//!   when `cache_timeout` is `0` (elapsed time is never negative), so every read is a miss, but
//!   `_update_cache` is unconditional — writes still happen. [`Cache::new`] with
//!   [`Duration::ZERO`] reproduces this: [`Cache::get`] short-circuits to `None` without even
//!   touching the map, while [`Cache::put`] is unaffected.
//!
//! # Invalidation
//!
//! [`Cache::clear`] mirrors `clear_cache()` (`client.py:252-259` at `v8.4.1`, called from
//! `verify`, `logout`, and every credential-clearing path). [`Cache::invalidate`] mirrors the
//! many single-key deletes scattered through `client.py` (`_invalidate_device_cache`,
//! `_invalidate_profile_cache`, `_invalidate_profiles_list_cache`, and the inline
//! `del self._cache[bucket][key]` after reboot/LED/nightlight, guest-network, speed-test and
//! network-rename writes). **`account` and `networks` are never invalidated by any Python writer
//! except the four account writes** — nothing else in this crate calls `invalidate` on those
//! keys either; that is a `Client`-level fact about which methods it calls, not something
//! `Cache` enforces.
//!
//! # Reads in flight during an invalidation
//!
//! Every invalidator ([`Cache::invalidate`], [`Cache::invalidate_bucket`], [`Cache::clear`])
//! advances one cache-wide generation counter, whether or not it removed anything. A cached read
//! takes a snapshot of that counter after its cache check and before it awaits the request, and
//! stores its response only if the counter is still the same afterwards — so a read that started
//! before a write and finished after it hands its response to its own caller but cannot put the
//! pre-write snapshot back in the cache for everyone else. This mirrors `_cache_epoch` /
//! `_update_cache(.., epoch=)` (`client.py:111-113`, `:198-229` at `v8.4.1`). The counter is
//! cache-wide on purpose: invalidating one resource also blocks the fill of an unrelated read
//! that is in flight, which costs at most one extra fetch and never leaves stale data behind.
//! [`Cache::put`] stays unconditional, and neither [`Cache::put`] nor the expiry pruning done by
//! [`Cache::get`] advances the counter. The counter lives under the same lock as the entries, so
//! the check and the insert in the guarded store cannot be separated by an invalidation.
//!
//! Two behaviours are intentionally **safer** than `eero-api`. Both are marked
//! `Divergence from eero-api:` at the exact point they apply:
//!
//! - (a) see [`Cache::clear`] — clearing removes timestamps, not just values.
//! - (b) is a **contract for the `Client` facade**, not code in this module (`Cache` has no
//!   notion of "LED brightness" or "DNS settings"): `eero-api`'s `set_led_brightness` never
//!   invalidates `eeros`, and its DNS/SQM/security setters never invalidate `network`, even
//!   though the written fields are part of those cached objects. The facade therefore calls
//!   `invalidate(&CacheKey::eeros(network_id))` after a successful LED-brightness write, and
//!   `invalidate(&CacheKey::network(network_id))` after a successful DNS, SQM or security write.
//!
//! # Expired entries
//!
//! [`Cache::get`] removes an entry it finds expired, as `_is_cache_valid` does
//! (`client.py:162-196` at `v8.4.1`), so a long-lived client does not accumulate devices and
//! profiles it will never serve again. Entries that are never read again stay until a write or
//! [`Cache::clear`] removes them.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use serde_json::Value;
use tokio::time::Instant;

use crate::envelope::Envelope;

/// One of the four **per-network** cache buckets from `eero-api src/eero/client.py:56-62`,
/// scoped to a single network id when used with [`Cache::invalidate_bucket`].
///
/// `account` and `networks` are deliberately not variants here: both are flat, single-key
/// buckets with no network id to scope by, so dropping either one is just
/// `cache.invalidate(&CacheKey::Account)` / `cache.invalidate(&CacheKey::Networks)` — a whole
/// enum variant would be a distinction without a difference. `Bucket` exists specifically for
/// the shape the mutating `Client` methods need: "drop every device entry for network N" or
/// "drop every profile entry for network N" in one call, without the caller enumerating which
/// device/profile ids happen to be cached for that network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    /// The `network[{nid}]` bucket.
    Network,
    /// The `eeros[{nid}_eeros]` bucket.
    Eeros,
    /// The `devices[...]` bucket, covering both the `{nid}_devices` list key and every
    /// `{nid}_{did}` single-device key for the given network id.
    Devices,
    /// The `profiles[...]` bucket, covering both the `{nid}_profiles` list key and every
    /// `{nid}_{pid}` single-profile key for the given network id.
    Profiles,
}

/// A cache key, reproducing one of the eight key shapes `eero-api` builds by hand as f-strings
/// (see the module-level table). Structured fields replace string concatenation so a caller
/// cannot accidentally collide two conceptually different keys (e.g. a network id that happens
/// to contain an underscore) the way `f"{nid}_{did}"` string keys could in Python.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKey {
    /// The flat `account` entry. `eero-api src/eero/client.py:238-255`.
    Account,
    /// The flat `networks` entry. `eero-api src/eero/client.py:260-311`.
    Networks,
    /// A single network, keyed by network id. `eero-api src/eero/client.py:334-356`.
    Network {
        /// The network id.
        network_id: String,
    },
    /// The eero list for a network, keyed `{nid}_eeros`. `eero-api src/eero/client.py:362-385`.
    Eeros {
        /// The network id.
        network_id: String,
    },
    /// The device list for a network, keyed `{nid}_devices`.
    /// `eero-api src/eero/client.py:437-460`.
    Devices {
        /// The network id.
        network_id: String,
    },
    /// A single device, keyed `{nid}_{did}`. `eero-api src/eero/client.py:463-491`.
    Device {
        /// The network id.
        network_id: String,
        /// The device id.
        device_id: String,
    },
    /// The profile list for a network, keyed `{nid}_profiles`.
    /// `eero-api src/eero/client.py:580-603`.
    Profiles {
        /// The network id.
        network_id: String,
    },
    /// A single profile, keyed `{nid}_{pid}`. `eero-api src/eero/client.py:606-634`.
    Profile {
        /// The network id.
        network_id: String,
        /// The profile id.
        profile_id: String,
    },
}

impl CacheKey {
    /// Builds a [`CacheKey::Network`].
    pub fn network(network_id: impl Into<String>) -> Self {
        Self::Network {
            network_id: network_id.into(),
        }
    }

    /// Builds a [`CacheKey::Eeros`].
    pub fn eeros(network_id: impl Into<String>) -> Self {
        Self::Eeros {
            network_id: network_id.into(),
        }
    }

    /// Builds a [`CacheKey::Devices`].
    pub fn devices(network_id: impl Into<String>) -> Self {
        Self::Devices {
            network_id: network_id.into(),
        }
    }

    /// Builds a [`CacheKey::Device`].
    pub fn device(network_id: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self::Device {
            network_id: network_id.into(),
            device_id: device_id.into(),
        }
    }

    /// Builds a [`CacheKey::Profiles`].
    pub fn profiles(network_id: impl Into<String>) -> Self {
        Self::Profiles {
            network_id: network_id.into(),
        }
    }

    /// Builds a [`CacheKey::Profile`].
    pub fn profile(network_id: impl Into<String>, profile_id: impl Into<String>) -> Self {
        Self::Profile {
            network_id: network_id.into(),
            profile_id: profile_id.into(),
        }
    }

    /// Returns this key's network id, or `None` for the two flat keys ([`CacheKey::Account`],
    /// [`CacheKey::Networks`]) that have no network scope at all.
    fn network_id(&self) -> Option<&str> {
        match self {
            Self::Account | Self::Networks => None,
            Self::Network { network_id }
            | Self::Eeros { network_id }
            | Self::Devices { network_id }
            | Self::Device { network_id, .. }
            | Self::Profiles { network_id }
            | Self::Profile { network_id, .. } => Some(network_id),
        }
    }

    /// Returns `true` if this key is a member of `bucket` scoped to `network_id` — i.e. what
    /// [`Cache::invalidate_bucket`] uses to decide which entries to drop. Private: this is an
    /// implementation detail of bucket invalidation, not a general-purpose classifier (unlike
    /// the old `bucket()` method this replaces, it can say "no" for the *same* bucket kind when
    /// the network id doesn't match, which is the whole point of the fix). Split into a
    /// kind check (`matches!`, one boolean, no per-arm bodies to accidentally collapse under
    /// `clippy::match_same_arms`) and a separate id comparison via [`CacheKey::network_id`].
    fn is_in_bucket(&self, bucket: Bucket, network_id: &str) -> bool {
        let same_kind = matches!(
            (bucket, self),
            (Bucket::Network, Self::Network { .. })
                | (Bucket::Eeros, Self::Eeros { .. })
                | (Bucket::Devices, Self::Devices { .. } | Self::Device { .. })
                | (
                    Bucket::Profiles,
                    Self::Profiles { .. } | Self::Profile { .. }
                )
        );
        same_kind && self.network_id() == Some(network_id)
    }
}

impl fmt::Display for CacheKey {
    /// Renders the key using the exact `bucket[key]` shape from the module-level table, e.g.
    /// `"devices[abc123_def456]"`. Used only by [`Cache`]'s `Debug` impl and in tests — never
    /// prints a cached value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Account => write!(f, "account"),
            Self::Networks => write!(f, "networks"),
            Self::Network { network_id } => write!(f, "network[{network_id}]"),
            Self::Eeros { network_id } => write!(f, "eeros[{network_id}_eeros]"),
            Self::Devices { network_id } => write!(f, "devices[{network_id}_devices]"),
            Self::Device {
                network_id,
                device_id,
            } => write!(f, "devices[{network_id}_{device_id}]"),
            Self::Profiles { network_id } => write!(f, "profiles[{network_id}_profiles]"),
            Self::Profile {
                network_id,
                profile_id,
            } => write!(f, "profiles[{network_id}_{profile_id}]"),
        }
    }
}

/// A stored value plus the instant it was written, so [`Cache::get`] can compare it against the
/// cache's TTL. Uses [`tokio::time::Instant`] rather than [`std::time::Instant`] specifically so
/// tests can drive expiry deterministically with `tokio::time::pause()` / `advance()` instead of
/// a real `sleep` (the crate's testing conventions forbid `sleep` in TTL tests). Outside of a paused
/// test clock this behaves identically to `std::time::Instant`.
struct Entry {
    value: Envelope,
    inserted_at: Instant,
}

/// An opaque snapshot of a [`Cache`]'s invalidation counter, taken by
/// `Cache::generation` and handed back to `Cache::put_if_current`.
///
/// Two snapshots are equal exactly when no invalidation ran between them. The counter wraps
/// instead of overflowing; only inequality is ever checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheGeneration(u64);

/// Everything a [`Cache`] guards with one lock: the entries and the invalidation counter, so a
/// guarded store can compare the counter and insert without an invalidation slipping in between.
struct Inner {
    entries: HashMap<CacheKey, Entry>,
    generation: u64,
}

impl Inner {
    /// Advances the invalidation counter. Called by every invalidator under the write lock,
    /// whether or not it removed anything.
    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

/// An in-memory, TTL-bounded cache of [`Envelope`] values, keyed by [`CacheKey`].
///
/// This is a hand-written map rather than a crate like `moka`, which would be overkill for eight
/// key shapes with no eviction policy beyond TTL (Python has none either — the map is bounded by
/// how many networks/devices/profiles exist).
///
/// # Locking
///
/// Guarded by [`std::sync::RwLock`], not `tokio::sync::RwLock`. Every critical section here is a
/// cheap, synchronous `HashMap` operation with no `.await` inside it, so there is nothing for an
/// async-aware lock to buy: `std::sync::RwLock` has no executor overhead and cannot be held
/// across a `.await` by construction (`RwLockReadGuard`/`RwLockWriteGuard` are not `Send`, and
/// every guard here is dropped before this function returns to its caller). A poisoned lock (a
/// prior holder panicked while holding it) is recovered rather than propagated — a panic
/// elsewhere in the process should not turn every future cache access into a panic too — via
/// [`PoisonError::into_inner`].
///
/// `Cache` is cheap to [`Clone`]: the map and the invalidation counter live behind an [`Arc`],
/// so cloning shares the same underlying storage (the way the `Client` facade hands copies of one
/// `Cache` to whatever needs it) rather than duplicating entries.
///
/// # Examples
///
/// ```
/// use rusteero::cache::{Cache, CacheKey};
/// use rusteero::envelope::Envelope;
/// use serde_json::json;
/// use std::time::Duration;
///
/// let cache = Cache::new(Duration::from_secs(60));
/// let key = CacheKey::network("abc123");
///
/// // Nothing has been cached yet.
/// assert!(cache.get(&key).is_none());
///
/// cache.put(
///     key.clone(),
///     Envelope::from_value(json!({"meta": {"code": 200}, "data": {"name": "Home"}})),
/// );
/// assert!(cache.get(&key).is_some());
///
/// cache.invalidate(&key);
/// assert!(cache.get(&key).is_none());
/// ```
pub struct Cache {
    ttl: Duration,
    inner: Arc<RwLock<Inner>>,
}

impl Cache {
    /// Creates an empty cache with the given TTL.
    ///
    /// `ttl = Duration::ZERO` disables reads entirely (see the module-level docs); it does not
    /// disable writes.
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            inner: Arc::new(RwLock::new(Inner {
                entries: HashMap::new(),
                generation: 0,
            })),
        }
    }

    /// Reads the map, recovering from a poisoned lock instead of panicking.
    fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the map, recovering from a poisoned lock instead of panicking.
    fn write(&self) -> RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `entry` has lived for at least this cache's TTL (`_is_cache_valid`'s strict `<`
    /// makes an entry exactly `ttl` old already expired, `client.py:188`).
    fn is_expired(&self, entry: &Entry) -> bool {
        entry.inserted_at.elapsed() >= self.ttl
    }

    /// Returns a cached value for `key`, or `None` on a miss.
    ///
    /// A "miss" covers three cases, matching `_is_cache_valid` plus the `if cached:` guard in
    /// `eero-api src/eero/client.py:162-196, 590-594` at `v8.4.1`: no entry exists for `key`; an
    /// entry exists but was written `>= ` the cache's TTL ago; or an entry exists, is fresh, but
    /// its value is "falsy" (an empty JSON object or empty JSON array — see the module-level
    /// docs). The third case does not remove the entry; it is simply not served.
    ///
    /// An entry found expired is removed (`client.py:188-196`), after re-checking under the
    /// write lock that a concurrent [`Cache::put`] has not refreshed it in the meantime. Pruning
    /// does not advance the invalidation counter: it removes nothing a reader could still be
    /// served.
    ///
    /// Always `None` when this cache's TTL is [`Duration::ZERO`], without taking the lock and
    /// without pruning: a zero-TTL cache never serves a read, and its entries are replaced by the
    /// next write to the same key.
    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<Envelope> {
        if self.ttl.is_zero() {
            return None;
        }

        {
            let inner = self.read();
            let entry = inner.entries.get(key)?;
            if !self.is_expired(entry) {
                if is_falsy(&entry.value) {
                    return None;
                }
                return Some(entry.value.clone());
            }
        }

        let mut inner = self.write();
        if inner
            .entries
            .get(key)
            .is_some_and(|entry| self.is_expired(entry))
        {
            inner.entries.remove(key);
        }
        None
    }

    /// Stores `value` under `key`, stamped with the current time.
    ///
    /// Unconditional: unlike [`Cache::get`], this is never skipped by a zero TTL — it mirrors
    /// `_update_cache`'s unconditional write (`eero-api src/eero/client.py:198-229` at `v8.4.1`
    /// with no `epoch=`), which every cached getter calls after a live fetch regardless of
    /// `refresh_cache` or `cache_timeout`. A caller implementing `refresh_cache` semantics simply
    /// skips calling [`Cache::get`] for that one call while still calling `put` — `Cache` has no
    /// `refresh_cache` parameter of its own. It does not advance the invalidation counter.
    ///
    /// The `Client` getters use `Cache::put_if_current` instead, so a response fetched before an
    /// invalidation cannot be stored after it.
    pub fn put(&self, key: CacheKey, value: Envelope) {
        let mut inner = self.write();
        inner.entries.insert(
            key,
            Entry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Returns a snapshot of the invalidation counter, to be taken after a cache check and
    /// before the request whose response will be stored with `Cache::put_if_current`.
    ///
    /// Mirrors `cache_epoch = self._cache_epoch` (`client.py:595` at `v8.4.1`).
    #[must_use]
    pub(crate) fn generation(&self) -> CacheGeneration {
        CacheGeneration(self.read().generation)
    }

    /// Stores `value` under `key` only if no invalidation has run since `generation` was taken,
    /// and returns whether it stored.
    ///
    /// The comparison and the insert happen under one write lock. A rejected store is silent:
    /// the caller still hands its response to its own caller, and later readers miss and fetch
    /// fresh data (`_update_cache(.., epoch=)`, `client.py:219-220` at `v8.4.1`).
    pub(crate) fn put_if_current(
        &self,
        key: CacheKey,
        value: Envelope,
        generation: CacheGeneration,
    ) -> bool {
        let mut inner = self.write();
        if inner.generation != generation.0 {
            return false;
        }
        inner.entries.insert(
            key,
            Entry {
                value,
                inserted_at: Instant::now(),
            },
        );
        true
    }

    /// Removes a single key, if present, and advances the invalidation counter either way.
    /// Mirrors the many `del self._cache[bucket][key]` / `_invalidate_device_cache` /
    /// `_invalidate_profile_cache` sites in `client.py` (see the module-level docs for the full
    /// list and the two divergences).
    pub fn invalidate(&self, key: &CacheKey) {
        let mut inner = self.write();
        inner.bump_generation();
        inner.entries.remove(key);
    }

    /// Removes every key belonging to `bucket` for `network_id`, leaving every other network's
    /// entries in that same bucket untouched, and advances the invalidation counter either way.
    ///
    /// Python's `_invalidate_all_profile_caches` is the one call site that clears a whole
    /// bucket this way; elsewhere `client.py` deletes one key at a time (e.g.
    /// `_invalidate_device_cache` removes exactly the named devices' entries and
    /// `devices[{nid}_devices]`). Writers that need "drop every device entry for network N" as a
    /// single call, without tracking which specific device/profile ids happen to be cached, use
    /// this primitive alongside [`Cache::invalidate`] and [`Cache::clear`]. It is a superset of
    /// what any individual Python writer does (it also drops device/profile ids Python's own
    /// inline deletes never touch), which is strictly safer — a dropped entry is just an extra
    /// network round trip, never stale data served past its TTL.
    pub fn invalidate_bucket(&self, bucket: Bucket, network_id: &str) {
        let mut inner = self.write();
        inner.bump_generation();
        inner
            .entries
            .retain(|key, _| !key.is_in_bucket(bucket, network_id));
    }

    /// Removes every cached entry and advances the invalidation counter.
    ///
    /// Mirrors `clear_cache()` (`eero-api src/eero/client.py:252-259` at `v8.4.1`), called on
    /// `verify`, `logout`, and every credential-clearing path.
    ///
    /// `Divergence from eero-api:` Python's `clear_cache()` sets each entry's `data` back to
    /// `None` but leaves its `timestamp` untouched for the flat `account`/`networks` keys, so
    /// `_is_cache_valid` can keep reporting those keys as "fresh" after a clear — harmless only
    /// because `_get_from_cache` then returns `None`, which the `if cached:` guard treats as a
    /// miss anyway. This method removes the whole entry — value *and* timestamp together — so
    /// there is no stale timestamp left to reason about, for any key shape, not just the flat
    /// ones.
    pub fn clear(&self) {
        let mut inner = self.write();
        inner.bump_generation();
        inner.entries.clear();
    }

    /// Returns the number of entries currently stored, expired or not.
    ///
    /// Exists mainly for tests and for a `Cache`'s [`fmt::Debug`] output; TTL expiry is
    /// lazy (an entry is removed when [`Cache::get`] next finds it expired), so this can include
    /// entries [`Cache::get`] would no longer serve.
    #[must_use]
    pub fn len(&self) -> usize {
        self.read().entries.len()
    }

    /// Returns `true` if no entries are stored, expired or not.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Clone for Cache {
    /// Cheap: clones the [`Arc`] handle, so the clone shares the same underlying storage rather
    /// than duplicating entries.
    fn clone(&self) -> Self {
        Self {
            ttl: self.ttl,
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for Cache {
    /// Builds a cache with [`crate::consts::DEFAULT_CACHE_TTL`] (60 seconds), matching
    /// `eero-api`'s `cache_timeout: int = 60` default (`client.py:41`).
    fn default() -> Self {
        Self::new(crate::consts::DEFAULT_CACHE_TTL)
    }
}

/// A hand-written `Debug` impl that prints the TTL, the entry count and the *keys* currently
/// stored — never a cached value. A cached `network[nid]` entry can carry the network's Wi-Fi
/// password (`get_network`'s raw response includes it); an auto-derived `Debug` that dumped
/// `Entry.value` would turn an incidental `{:?}` into a credential leak, the same reasoning
/// `Envelope`'s own hand-written `Debug` documents.
impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.read();
        let mut keys: Vec<String> = inner.entries.keys().map(ToString::to_string).collect();
        keys.sort_unstable();
        f.debug_struct("Cache")
            .field("ttl", &self.ttl)
            .field("len", &inner.entries.len())
            .field("keys", &keys)
            .finish_non_exhaustive()
    }
}

/// Returns `true` if `value`'s wire payload is "falsy" in the Python sense this cache
/// reproduces. See the module-level "falsy-value rule" docs — **this is a deliberate port of a
/// real Python quirk, not a bug to clean up.**
fn is_falsy(value: &Envelope) -> bool {
    match value.as_value() {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => is_zero(n),
        Value::String(s) => s.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
    }
}

/// Returns `true` if `n` is the JSON number zero, in whichever of the three representations
/// `serde_json::Number` can hold it (`i64`, `u64`, or `f64`). Avoids `clippy::float_cmp` by
/// checking the exact integer representations first and only falling back to an epsilon
/// comparison for a genuinely fractional encoding of zero (`0.0`), which every real Eero
/// response that means "zero" avoids by sending a bare integer.
fn is_zero(n: &serde_json::Number) -> bool {
    if let Some(i) = n.as_i64() {
        return i == 0;
    }
    if let Some(u) = n.as_u64() {
        return u == 0;
    }
    n.as_f64().is_some_and(|f| f.abs() < f64::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::{Bucket, Cache, CacheKey};
    use crate::envelope::Envelope;
    use serde_json::json;
    use std::time::Duration;

    fn envelope(body: serde_json::Value) -> Envelope {
        Envelope::from_value(body)
    }

    fn non_empty() -> Envelope {
        envelope(json!({"meta": {"code": 200}, "data": {"name": "Home"}}))
    }

    // ===================== basic hit / miss =====================

    #[test]
    fn new_cache_starts_empty() {
        let cache = Cache::new(Duration::from_secs(60));
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn miss_on_a_key_that_was_never_written() {
        let cache = Cache::new(Duration::from_secs(60));
        assert!(cache.get(&CacheKey::Account).is_none());
    }

    #[tokio::test]
    async fn hit_before_expiry() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        assert_eq!(cache.get(&CacheKey::Account), Some(non_empty()));
    }

    #[tokio::test]
    async fn miss_after_expiry() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());

        tokio::time::advance(Duration::from_secs(61)).await;

        assert!(cache.get(&CacheKey::Account).is_none());
    }

    #[tokio::test]
    async fn miss_exactly_at_the_ttl_boundary() {
        // eero-api's validity check is strict `<`, i.e. `elapsed < ttl` is valid; an entry that
        // is exactly `ttl` old is already invalid (`client.py:92-93`).
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());

        tokio::time::advance(Duration::from_secs(60)).await;

        assert!(cache.get(&CacheKey::Account).is_none());
    }

    #[tokio::test]
    async fn hit_just_before_the_ttl_boundary() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());

        tokio::time::advance(Duration::from_millis(59_999)).await;

        assert!(cache.get(&CacheKey::Account).is_some());
    }

    // ===================== falsy-value rule =====================

    #[test]
    fn each_falsy_shape_is_treated_as_a_miss() {
        // The five falsy shapes (`null`, `{}`, `[]`, `""`, `0`), plus JSON `false` for full
        // parity with Python's `if cached:` truthiness (see the module-level "falsy-value rule"
        // docs). Every one of these must be a miss even though the entry is freshly written and
        // well within the TTL.
        let falsy_shapes = [
            ("null", json!(null)),
            ("empty object", json!({})),
            ("empty array", json!([])),
            ("zero", json!(0)),
            ("empty string", json!("")),
            ("false", json!(false)),
        ];

        for (label, shape) in falsy_shapes {
            let cache = Cache::new(Duration::from_secs(60));
            cache.put(CacheKey::Account, envelope(shape.clone()));
            assert!(
                cache.get(&CacheKey::Account).is_none(),
                "{label} ({shape:?}) must not be served from cache"
            );
        }
    }

    #[test]
    fn non_falsy_shapes_of_the_same_json_types_are_hits() {
        // The counterexample for each shape above, so `each_falsy_shape_is_treated_as_a_miss`
        // is proven to discriminate on emptiness/zero-ness, not on JSON type.
        let truthy_shapes = [
            json!({"data": {"name": "Home"}}),
            json!([{"url": "/2.2/networks/1"}]),
            json!(1),
            json!("non-empty"),
            json!(true),
        ];

        for shape in truthy_shapes {
            let cache = Cache::new(Duration::from_secs(60));
            cache.put(CacheKey::Account, envelope(shape.clone()));
            assert!(
                cache.get(&CacheKey::Account).is_some(),
                "{shape:?} must be served from cache"
            );
        }
    }

    // ===================== refresh_cache-style bypass =====================

    #[test]
    fn put_overwrites_an_existing_valid_entry() {
        // Mirrors `refresh_cache=True`: the caller skips `get()` for one call but still calls
        // `put()`, which must succeed and replace the value even though the old entry was still
        // within its TTL.
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(
            CacheKey::Account,
            envelope(json!({"data": {"name": "old"}})),
        );
        cache.put(
            CacheKey::Account,
            envelope(json!({"data": {"name": "new"}})),
        );

        let cached = cache.get(&CacheKey::Account).expect("hit");
        assert_eq!(cached.data()["name"], json!("new"));
    }

    // ===================== cache_timeout = 0 =====================

    #[test]
    fn zero_ttl_disables_reads_but_not_writes() {
        let cache = Cache::new(Duration::ZERO);
        cache.put(CacheKey::Account, non_empty());

        assert!(cache.get(&CacheKey::Account).is_none());
        assert_eq!(cache.len(), 1, "the write must still have happened");
    }

    // ===================== key/bucket shapes =====================

    #[test]
    fn every_key_shape_round_trips_independently() {
        let cache = Cache::new(Duration::from_secs(60));
        let keys = [
            CacheKey::Account,
            CacheKey::Networks,
            CacheKey::network("nid"),
            CacheKey::eeros("nid"),
            CacheKey::devices("nid"),
            CacheKey::device("nid", "did"),
            CacheKey::profiles("nid"),
            CacheKey::profile("nid", "pid"),
        ];

        for (i, key) in keys.iter().enumerate() {
            cache.put(key.clone(), envelope(json!({"data": {"i": i}})));
        }

        for (i, key) in keys.iter().enumerate() {
            let cached = cache.get(key).unwrap_or_else(|| panic!("miss for {key}"));
            assert_eq!(cached.data()["i"], json!(i), "wrong value for {key}");
        }
        assert_eq!(cache.len(), keys.len());
    }

    #[test]
    fn device_list_and_single_device_keys_do_not_collide() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(
            CacheKey::devices("nid"),
            envelope(json!({"data": {"which": "list"}})),
        );
        cache.put(
            CacheKey::device("nid", "devices"),
            envelope(json!({"data": {"which": "single"}})),
        );

        let list = cache.get(&CacheKey::devices("nid")).expect("list hit");
        let single = cache
            .get(&CacheKey::device("nid", "devices"))
            .expect("single hit");
        assert_eq!(list.data()["which"], json!("list"));
        assert_eq!(single.data()["which"], json!("single"));
    }

    #[test]
    fn key_display_matches_python_key_shapes() {
        assert_eq!(CacheKey::Account.to_string(), "account");
        assert_eq!(CacheKey::Networks.to_string(), "networks");
        assert_eq!(CacheKey::network("nid").to_string(), "network[nid]");
        assert_eq!(CacheKey::eeros("nid").to_string(), "eeros[nid_eeros]");
        assert_eq!(CacheKey::devices("nid").to_string(), "devices[nid_devices]");
        assert_eq!(
            CacheKey::device("nid", "did").to_string(),
            "devices[nid_did]"
        );
        assert_eq!(
            CacheKey::profiles("nid").to_string(),
            "profiles[nid_profiles]"
        );
        assert_eq!(
            CacheKey::profile("nid", "pid").to_string(),
            "profiles[nid_pid]"
        );
    }

    #[test]
    fn devices_for_different_networks_never_collide() {
        // Two networks' `devices` entries — both the `{nid}_devices` list key and a
        // `{nid}_{did}` single-device key sharing the same device id across networks — must
        // never overwrite one another.
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(
            CacheKey::devices("net-1"),
            envelope(json!({"data": {"network": "net-1", "kind": "list"}})),
        );
        cache.put(
            CacheKey::devices("net-2"),
            envelope(json!({"data": {"network": "net-2", "kind": "list"}})),
        );
        cache.put(
            CacheKey::device("net-1", "shared-device-id"),
            envelope(json!({"data": {"network": "net-1", "kind": "single"}})),
        );
        cache.put(
            CacheKey::device("net-2", "shared-device-id"),
            envelope(json!({"data": {"network": "net-2", "kind": "single"}})),
        );

        assert_eq!(
            cache.get(&CacheKey::devices("net-1")).unwrap().data()["network"],
            json!("net-1")
        );
        assert_eq!(
            cache.get(&CacheKey::devices("net-2")).unwrap().data()["network"],
            json!("net-2")
        );
        assert_eq!(
            cache
                .get(&CacheKey::device("net-1", "shared-device-id"))
                .unwrap()
                .data()["network"],
            json!("net-1")
        );
        assert_eq!(
            cache
                .get(&CacheKey::device("net-2", "shared-device-id"))
                .unwrap()
                .data()["network"],
            json!("net-2")
        );
        assert_eq!(
            cache.len(),
            4,
            "all four entries must coexist independently"
        );
    }

    // ===================== invalidation =====================

    #[test]
    fn clear_removes_values_and_timestamps() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        cache.put(CacheKey::network("nid"), non_empty());

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.get(&CacheKey::Account).is_none());
        assert!(cache.get(&CacheKey::network("nid")).is_none());
    }

    #[test]
    fn invalidate_drops_exactly_one_key() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::device("nid", "did"), non_empty());
        cache.put(CacheKey::devices("nid"), non_empty());

        cache.invalidate(&CacheKey::device("nid", "did"));

        assert!(cache.get(&CacheKey::device("nid", "did")).is_none());
        assert!(
            cache.get(&CacheKey::devices("nid")).is_some(),
            "the sibling list key must survive"
        );
    }

    #[test]
    fn invalidate_bucket_drops_only_that_bucket_kind() {
        // Invalidating the `Eeros` bucket for a network must not touch a `Network`-bucket entry
        // for that same network, nor the flat `Account` entry.
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::network("nid"), non_empty());
        cache.put(CacheKey::eeros("nid"), non_empty());
        cache.put(CacheKey::Account, non_empty());

        cache.invalidate_bucket(Bucket::Eeros, "nid");

        assert!(cache.get(&CacheKey::eeros("nid")).is_none());
        assert!(cache.get(&CacheKey::network("nid")).is_some());
        assert!(cache.get(&CacheKey::Account).is_some());
    }

    #[test]
    fn invalidate_bucket_drops_only_the_targeted_networks_entries() {
        // The required behaviour the mutating `Client` methods depend on: "drop every device
        // entry for network N" must leave network M's device entries alone, even though both
        // live in the same `Devices` bucket and even share a device id.
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::devices("net-1"), non_empty());
        cache.put(CacheKey::device("net-1", "shared-id"), non_empty());
        cache.put(CacheKey::devices("net-2"), non_empty());
        cache.put(CacheKey::device("net-2", "shared-id"), non_empty());

        cache.invalidate_bucket(Bucket::Devices, "net-1");

        assert!(cache.get(&CacheKey::devices("net-1")).is_none());
        assert!(cache.get(&CacheKey::device("net-1", "shared-id")).is_none());
        assert!(
            cache.get(&CacheKey::devices("net-2")).is_some(),
            "network-2's device list must survive"
        );
        assert!(
            cache.get(&CacheKey::device("net-2", "shared-id")).is_some(),
            "network-2's device entry must survive, even sharing a device id with network-1"
        );
    }

    #[test]
    fn invalidate_bucket_drops_every_profile_entry_for_one_network() {
        // Same guarantee as the devices case above, for the `Profiles` bucket.
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::profiles("net-1"), non_empty());
        cache.put(CacheKey::profile("net-1", "p1"), non_empty());
        cache.put(CacheKey::profile("net-1", "p2"), non_empty());
        cache.put(CacheKey::profiles("net-2"), non_empty());
        cache.put(CacheKey::profile("net-2", "p1"), non_empty());

        cache.invalidate_bucket(Bucket::Profiles, "net-1");

        assert!(cache.get(&CacheKey::profiles("net-1")).is_none());
        assert!(cache.get(&CacheKey::profile("net-1", "p1")).is_none());
        assert!(cache.get(&CacheKey::profile("net-1", "p2")).is_none());
        assert!(cache.get(&CacheKey::profiles("net-2")).is_some());
        assert!(cache.get(&CacheKey::profile("net-2", "p1")).is_some());
    }

    // ===================== invalidation generation =====================

    #[test]
    fn generation_advances_on_invalidate_even_when_the_key_is_absent() {
        let cache = Cache::new(Duration::from_secs(60));
        let before = cache.generation();

        cache.invalidate(&CacheKey::device("n1", "never-cached"));

        assert_ne!(cache.generation(), before);
    }

    #[test]
    fn generation_advances_on_invalidate_bucket_and_clear_of_an_empty_cache() {
        let cache = Cache::new(Duration::from_secs(60));
        let start = cache.generation();

        cache.invalidate_bucket(Bucket::Devices, "n1");
        let after_bucket = cache.generation();
        assert_ne!(after_bucket, start);

        cache.clear();
        assert_ne!(cache.generation(), after_bucket);
    }

    #[test]
    fn generation_advances_when_an_invalidation_removes_an_entry() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        let before = cache.generation();

        cache.invalidate(&CacheKey::Account);

        assert_ne!(cache.generation(), before);
    }

    #[test]
    fn put_does_not_advance_generation() {
        let cache = Cache::new(Duration::from_secs(60));
        let before = cache.generation();

        cache.put(CacheKey::Account, non_empty());
        cache.put(CacheKey::Account, non_empty());

        assert_eq!(cache.generation(), before);
    }

    #[test]
    fn generation_is_shared_between_clones() {
        let cache = Cache::new(Duration::from_secs(60));
        let clone = cache.clone();
        let before = cache.generation();

        clone.invalidate(&CacheKey::Networks);

        assert_ne!(cache.generation(), before);
        assert_eq!(cache.generation(), clone.generation());
    }

    #[test]
    fn an_unrelated_invalidation_still_advances_the_generation() {
        // The counter is cache-wide on purpose: dropping network-1's devices also blocks the
        // fill of a profile read for network-2 that is in flight.
        let cache = Cache::new(Duration::from_secs(60));
        let captured_for_net_2_profile = cache.generation();

        cache.invalidate_bucket(Bucket::Devices, "net-1");

        assert!(!cache.put_if_current(
            CacheKey::profile("net-2", "p1"),
            non_empty(),
            captured_for_net_2_profile
        ));
        assert!(cache.get(&CacheKey::profile("net-2", "p1")).is_none());
    }

    #[test]
    fn put_if_current_stores_when_nothing_was_invalidated() {
        let cache = Cache::new(Duration::from_secs(60));
        let generation = cache.generation();

        assert!(cache.put_if_current(CacheKey::Account, non_empty(), generation));

        assert_eq!(cache.get(&CacheKey::Account), Some(non_empty()));
    }

    #[test]
    fn put_if_current_stores_after_unguarded_puts() {
        // A concurrent plain `put` is not an invalidation: the generation is unchanged.
        let cache = Cache::new(Duration::from_secs(60));
        let generation = cache.generation();
        cache.put(CacheKey::Networks, non_empty());

        assert!(cache.put_if_current(CacheKey::Account, non_empty(), generation));
    }

    #[test]
    fn put_if_current_refuses_after_every_kind_of_invalidation() {
        type Invalidator = fn(&Cache);
        let invalidators: [(&str, Invalidator); 3] = [
            ("invalidate", |cache| {
                cache.invalidate(&CacheKey::device("n1", "d1"));
            }),
            ("invalidate_bucket", |cache| {
                cache.invalidate_bucket(Bucket::Devices, "n1");
            }),
            ("clear", Cache::clear),
        ];

        for (label, invalidate) in invalidators {
            for previously_present in [false, true] {
                let cache = Cache::new(Duration::from_secs(60));
                let key = CacheKey::device("n1", "d1");
                if previously_present {
                    cache.put(key.clone(), envelope(json!({"data": {"which": "old"}})));
                }
                let generation = cache.generation();

                invalidate(&cache);

                assert!(
                    !cache.put_if_current(
                        key.clone(),
                        envelope(json!({"data": {"which": "stale"}})),
                        generation
                    ),
                    "{label} (previously present: {previously_present}) must reject the store"
                );
                assert!(
                    cache.get(&key).is_none(),
                    "{label} (previously present: {previously_present}) left an entry behind"
                );
            }
        }
    }

    #[test]
    fn a_fresh_generation_taken_after_an_invalidation_is_accepted() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.clear();
        let generation = cache.generation();

        assert!(cache.put_if_current(CacheKey::Account, non_empty(), generation));
    }

    // ===================== expiry pruning =====================

    #[tokio::test]
    async fn an_expired_entry_is_removed_when_it_is_read() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::device("n1", "d1"), non_empty());
        cache.put(CacheKey::device("n1", "d2"), non_empty());

        tokio::time::advance(Duration::from_secs(61)).await;
        // d2 is written again, so only d1 is expired.
        cache.put(CacheKey::device("n1", "d2"), non_empty());

        assert!(cache.get(&CacheKey::device("n1", "d1")).is_none());
        assert_eq!(cache.len(), 1, "only the refreshed d2 entry is left");
        assert!(cache.get(&CacheKey::device("n1", "d2")).is_some());
    }

    #[tokio::test]
    async fn an_expired_single_entry_key_is_removed_when_it_is_read() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        cache.put(CacheKey::Networks, non_empty());

        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(cache.get(&CacheKey::Account).is_none());

        assert_eq!(
            cache.len(),
            1,
            "the account entry is gone, networks is not read"
        );
        assert!(cache.get(&CacheKey::Networks).is_none());
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn fresh_entries_are_left_alone_by_a_read() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        cache.put(CacheKey::device("n1", "d1"), non_empty());

        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(cache.get(&CacheKey::Account).is_some());
        assert!(cache.get(&CacheKey::device("n1", "d1")).is_some());
        assert!(cache.get(&CacheKey::device("n1", "absent")).is_none());

        assert_eq!(cache.len(), 2);
    }

    #[tokio::test]
    async fn reading_a_pruned_key_again_is_a_plain_miss() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::device("n1", "d1"), non_empty());
        tokio::time::advance(Duration::from_secs(61)).await;

        assert!(cache.get(&CacheKey::device("n1", "d1")).is_none());
        assert!(cache.get(&CacheKey::device("n1", "d1")).is_none());
        assert!(cache.is_empty());
    }

    #[tokio::test]
    async fn an_expired_entry_is_replaced_by_the_next_write() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(
            CacheKey::Account,
            envelope(json!({"data": {"name": "old"}})),
        );
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(cache.get(&CacheKey::Account).is_none());

        cache.put(
            CacheKey::Account,
            envelope(json!({"data": {"name": "new"}})),
        );

        let cached = cache
            .get(&CacheKey::Account)
            .expect("fresh entry is served");
        assert_eq!(cached.data()["name"], json!("new"));
    }

    #[test]
    fn a_fresh_falsy_entry_is_not_pruned() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, envelope(json!({})));

        assert!(cache.get(&CacheKey::Account).is_none());

        assert_eq!(
            cache.len(),
            1,
            "a falsy entry is not served but is not expired either"
        );
    }

    #[tokio::test]
    async fn pruning_does_not_advance_the_generation() {
        tokio::time::pause();
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(CacheKey::Account, non_empty());
        let generation = cache.generation();
        tokio::time::advance(Duration::from_secs(61)).await;

        assert!(cache.get(&CacheKey::Account).is_none());

        assert_eq!(cache.generation(), generation);
        assert!(cache.put_if_current(CacheKey::Account, non_empty(), generation));
    }

    // ===================== Debug redaction =====================

    #[test]
    fn debug_does_not_leak_a_cached_password_but_does_show_the_key() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.put(
            CacheKey::network("nid"),
            envelope(json!({
                "data": {"name": "Home", "guest_network": {"password": "super-secret-wifi-pw"}}
            })),
        );

        let rendered = format!("{cache:?}");
        assert!(!rendered.contains("super-secret-wifi-pw"));
        assert!(rendered.contains("network[nid]"));
        assert!(
            rendered.contains('1'),
            "entry count should be visible: {rendered}"
        );
    }
}
