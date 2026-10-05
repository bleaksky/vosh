//! Where a banner goes. [`Banners::System`] bounces the Dock or flashes
//! the taskbar. A test build holds [`Banners::Recorded`] instead, which
//! keeps each banner for the test to read, so no test ever posts one or
//! touches the system's notification center.

#[cfg(test)]
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, UserAttentionType};

use super::focus::Fate;
use super::Attention;
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

/// A banner a test build kept, with what the focus rule said of it.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Posted {
    pub(crate) banner: Banner,
    pub(crate) fate: Fate,
}

/// Where banners go.
pub(crate) enum Banners {
    /// The system's attention calls.
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
            Self::System(SystemBanners)
        }
    }
}

/// The system's side of an alert.
#[derive(Debug, Default)]
pub(crate) struct SystemBanners;

impl Banners {
    /// Post `banner` as `fate` says, and bounce the Dock or flash the
    /// taskbar. Returns true when Vosh played a system sound in place of
    /// the page's tone.
    pub(crate) fn post<R: tauri::Runtime>(
        &self,
        app: &AppHandle<R>,
        banner: &Banner,
        fate: &Fate,
    ) -> bool {
        match self {
            Banners::System(system) => {
                let _ = banner;
                system.post(app, fate)
            }
            #[cfg(test)]
            Banners::Recorded(list) => {
                if fate.banner || fate.attention.is_some() {
                    list.lock().expect("the banners").push(Posted {
                        banner: banner.clone(),
                        fate: fate.clone(),
                    });
                }
                false
            }
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

impl SystemBanners {
    #[allow(clippy::unused_self)]
    fn post<R: tauri::Runtime>(&self, app: &AppHandle<R>, fate: &Fate) -> bool {
        if let (Some(attention), Some(main)) = (fate.attention, app.get_webview_window("main")) {
            let kind = match attention {
                Attention::Once => UserAttentionType::Informational,
                Attention::Until => UserAttentionType::Critical,
            };
            if let Err(e) = main.request_user_attention(Some(kind)) {
                tracing::warn!(error = %e, "the attention call failed");
            }
        }
        false
    }
}
