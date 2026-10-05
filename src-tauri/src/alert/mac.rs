//! Banners on macOS, through `UNUserNotificationCenter` (Alerts Q1). The
//! notification plugin goes through the deprecated
//! `NSUserNotificationCenter` there, which drops banners, so Vosh keeps a
//! small module of its own. It posts, answers a click by selecting the
//! session, lets a banner show while Vosh is in front when its alert asks
//! for that. It also plays a system sound through `NSSound`, which Alerts
//! Q4 names for a window too hidden to play its own tone.
//!
//! `UNUserNotificationCenter` works only in a bundled app. A dev build runs
//! from no bundle and touching the center there aborts, so every call
//! first checks that the app has a bundle identifier, and without one
//! Vosh posts nothing and says banners are unavailable. A bundled build
//! that is not signed with com.aabahran.vosh may still see its banners
//! dropped, which Help says.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use objc2_06::rc::Retained;
use objc2_06::runtime::{AnyClass, AnyObject, Bool, ProtocolObject};
use objc2_06::{define_class, msg_send, AnyThread};
use objc2_foundation::{NSBundle, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use super::banner::Banner;
use crate::sessions::SessionId;

/// What a click on a banner does, given the session the banner names.
type OnClick = Box<dyn Fn(SessionId) + Send + Sync>;

static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

static NEXT: AtomicU64 = AtomicU64::new(1);

/// The prefix of every banner identifier, before the session's number.
const ID_PREFIX: &str = "vosh-";

/// Whether the app runs from a bundle, which `UNUserNotificationCenter`
/// needs.
fn bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
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
}
