//! Banners on Windows and Linux, through tauri-plugin-notification, a
//! toast on Windows and the freedesktop server on Linux (Alerts Q1). A
//! click on a toast of an installed Vosh starts the app again, since the
//! toast has no activator, and the single instance guard in lib.rs hands
//! that start to the Vosh that runs, which selects the session of the
//! newest banner.
//! Linux shows the banner with no click until a listener of Vosh's own
//! takes the plugin's place.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use super::banner::Banner;

/// Post `banner`, with the system's sound when `sound` says so.
pub(super) fn post<R: tauri::Runtime>(app: &AppHandle<R>, banner: &Banner, sound: bool) {
    let mut title = banner.title.clone();
    if let Some(label) = &banner.label {
        title = format!("{title} \u{b7} {label}");
    }
    let mut builder = app
        .notification()
        .builder()
        .title(title)
        .body(banner.body());
    if sound {
        builder = builder.sound("Default");
    }
    if let Err(e) = builder.show() {
        tracing::warn!(error = %e, "the banner did not go out");
    }
}
