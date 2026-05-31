//! System font enumeration + `font://` URI scheme.
//!
//! `WKWebView` on recent macOS refuses to match user-installed fonts by
//! CSS family name (anti-fingerprinting). The only path that still
//! works is `@font-face` with an explicit `src` URL. We expose the
//! system font catalog through a Tauri command and serve the actual
//! font bytes through a custom URI scheme; the frontend mints an
//! `@font-face` block pointing at `font://<family>` whenever the user
//! picks a system font, and the matched-by-CSS family name resolves
//! to the file we serve here.

use std::path::PathBuf;
use std::sync::OnceLock;

use font_kit::handle::Handle;
use font_kit::source::SystemSource;
use serde::Serialize;
use tauri::http::{Response, StatusCode};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FontEntry {
    /// CSS family name. What the user picks from the settings list and
    /// what we mint into the `@font-face` block.
    pub family: String,
    /// True when the regular face of this family advertises itself as
    /// monospace. The settings UI uses this to default the picker to
    /// monospaces, which is what makes sense for a terminal.
    pub monospace: bool,
}

/// Process-lifetime cache of the enumerated font list. Computing it
/// requires reading + parsing every installed font file to decide the
/// monospace flag, which costs 200–500ms on typical desktops. The set
/// rarely changes during a session, so we lock in the first result
/// and serve it instantly on every subsequent Settings open.
///
/// Failure modes (font-kit can't enumerate, system source unhappy)
/// also lock in: retrying with the same underlying state will not
/// produce a different answer, and the Settings panel already
/// degrades gracefully on an empty list.
static FONTS_CACHE: OnceLock<Vec<FontEntry>> = OnceLock::new();

/// List every distinct system font family. Sorted, deduped. Errors
/// from font-kit surface as an empty list rather than a hard fail so
/// the settings panel still opens on a partially-broken system.
#[tauri::command]
pub(crate) fn fonts_list() -> Vec<FontEntry> {
    FONTS_CACHE.get_or_init(enumerate_fonts).clone()
}

fn enumerate_fonts() -> Vec<FontEntry> {
    let source = SystemSource::new();
    let families = match source.all_families() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "font-kit failed to enumerate families");
            return Vec::new();
        }
    };

    // The cost here is dominated by `is_family_monospace`, which
    // loads + parses each font file from disk to read its monospace
    // flag. We pay it once per process; the FONTS_CACHE above keeps
    // every later call instant.
    let mut entries: Vec<FontEntry> = families
        .into_iter()
        .map(|family| {
            let monospace = is_family_monospace(&source, &family);
            FontEntry { family, monospace }
        })
        .collect();
    entries.sort_by_key(|a| a.family.to_lowercase());
    entries.dedup_by(|a, b| a.family == b.family);
    entries
}

fn is_family_monospace(source: &SystemSource, family: &str) -> bool {
    let Ok(handle) = source.select_family_by_name(family) else {
        return false;
    };
    handle
        .fonts()
        .first()
        .and_then(|h| h.load().ok())
        .is_some_and(|font| font.is_monospace())
}

/// Find the on-disk path of a font family's regular face. Returns
/// `None` for memory-only handles (which on our targets shouldn't
/// happen for system-installed fonts) and for unknown families.
pub(crate) fn font_path_for_family(family: &str) -> Option<(PathBuf, u32)> {
    let source = SystemSource::new();
    let handle_family = source.select_family_by_name(family).ok()?;
    let handle = handle_family.fonts().first()?.clone();
    match handle {
        Handle::Path { path, font_index } => Some((path, font_index)),
        Handle::Memory { .. } => None,
    }
}

/// `font://<family>` URI handler. Family name is percent-decoded out
/// of the URI path so spaces and Unicode round-trip cleanly. Serves
/// the raw font bytes with a font/ttf or font/otf MIME based on the
/// file extension. CSS @font-face will accept either.
pub(crate) fn handle_font_uri(uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let raw = uri.path().trim_start_matches('/');
    // Some webviews include the authority in the path; strip a leading
    // `//host` form too.
    let raw = raw.trim_start_matches('/');
    let family = match urlencoding::decode(raw) {
        Ok(s) => s.into_owned(),
        Err(_) => raw.to_string(),
    };
    let Some((path, _index)) = font_path_for_family(&family) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap();
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, family = %family, "failed to read font file");
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Vec::new())
                .unwrap();
        }
    };
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "font/ttf",
    };
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", mime)
        .header("access-control-allow-origin", "*")
        .body(bytes)
        .unwrap()
}
