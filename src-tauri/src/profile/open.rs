//! The profiles the sessions play. Each one is in memory once, however
//! many sessions play it, so an edit from any of them reaches every
//! session on it. The session map keeps them, see [`crate::sessions`],
//! and each [`Session`](crate::sessions::Session) points at the one it
//! plays. A session launch restored points at one that waits, see
//! [`OpenProfile::waiting`], until its first selection.

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::profile::live::Profile;
use crate::sessions::Session;

/// One profile the sessions play, with what holds its saves back, or the
/// defaults a restored session waits on, see [`OpenProfile::waiting`].
pub(crate) struct OpenProfile {
    /// Its place in the order the profiles opened. A step that holds two
    /// profiles at once takes the lower first.
    id: u64,
    /// Its name in the profile set. None only for the defaults the app
    /// starts on, before launch loads a profile or when profiles.toml
    /// does not read. A leaf lock, held for a copy.
    name: std::sync::Mutex<Option<String>>,
    profile: Arc<Mutex<Profile>>,
    /// Counts the marks that ask for a save, so the save a burst of them
    /// started writes once, after the last.
    dirty_gen: AtomicU64,
    /// Set by `#profile reset` and `#profile load`, which leave the
    /// profile apart from its file on purpose, so the passive saves, the
    /// debounce and quit, leave the file alone. The next durable change
    /// clears it.
    persist_held: AtomicBool,
}

impl OpenProfile {
    /// Open `profile` as `name`, the name it keeps too.
    pub(crate) fn new(id: u64, name: Option<String>, mut profile: Profile) -> Self {
        profile.name.clone_from(&name);
        Self {
            id,
            name: std::sync::Mutex::new(name),
            profile: Arc::new(Mutex::new(profile)),
            dirty_gen: AtomicU64::new(0),
            persist_held: AtomicBool::new(false),
        }
    }

    /// The defaults a session launch restored plays under `name`, the
    /// profile it last played, until its first selection opens that
    /// profile or joins it, see [`crate::app::launch::open_restored`]. The
    /// session map keeps it out of the open profiles, so no save writes
    /// it.
    pub(crate) fn waiting(id: u64, name: &str) -> Self {
        Self::new(id, Some(name.to_string()), Profile::default())
    }

    /// Its name in the profile set, see [`OpenProfile::name`].
    pub(crate) fn name(&self) -> Option<String> {
        self.name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Lock the profile.
    pub(crate) async fn lock(self: &Arc<Self>) -> ProfileGuard {
        ProfileGuard::lock(self.clone()).await
    }

    /// The profile, locked, unless another step holds it. A frame of the
    /// native surface reads the font this way, so it never waits.
    #[cfg(native_surface)]
    pub(crate) fn try_lock(&self) -> Option<tokio::sync::MutexGuard<'_, Profile>> {
        self.profile.try_lock().ok()
    }

    /// Count one more mark, and return the count after it.
    pub(crate) fn mark(&self) -> u64 {
        self.dirty_gen.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// The count of marks so far.
    pub(crate) fn marks(&self) -> u64 {
        self.dirty_gen.load(Ordering::Acquire)
    }

    /// Whether `#profile reset` or `#profile load` holds the passive
    /// saves back.
    pub(crate) fn held(&self) -> bool {
        self.persist_held.load(Ordering::Acquire)
    }

    /// Hold the passive saves back, or let them go again.
    pub(crate) fn hold(&self, held: bool) {
        self.persist_held.store(held, Ordering::Release);
    }
}

/// Its place and its name, for a result that names the profile it ran
/// under.
impl std::fmt::Debug for OpenProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenProfile")
            .field("id", &self.id)
            .field("name", &self.name())
            .finish_non_exhaustive()
    }
}

/// An open profile, locked. It reads and writes as the [`Profile`] and
/// keeps the [`OpenProfile`] it came from at hand.
pub(crate) struct ProfileGuard {
    guard: OwnedMutexGuard<Profile>,
    open: Arc<OpenProfile>,
}

impl ProfileGuard {
    /// Lock `open`, keeping the handle.
    pub(crate) async fn lock(open: Arc<OpenProfile>) -> Self {
        Self {
            guard: open.profile.clone().lock_owned().await,
            open,
        }
    }

    /// The open profile this guard holds.
    pub(crate) fn open(&self) -> &Arc<OpenProfile> {
        &self.open
    }

    /// The sessions among `sessions` that play this profile. A switch
    /// moves a session only while it holds the profile it leaves and the
    /// next, so the ones picked stay on this one while the guard is held.
    pub(crate) fn players<'a>(
        &'a self,
        sessions: &'a [Arc<Session>],
    ) -> impl Iterator<Item = &'a Arc<Session>> + 'a {
        sessions
            .iter()
            .filter(|session| Arc::ptr_eq(&session.profile(), &self.open))
    }

    /// Give the profile the name `name` in the profile set, in memory and
    /// on its [`OpenProfile`] alike.
    pub(crate) fn set_name(&mut self, name: &str) {
        self.guard.name = Some(name.to_string());
        *self
            .open
            .name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(name.to_string());
    }
}

impl Deref for ProfileGuard {
    type Target = Profile;

    fn deref(&self) -> &Profile {
        &self.guard
    }
}

impl DerefMut for ProfileGuard {
    fn deref_mut(&mut self) -> &mut Profile {
        &mut self.guard
    }
}

/// Lock `a` and `b`, two profiles, the one that opened first first.
pub(crate) async fn lock_both(
    a: &Arc<OpenProfile>,
    b: &Arc<OpenProfile>,
) -> (ProfileGuard, ProfileGuard) {
    if a.id <= b.id {
        let first = a.lock().await;
        (first, b.lock().await)
    } else {
        let second = b.lock().await;
        (a.lock().await, second)
    }
}
