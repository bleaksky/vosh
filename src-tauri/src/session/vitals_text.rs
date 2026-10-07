//! The vitals text a footer or the status line draws (Q7 to Q9 of the
//! Vitals Styles review). While the page watches it, the session renders
//! your text as the watch starts, on each Char.Vitals and Char.Combat,
//! each second while the text reads the tick and each minute while it
//! reads the time or the date, and when Settings saves a new text. Each
//! render goes out on `session://vitals-text` once the locks let go.

use std::sync::Mutex;
use std::time::Duration;

use chrono::Timelike;
use tauri::AppHandle;
use tokio::time::Instant;
use vosh_prompt::vitals::{self, VitalsText};
use vosh_prompt::Template;

use crate::app::events;
use crate::profile::live::Profile;
use crate::sessions::Session;

use super::connection::Connection;

/// The packages that move what a vitals text shows.
const PACKAGES: [&str; 2] = ["Char.Vitals", "Char.Combat"];

/// Whether the page draws the session's vitals text, and at what width.
/// A leaf lock, held for a copy.
#[derive(Debug, Default)]
pub(crate) struct VitalsWatch(Mutex<Watch>);

#[derive(Debug, Default, Clone, Copy)]
struct Watch {
    /// The width of the footer or the status line in terminal cells,
    /// None while nothing draws the text.
    cols: Option<usize>,
    /// When the tick or the clock the text reads shows something new.
    next: Option<Instant>,
}

impl VitalsWatch {
    fn lock(&self) -> std::sync::MutexGuard<'_, Watch> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn get(&self) -> Watch {
        *self.lock()
    }

    /// Draw the text `cols` cells wide from now on, or stop with None.
    pub(crate) fn watch(&self, cols: Option<usize>) {
        *self.lock() = Watch { cols, next: None };
    }

    /// A render `cols` wide next moves at `next`. A watch that stopped
    /// or changed width since the render began keeps its own.
    fn rendered(&self, cols: usize, next: Option<Instant>) {
        let mut watch = self.lock();
        if watch.cols == Some(cols) {
            watch.next = next;
        }
    }
}

/// Render the vitals text of `session`, while the page watches it, and
/// note when its clock next moves. Call under the profile and
/// connection locks, and emit what it returns once they let go.
pub(crate) fn render(
    session: &Session,
    p: &Profile,
    c: &Connection,
    now: Instant,
) -> Option<VitalsText> {
    let cols = session.vitals_watch.get().cols?;
    let template = Template::parse(&p.ui.vitals_text_drawn());
    let client = crate::prompt::client_values(p, c, now);
    let live = c.prompt.vars.resolver(&client);
    let clock = chrono::Local::now().naive_local();
    let drawn = vitals::draw(&template, &live, Some(cols), clock);
    let reads = vitals::clock(&template);
    let next = if reads.tick {
        Some(now + Duration::from_secs(1))
    } else if reads.wall {
        let into = Duration::new(u64::from(clock.second()), clock.nanosecond());
        Some(now + Duration::from_secs(60).saturating_sub(into))
    } else {
        None
    };
    session.vitals_watch.rendered(cols, next);
    Some(drawn)
}

/// Render after `package`, when it moves what the text shows.
pub(crate) fn after_package(
    session: &Session,
    p: &Profile,
    c: &Connection,
    package: &str,
    now: Instant,
) -> Option<VitalsText> {
    if PACKAGES.contains(&package) {
        render(session, p, c, now)
    } else {
        None
    }
}

/// Render when the tick or the clock the text reads shows something
/// new. The loop asks on each poll, with no lock held.
pub(crate) async fn on_poll<R: tauri::Runtime>(app: &AppHandle<R>, session: &Session) {
    let now = Instant::now();
    let due = session
        .vitals_watch
        .get()
        .next
        .is_some_and(|next| next <= now);
    if !due {
        return;
    }
    let drawn = {
        let p = session.lock_profile().await;
        let c = session.connection.lock();
        render(session, &p, &c, now)
    };
    emit(app, session, drawn);
}

/// Send a render to the page, with its session.
pub(crate) fn emit<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Session,
    drawn: Option<VitalsText>,
) {
    if let Some(drawn) = drawn {
        session.emit(app, events::VITALS_TEXT, &drawn);
    }
}
