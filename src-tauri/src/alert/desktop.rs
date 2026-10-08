//! Banners on Windows and Linux, through tauri-plugin-notification, a
//! toast on Windows and the freedesktop server on Linux. The
//! plugin says banners are always allowed on the desktop, so Windows
//! reads its own setting from `ToastNotifier.Setting`. A click on a toast
//! of an installed Vosh starts the app again, since the toast has no
//! activator, and the single instance guard in lib.rs hands that start to
//! the Vosh that runs, which selects the session of the newest banner.
//! Linux shows the banner with no click until a listener of Vosh's own
//! takes the plugin's place.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use super::banner::{Banner, Permission};

/// The sound a banner asks the system for. Windows names its default
/// toast sound, and Linux a name from the freedesktop sound naming spec,
/// which a server that plays sounds knows.
#[cfg(windows)]
const SOUND: &str = "Default";
#[cfg(not(windows))]
const SOUND: &str = "message-new-instant";

/// Post `banner`, with the system's sound when `sound` says so. Returns
/// None when the banner did not go out, or else whether the system plays
/// that sound in place of the page's tone. On Windows it does while
/// Windows Settings lets Vosh show toasts, though Focus Assist may still
/// hush one. A Linux server may play no sound at all, so there the
/// page's tone plays as well.
pub(super) fn post<R: tauri::Runtime>(
    app: &AppHandle<R>,
    banner: &Banner,
    sound: bool,
) -> Option<bool> {
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
        builder = builder.sound(SOUND);
    }
    if let Err(e) = builder.show() {
        tracing::warn!(error = %e, "the banner did not go out");
        return None;
    }
    #[cfg(windows)]
    {
        Some(sound && windows_setting() == Permission::Granted)
    }
    #[cfg(not(windows))]
    {
        Some(false)
    }
}

/// Whether you allow Vosh's toasts, as Windows Settings says. Linux has
/// no such switch for an app.
pub(super) fn permission() -> Permission {
    #[cfg(windows)]
    {
        windows_setting()
    }
    #[cfg(not(windows))]
    {
        Permission::Granted
    }
}

#[cfg(windows)]
fn windows_setting() -> Permission {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{NotificationSetting, ToastNotificationManager};
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("com.aabahran.vosh"));
    match notifier.and_then(|n| n.Setting()) {
        Ok(NotificationSetting::Enabled) => Permission::Granted,
        Ok(_) => Permission::Denied,
        // A Vosh that is not installed has no toasts of its own yet.
        Err(_) => Permission::Unavailable,
    }
}

/// Open the system page for notifications.
pub(super) fn open_settings() -> Result<(), String> {
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer.exe")
        .arg("ms-settings:notifications")
        .spawn();
    #[cfg(not(windows))]
    let opened = std::process::Command::new("gnome-control-center")
        .arg("notifications")
        .spawn();
    opened.map(|_| ()).map_err(|_| {
        "Open the notification settings of your desktop to let Vosh show banners.".to_string()
    })
}
