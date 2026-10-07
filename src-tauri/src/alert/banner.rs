//! Where a banner goes. [`Banners::System`] posts it, bounces the Dock or
//! flashes the taskbar, and plays a system sound where the window cannot.
//! A test build holds [`Banners::Recorded`] instead, which keeps each
//! banner for the test to read, so no test ever posts one or touches the
//! system's notification center.

#[cfg(test)]
use std::sync::Arc;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, UserAttentionType};

use super::focus::Fate;
use super::Attention;
use crate::app::events::{broadcast, SESSION_SELECTED};
use crate::app::state::SharedState;
use crate::sessions::SessionId;

/// One banner to post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Banner {
    pub(crate) session: SessionId,
    pub(crate) title: String,
    /// The session as its row reads, under the title.
    pub(crate) label: Option<String>,
    /// The words, with Title and words on.
    pub(crate) words: Option<String>,
    /// The Lua owner that raised it.
    pub(crate) owner: Option<String>,
}

impl Banner {
    /// The banner's body: the words, or else Vosh, so a title alone
    /// never looks cut short.
    pub(crate) fn body(&self) -> &str {
        self.words.as_deref().unwrap_or("Vosh")
    }
}

/// Whether you allow Vosh's banners, as the system says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Permission {
    Granted,
    /// Linux always allows banners, so only macOS and Windows deny.
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Denied,
    /// Vosh has not asked yet, so the next ask shows the system's
    /// question. Only macOS asks.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    NotAsked,
    /// This build cannot post banners, such as a dev build with no
    /// bundle on macOS or a Windows Vosh that is not installed.
    #[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
    Unavailable,
}

/// `vosh://session-selected`: Vosh selected a session itself, as a click
/// on a banner does, so every window follows. Linux banners take no
/// click, so only macOS and Windows send it.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SessionSelected {
    pub(crate) session: SessionId,
}

/// A banner a test build kept, with what the focus rule said of it.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Posted {
    pub(crate) banner: Banner,
    pub(crate) fate: Fate,
}

/// Where banners go.
pub(crate) enum Banners {
    /// The system's banners, sound and attention calls.
    #[cfg_attr(test, allow(dead_code))]
    System(SystemBanners),
    /// A list a test reads, in a test build.
    #[cfg(test)]
    Recorded(Arc<Mutex<Vec<Posted>>>),
}

impl Default for Banners {
    /// The system's in the app, a list in a test build.
    fn default() -> Self {
        #[cfg(test)]
        {
            Self::Recorded(Arc::default())
        }
        #[cfg(not(test))]
        {
            Self::System(SystemBanners::default())
        }
    }
}

/// What the system banners remember: the session of the newest banner
/// that went out since you last came to Vosh, which a click that starts
/// Vosh again on Windows selects.
#[derive(Debug, Default)]
pub(crate) struct SystemBanners {
    newest: Mutex<Option<SessionId>>,
}

impl SystemBanners {
    fn newest(&self) -> std::sync::MutexGuard<'_, Option<SessionId>> {
        self.newest
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Banners {
    /// Post `banner` as `fate` says, and bounce the Dock or flash the
    /// taskbar, then hand `told` whether Vosh played a system sound in
    /// place of the page's tone, as for a window too hidden to play it.
    /// Only the main thread answers whether the window shows, so an alert
    /// with a tone posts from a blocking task, and the session that rang
    /// it never waits on the main thread.
    pub(crate) fn post<R: tauri::Runtime>(
        &self,
        app: &AppHandle<R>,
        banner: Banner,
        fate: Fate,
        told: impl FnOnce(bool) + Send + 'static,
    ) {
        match self {
            Banners::System(system) if fate.sound.is_none() => {
                told(system.post(app, &banner, &fate, false));
            }
            Banners::System(_) => {
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let hidden = main_hidden(&app);
                    let played = app.try_state::<SharedState>().is_some_and(|state| {
                        matches!(&state.banners, Banners::System(system)
                            if system.post(&app, &banner, &fate, hidden))
                    });
                    told(played);
                });
            }
            #[cfg(test)]
            Banners::Recorded(list) => {
                if fate.banner || fate.attention.is_some() {
                    list.lock()
                        .expect("the banners")
                        .push(Posted { banner, fate });
                }
                told(false);
            }
        }
    }

    /// Take back the banners of `session` that the Lua `owner` posted.
    pub(crate) fn withdraw(&self, session: SessionId, owner: &str) {
        match self {
            Banners::System(_) => {
                #[cfg(target_os = "macos")]
                super::mac::withdraw(session, owner);
                #[cfg(not(target_os = "macos"))]
                let _ = (session, owner);
            }
            #[cfg(test)]
            Banners::Recorded(list) => list.lock().expect("the banners").retain(|p| {
                !(p.banner.session == session && p.banner.owner.as_deref() == Some(owner))
            }),
        }
    }

    /// Take the session of the newest banner the system showed since
    /// you last came to Vosh, which a click that starts Vosh again on
    /// Windows selects. A second start takes it once, so the next one
    /// from the Start menu only brings Vosh to the front.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn take_newest(&self) -> Option<SessionId> {
        match self {
            Banners::System(system) => system.newest().take(),
            #[cfg(test)]
            Banners::Recorded(_) => None,
        }
    }

    /// You came to the main window, so no banner from before is new.
    pub(crate) fn forget_newest(&self) {
        match self {
            Banners::System(system) => *system.newest() = None,
            #[cfg(test)]
            Banners::Recorded(_) => {}
        }
    }

    /// The banners a test build kept.
    #[cfg(test)]
    pub(crate) fn recorded(&self) -> Vec<Posted> {
        match self {
            Banners::Recorded(list) => list.lock().expect("the banners").clone(),
            Banners::System(_) => Vec::new(),
        }
    }
}

/// Whether the main window is minimized, hidden or gone, so it may not
/// play the page's tone. Waits on the main thread, so call it from a
/// blocking task.
fn main_hidden<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window("main").map_or(true, |w| {
        w.is_minimized().unwrap_or(false) || !w.is_visible().unwrap_or(true)
    })
}

impl SystemBanners {
    /// Post as [`Banners::post`] says, with `hidden` whether the main
    /// window may not play the page's tone, in which case the system
    /// plays a sound in its place. Returns whether it did.
    fn post<R: tauri::Runtime>(
        &self,
        app: &AppHandle<R>,
        banner: &Banner,
        fate: &Fate,
        hidden: bool,
    ) -> bool {
        let main = app.get_webview_window("main");
        let system_sound = fate.sound.is_some() && hidden;
        let mut played = false;
        if fate.banner {
            #[cfg(target_os = "macos")]
            let shown = {
                super::mac::post(banner, false);
                true
            };
            #[cfg(not(target_os = "macos"))]
            let shown = match super::desktop::post(app, banner, system_sound) {
                Some(sound) => {
                    played = sound;
                    true
                }
                None => false,
            };
            if shown {
                *self.newest() = Some(banner.session);
            }
        }
        #[cfg(target_os = "macos")]
        if system_sound {
            if let Some(sound) = &fate.sound {
                super::mac::play(sound);
                played = true;
            }
        }
        if let (Some(attention), Some(main)) = (fate.attention, main) {
            let kind = match attention {
                Attention::Once => UserAttentionType::Informational,
                Attention::Until => UserAttentionType::Critical,
            };
            if let Err(e) = main.request_user_attention(Some(kind)) {
                tracing::warn!(error = %e, "the attention call failed");
            }
        }
        played
    }
}

/// Answer clicks on banners at launch: a click selects the session its
/// banner names and brings the main window to the front.
pub(crate) fn install<R: tauri::Runtime>(app: &AppHandle<R>) {
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        super::mac::install(Box::new(move |session| show_session(&app, session)));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Whether you allow Vosh's banners. Waits on the system, so call it off
/// the main thread.
pub(crate) fn permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        super::mac::permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        super::desktop::permission()
    }
}

/// Ask the system to let Vosh post banners, and wait for your answer.
/// Only macOS asks, and elsewhere the answer is what the system says now.
#[cfg_attr(not(target_os = "macos"), allow(clippy::unused_async))]
pub(crate) async fn ask() -> Permission {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        super::mac::ask(Box::new(move |answer| {
            let _ = tx.send(answer);
        }));
        rx.await.unwrap_or(Permission::Unavailable)
    }
    #[cfg(not(target_os = "macos"))]
    {
        super::desktop::permission()
    }
}

/// Open the system's notification settings at Vosh.
pub(crate) fn open_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        super::mac::open_settings()
    }
    #[cfg(not(target_os = "macos"))]
    {
        super::desktop::open_settings()
    }
}

/// Select `session` and bring the main window to the front, as a click
/// on its banner does. Every window hears the selection.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) fn show_session<R: tauri::Runtime>(app: &AppHandle<R>, session: SessionId) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<SharedState>() else {
            return;
        };
        let state = state.inner().clone();
        match crate::app::launch::select_session(&app, &state, session).await {
            Ok(()) => broadcast(&app, SESSION_SELECTED, &SessionSelected { session }),
            Err(e) => tracing::warn!(error = %e, "a banner named a session Vosh no longer holds"),
        }
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.unminimize();
            let _ = main.show();
            let _ = main.set_focus();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_start_selects_the_newest_banner_once_and_none_once_you_came_back() {
        let banners = Banners::System(SystemBanners::default());
        let (one, two) = (SessionId::from_number(1), SessionId::from_number(2));
        let Banners::System(system) = &banners else {
            unreachable!()
        };
        *system.newest() = Some(one);
        banners.forget_newest();
        assert_eq!(banners.take_newest(), None, "you came back since");
        *system.newest() = Some(two);
        assert_eq!(banners.take_newest(), Some(two));
        assert_eq!(banners.take_newest(), None, "a start from the Start menu");
    }
}
