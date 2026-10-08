//! Banners on macOS, through `UNUserNotificationCenter`. The
//! notification plugin goes through the deprecated
//! `NSUserNotificationCenter` there, which drops banners, so Vosh keeps a
//! small module of its own. It posts, answers a click by selecting the
//! session, lets a banner show while Vosh is in front when its alert asks
//! for that, reads whether you allow banners and asks, and takes back the
//! banners of a plugin that turned off. It also plays
//! a system sound through `NSSound`, for a window too hidden to play its
//! own tone.
//!
//! `UNUserNotificationCenter` works only in a bundled app. A dev build runs
//! from no bundle and touching the center there aborts, so every call
//! first checks that the app has a bundle identifier, and without one
//! Vosh posts nothing and says banners are unavailable. A bundled build
//! that is not signed with com.aabahran.vosh may still see its banners
//! dropped, which Help says.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2_06::rc::Retained;
use objc2_06::runtime::{AnyClass, AnyObject, Bool, ProtocolObject};
use objc2_06::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::banner::{Banner, Permission};
use crate::sessions::SessionId;

/// What a click on a banner does, given the session the banner names.
type OnClick = Box<dyn Fn(SessionId) + Send + Sync>;

static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

/// The banners still showing, by identifier, with the session and the
/// Lua owner of each, so a plugin that turns off takes its own back.
static SHOWING: Mutex<Vec<(String, SessionId, Option<String>)>> = Mutex::new(Vec::new());

/// The most banners [`SHOWING`] keeps track of.
const SHOWING_KEPT: usize = 100;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// The prefix of every banner identifier, before the session's number.
const ID_PREFIX: &str = "vosh-";

/// Whether the app runs from an app bundle, which
/// `UNUserNotificationCenter` needs. A dev build may carry an identifier
/// in the Info.plist its binary embeds and still run from no bundle, so
/// the bundle's path must name an app too.
fn bundled() -> bool {
    let bundle = NSBundle::mainBundle();
    let path = bundle.bundlePath().to_string();
    let app = std::path::Path::new(&path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"));
    bundle.bundleIdentifier().is_some() && app
}

fn center() -> Option<Retained<UNUserNotificationCenter>> {
    bundled().then(UNUserNotificationCenter::currentNotificationCenter)
}

#[derive(Default)]
struct DelegateIvars;

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the delegate
    // implements no Drop.
    #[unsafe(super(NSObject))]
    #[name = "VoshNotificationDelegate"]
    #[ivars = DelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// A banner whose alert rings while you look at Vosh shows anyway,
        /// since Vosh posts one only when the focus rule asked for it.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        /// A click on a banner selects its session and brings Vosh to the
        /// front. It never sends anything to the game.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            if let (Some(session), Some(on_click)) = (session_of(&id), ON_CLICK.get()) {
                on_click(session);
            }
            handler.call(());
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars);
        // SAFETY: NSObject's init takes no arguments and returns the
        // object it was given.
        unsafe { msg_send![super(this), init] }
    }
}

/// Answer clicks and let banners show while Vosh is in front, with
/// `on_click` for a click. Call once, at launch.
pub(super) fn install(on_click: OnClick) {
    let _ = ON_CLICK.set(on_click);
    let Some(center) = center() else {
        return;
    };
    let delegate = Delegate::new();
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // The center holds its delegate weakly, so the delegate lives as long
    // as the app.
    std::mem::forget(delegate);
}

/// The identifier of a banner for `session`.
fn banner_id(session: SessionId) -> String {
    format!(
        "{ID_PREFIX}{session}-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// The session a banner identifier names.
fn session_of(id: &str) -> Option<SessionId> {
    let rest = id.strip_prefix(ID_PREFIX)?;
    let (number, _) = rest.split_once('-')?;
    number.parse::<u32>().ok().map(SessionId::from_number)
}

/// Post `banner`, with the system's sound when `sound` says so. Does
/// nothing in a build with no bundle.
pub(super) fn post(banner: &Banner, sound: bool) {
    let Some(center) = center() else {
        return;
    };
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(&banner.title));
    if let Some(label) = &banner.label {
        content.setSubtitle(&NSString::from_str(label));
    }
    content.setBody(&NSString::from_str(banner.body()));
    content.setThreadIdentifier(&NSString::from_str(&format!(
        "{ID_PREFIX}session-{}",
        banner.session
    )));
    if sound {
        content.setSound(Some(&UNNotificationSound::defaultSound()));
    }
    let id = banner_id(banner.session);
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&id),
        &content,
        None,
    );
    center.addNotificationRequest_withCompletionHandler(&request, None);
    let mut showing = SHOWING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    showing.push((id, banner.session, banner.owner.clone()));
    if showing.len() > SHOWING_KEPT {
        let extra = showing.len() - SHOWING_KEPT;
        showing.drain(..extra);
    }
}

/// Take back the banners of `session` that the Lua `owner` posted.
pub(super) fn withdraw(session: SessionId, owner: &str) {
    let ids: Vec<String> = {
        let mut showing = SHOWING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (gone, kept): (Vec<_>, Vec<_>) = showing
            .drain(..)
            .partition(|(_, s, o)| *s == session && o.as_deref() == Some(owner));
        *showing = kept;
        gone.into_iter().map(|(id, ..)| id).collect()
    };
    if ids.is_empty() {
        return;
    }
    let Some(center) = center() else {
        return;
    };
    let ids: Vec<Retained<NSString>> = ids.iter().map(|id| NSString::from_str(id)).collect();
    center.removeDeliveredNotificationsWithIdentifiers(&NSArray::from_retained_slice(&ids));
}

/// Whether you allow Vosh's banners, as System Settings says. Waits up
/// to two seconds for the answer, so call it off the main thread.
pub(super) fn permission() -> Permission {
    let Some(center) = center() else {
        return Permission::Unavailable;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let block = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        // SAFETY: the center hands a settings object that lives through
        // the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(status);
        }
    });
    center.getNotificationSettingsWithCompletionHandler(&block);
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(status) => permission_of(status),
        Err(_) => Permission::Unavailable,
    }
}

/// Ask macOS to let Vosh post banners, which shows its own question the
/// first time. Hands the answer to `answer` once you choose.
pub(super) fn ask(answer: Box<dyn FnOnce(Permission) + Send>) {
    let Some(center) = center() else {
        answer(Permission::Unavailable);
        return;
    };
    let answer = Mutex::new(Some(answer));
    let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let taken = answer.lock().ok().and_then(|mut a| a.take());
        if let Some(answer) = taken {
            answer(if granted.as_bool() {
                Permission::Granted
            } else {
                Permission::Denied
            });
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &block,
    );
}

fn permission_of(status: UNAuthorizationStatus) -> Permission {
    match status {
        UNAuthorizationStatus::NotDetermined => Permission::NotAsked,
        UNAuthorizationStatus::Denied => Permission::Denied,
        _ => Permission::Granted,
    }
}

/// Open the Notifications page of System Settings at Vosh.
pub(super) fn open_settings() -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=com.aabahran.vosh")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Vosh could not open System Settings ({e})."))
}

/// The system sound that stands in for the tone `sound`.
fn system_sound(sound: &str) -> &'static str {
    match sound {
        "bell" => "Ping",
        "knock" => "Tink",
        "low" => "Basso",
        _ => "Glass",
    }
}

/// Play the system sound for the tone `sound` through `NSSound`.
pub(super) fn play(sound: &str) {
    let Some(class) = AnyClass::get(c"NSSound") else {
        return;
    };
    let name = NSString::from_str(system_sound(sound));
    // SAFETY: soundNamed: takes an NSString and returns an NSSound or
    // nil, and play takes nothing and returns a BOOL.
    unsafe {
        let found: Option<Retained<AnyObject>> = msg_send![class, soundNamed: &*name];
        if let Some(found) = found {
            let _: Bool = msg_send![&*found, play];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_banner_names_its_session_in_its_identifier() {
        let session = SessionId::from_number(7);
        let id = banner_id(session);
        assert!(id.starts_with("vosh-7-"), "{id}");
        assert_eq!(session_of(&id), Some(session));
        assert_eq!(session_of("other-7-1"), None);
    }

    #[test]
    fn each_tone_has_a_system_sound() {
        let sounds: Vec<&str> = ["chime", "bell", "knock", "low"]
            .into_iter()
            .map(system_sound)
            .collect();
        assert_eq!(sounds, ["Glass", "Ping", "Tink", "Basso"]);
    }

    #[test]
    fn the_page_draws_the_tones_that_have_system_sounds() {
        // The page draws each tone with Web Audio from ALERT_TONES in
        // src/stores/session/alertTones.ts, and a system sound stands in
        // for it only while the window hides, so both sides know the same
        // four.
        let page = include_str!("../../../src/stores/session/alertTones.ts");
        let start = page
            .find("export const ALERT_TONES")
            .expect("alertTones.ts declares ALERT_TONES");
        let list = &page[start..];
        let list = &list[..list.find("];").expect("the page list ends")];
        let tones: Vec<&str> = regex::Regex::new(r"value: '(\w+)'")
            .unwrap()
            .captures_iter(list)
            .map(|caps| caps.get(1).unwrap().as_str())
            .collect();
        assert_eq!(tones, ["chime", "bell", "knock", "low"]);
    }
}
