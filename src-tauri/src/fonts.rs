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
    /// True when the first face the system lists for this family
    /// advertises itself as monospace. That face is not always the
    /// regular one. The settings UI uses this to default the picker to
    /// monospaces, which is what makes sense for a terminal.
    pub monospace: bool,
}

/// Process-lifetime cache of the enumerated font list. On macOS the
/// monospace flag comes from CoreText's font descriptors, which read
/// no font file, and the list takes about 0.1 s in a fresh process.
/// Before that, font-kit's `select_family_by_name` took about 2 s,
/// since it reads every file of each family. Windows and Linux still
/// load the first face of each family through font-kit. The set
/// rarely changes during a session, so we lock in the first result
/// and serve it instantly on every subsequent Settings open.
///
/// Failure modes (font-kit can't enumerate, system source unhappy)
/// also lock in: retrying with the same underlying state will not
/// produce a different answer, and the Settings panel already
/// degrades gracefully on an empty list.
///
/// Fill it only through `cached_fonts` on the blocking pool. Its
/// `get_or_init` blocks for the whole enumeration, and on the main
/// thread that freezes the app.
static FONTS_CACHE: OnceLock<Vec<FontEntry>> = OnceLock::new();

/// List every distinct system font family. Sorted, deduped. Errors
/// from font-kit surface as an empty list rather than a hard fail so
/// the settings panel still opens on a partially-broken system.
///
/// Async so it never runs on the main thread. Tauri runs a sync
/// command there on macOS, and the first enumeration of a launch took
/// 2 to 3 s, which froze every window until it finished. A cached list
/// returns at once. Otherwise the enumeration runs on the blocking
/// pool, and a call that lands while another thread enumerates waits
/// there for that result.
#[tauri::command]
pub(crate) async fn fonts_list() -> Vec<FontEntry> {
    if let Some(list) = FONTS_CACHE.get() {
        return list.clone();
    }
    tauri::async_runtime::spawn_blocking(|| cached_fonts(&FONTS_CACHE, enumerate_fonts).to_vec())
        .await
        .unwrap_or_default()
}

/// Start reading the font list on the blocking pool at launch, so the
/// first Appearance open finds it in the cache. It returns at once.
/// macOS only, where the read takes about 0.1 s and opens no font
/// file. Windows and Linux read the list when Appearance first asks,
/// since their cost was never measured.
#[cfg(target_os = "macos")]
pub(crate) fn warm_font_cache() {
    tauri::async_runtime::spawn_blocking(|| {
        cached_fonts(&FONTS_CACHE, enumerate_fonts);
    });
}

/// The list in `cache`, enumerating it first when the cache is empty.
/// It blocks while the enumeration runs, in this call or in another
/// thread's, so call it only on the blocking pool, never on the main
/// thread.
fn cached_fonts(
    cache: &OnceLock<Vec<FontEntry>>,
    enumerate: impl FnOnce() -> Vec<FontEntry>,
) -> &[FontEntry] {
    cache.get_or_init(enumerate)
}

fn enumerate_fonts() -> Vec<FontEntry> {
    debug_assert_ne!(
        std::thread::current().name(),
        Some("main"),
        "font enumeration blocks, keep it off the main thread"
    );
    let source = SystemSource::new();
    let families = match source.all_families() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "font-kit failed to enumerate families");
            return Vec::new();
        }
    };

    // The cost here is the monospace flag of each family. We pay it
    // once per process; the FONTS_CACHE above keeps every later call
    // instant.
    let mut entries: Vec<FontEntry> = families
        .into_iter()
        .map(|family| {
            #[cfg(target_os = "macos")]
            let monospace = is_family_monospace(&family);
            #[cfg(not(target_os = "macos"))]
            let monospace = is_family_monospace_font_kit(&source, &family);
            FontEntry { family, monospace }
        })
        .collect();
    entries.sort_by_key(|a| a.family.to_lowercase());
    entries.dedup_by(|a, b| a.family == b.family);
    entries
}

/// The monospace flag of the first face CoreText lists for `family`,
/// read from its font descriptor. It builds the family query font-kit's
/// `select_family_by_name` builds and reads the first match, which is
/// the face font-kit's handle list starts with whenever font-kit can
/// read its file. It loads no font, where font-kit read every file of
/// the family and took about 2 s for the whole list. Do not swap in
/// `CTFontDescriptorCreateMatchingFontDescriptor` or look at any other
/// face. Both change the answer for some families. An unknown family
/// is not monospace.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn is_family_monospace(family: &str) -> bool {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_text::font_collection;
    use core_text::font_descriptor::{
        self, kCTFontMonoSpaceTrait, kCTFontSymbolicTrait, kCTFontTraitsAttribute,
        CTFontDescriptorCopyAttribute,
    };

    let attributes: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[(
        CFString::new("NSFontFamilyAttribute"),
        CFString::new(family).as_CFType(),
    )]);
    let query = font_descriptor::new_from_attributes(&attributes);
    let collection = font_collection::new_from_descriptors(&CFArray::from_CFTypes(&[query]));
    let Some(descriptors) = collection.get_descriptors() else {
        return false;
    };
    let Some(first) = descriptors.get(0) else {
        return false;
    };
    // SAFETY: the descriptor is a live CTFontDescriptor and the key is
    // CoreText's own constant. The copy comes back retained, or null
    // when the face has no traits, and the create rule wrap releases it.
    let value = unsafe {
        let raw =
            CTFontDescriptorCopyAttribute(first.as_concrete_TypeRef(), kCTFontTraitsAttribute);
        if raw.is_null() {
            return false;
        }
        CFType::wrap_under_create_rule(raw)
    };
    if !value.instance_of::<CFDictionary>() {
        return false;
    }
    // SAFETY: the value is a CFDictionary, checked above, and the get
    // rule wrap retains it for as long as `traits` lives. The key is
    // CoreText's own constant.
    let (traits, key) = unsafe {
        let traits: CFDictionary<CFString, CFType> =
            CFDictionary::wrap_under_get_rule(value.as_CFTypeRef() as CFDictionaryRef);
        (traits, CFString::wrap_under_get_rule(kCTFontSymbolicTrait))
    };
    traits
        .find(&key)
        .and_then(|symbolic| symbolic.downcast::<CFNumber>())
        .and_then(|symbolic| symbolic.to_i64())
        .is_some_and(|bits| bits & i64::from(kCTFontMonoSpaceTrait) != 0)
}

/// The monospace flag of the first face font-kit lists for `family`,
/// read by loading that face. Windows and Linux use it. On macOS the
/// tests keep it to check the CoreText answer against.
#[cfg(any(test, not(target_os = "macos")))]
fn is_family_monospace_font_kit(source: &SystemSource, family: &str) -> bool {
    let Ok(handle) = source.select_family_by_name(family) else {
        return false;
    };
    handle
        .fonts()
        .first()
        .and_then(|h| h.load().ok())
        .is_some_and(|font| font.is_monospace())
}

/// Find the on-disk path of the first face font-kit lists for a
/// family, which is not always the regular one. Returns
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn same(a: &[FontEntry], b: &[FontEntry]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| x.family == y.family && x.monospace == y.monospace)
    }

    #[test]
    fn enumerate_sorts_without_case_and_lists_each_family_once() {
        let list = enumerate_fonts();
        for pair in list.windows(2) {
            assert!(
                pair[0].family.to_lowercase() <= pair[1].family.to_lowercase(),
                "{:?} sorts after {:?}",
                pair[0].family,
                pair[1].family
            );
        }
        let mut seen = HashSet::new();
        for entry in &list {
            assert!(
                seen.insert(&entry.family),
                "{:?} listed twice",
                entry.family
            );
        }
        // Every macOS and Windows install has fonts. A Linux box can
        // lack fontconfig fonts, and the list is then empty.
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        assert!(!list.is_empty(), "no fonts listed");
    }

    #[test]
    fn threads_that_ask_at_once_share_one_enumeration() {
        let cache = OnceLock::new();
        let runs = AtomicUsize::new(0);
        let lists: Vec<Vec<FontEntry>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..6)
                .map(|_| {
                    scope.spawn(|| {
                        cached_fonts(&cache, || {
                            runs.fetch_add(1, Ordering::SeqCst);
                            enumerate_fonts()
                        })
                        .to_vec()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        for list in &lists[1..] {
            assert!(same(&lists[0], list));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn core_text_flags_the_monospace_families() {
        for family in ["Menlo", "Monaco"] {
            assert!(is_family_monospace(family), "{family} is monospace");
        }
        // font-kit found no face for PingFang SC, so it was never
        // monospace there either.
        for family in ["Helvetica", "Times", "PingFang SC"] {
            assert!(!is_family_monospace(family), "{family} is proportional");
        }
        assert!(!is_family_monospace("No Such Family Vosh Test"));
        assert!(!is_family_monospace(""));
    }

    // Slow, about 2 s in a debug build, since the font-kit side reads
    // every font file. Run it with --ignored after a change to either
    // check or a macOS update.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "reads every installed font file"]
    fn core_text_agrees_with_font_kit_on_every_family() {
        let source = SystemSource::new();
        let families = source.all_families().expect("font-kit lists the families");
        let differ: Vec<(&String, bool)> = families
            .iter()
            .map(|family| (family, is_family_monospace(family)))
            .filter(|(family, mono)| *mono != is_family_monospace_font_kit(&source, family))
            .collect();
        assert!(
            differ.is_empty(),
            "CoreText and font-kit disagree on {differ:?}"
        );
    }

    #[test]
    fn the_command_lists_off_the_main_thread() {
        let first = tauri::async_runtime::block_on(fonts_list());
        let again = tauri::async_runtime::block_on(fonts_list());
        assert!(same(&first, &enumerate_fonts()));
        assert!(same(&first, &again));
    }
}
