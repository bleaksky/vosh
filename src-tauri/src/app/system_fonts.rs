//! System font enumeration + the `font` URI scheme.
//!
//! `WKWebView` on recent macOS refuses to match user-installed fonts by
//! CSS family name (anti-fingerprinting). The only path that still
//! works is `@font-face` with an explicit `src` URL. We expose the
//! system font catalog through a Tauri command and serve the actual
//! font bytes through a custom URI scheme; the frontend mints an
//! `@font-face` block whenever the user picks a system font, with a
//! URL from Tauri's `convertFileSrc` that carries the family in its
//! path (`font://localhost/<family>` on macOS and Linux,
//! `http://font.localhost/<family>` on Windows), and the
//! matched-by-CSS family name resolves to the face we serve here.
//!
//! The scheme serves the regular face of the family. On macOS that
//! face comes from CoreText's font descriptors, the same faces the
//! monospace flag reads, and its file from `kCTFontURLAttribute`.
//! Windows and Linux serve the first face font-kit lists. A face that
//! lives in a collection (`.ttc`) is cut out of it first, since `WebKit`
//! renders the first face of a collection served whole, and that is
//! the bold face of Avenir Next. On macOS a face CoreText cannot load
//! from memory gets a 404, since `WebKit` loads web fonts that way.
//! CoreText reads `PingFangUI.ttc` from disk but not from memory.

use std::path::Path;
use std::sync::OnceLock;

#[cfg(not(target_os = "macos"))]
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
    /// monospace (on macOS; Windows and Linux still read the first face
    /// font-kit lists). The settings UI uses this to default the picker
    /// to monospaces, which is what makes sense for a terminal.
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
/// A cached list returns at once. Otherwise the enumeration runs on the
/// blocking pool, and a call that lands while another thread enumerates
/// waits there for that result.
pub(crate) async fn list() -> Vec<FontEntry> {
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

/// The monospace flag of the regular face of `family`, read from
/// CoreText's font descriptors through [`regular_descriptor`]. It loads
/// no font, where font-kit read every file of the family and took
/// about 2 s for the whole list. It reads the regular face, not the
/// first one CoreText lists: some families list Italic first, and only
/// their Regular face says it is monospace, so the first face left them
/// out of the Font list. An unknown family is not monospace.
#[cfg(target_os = "macos")]
fn is_family_monospace(family: &str) -> bool {
    use core_text::font_descriptor::kCTFontMonoSpaceTrait;
    regular_descriptor(family)
        .is_some_and(|(_, face)| face.symbolic & i64::from(kCTFontMonoSpaceTrait) != 0)
}

/// The descriptor and traits of the regular face of `family`. It builds
/// the family query font-kit's `select_family_by_name` builds, then
/// picks the face [`regular_face`] names: upright, with weight and
/// width nearest normal. It reads no font file. Do not swap in
/// `CTFontDescriptorCreateMatchingFontDescriptor`, which picks Medium
/// for some families. None for an unknown family.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn regular_descriptor(
    family: &str,
) -> Option<(core_text::font_descriptor::CTFontDescriptor, FaceTraits)> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_text::font_collection;
    use core_text::font_descriptor::{
        self, kCTFontSymbolicTrait, kCTFontTraitsAttribute, kCTFontWeightTrait, kCTFontWidthTrait,
        CTFontDescriptorCopyAttribute,
    };

    let attributes: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[(
        CFString::new("NSFontFamilyAttribute"),
        CFString::new(family).as_CFType(),
    )]);
    let query = font_descriptor::new_from_attributes(&attributes);
    let collection = font_collection::new_from_descriptors(&CFArray::from_CFTypes(&[query]));
    let descriptors = collection.get_descriptors()?;
    // SAFETY: each key is CoreText's own constant, and the get rule wrap
    // retains it for as long as the string lives.
    let (symbolic_key, weight_key, width_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCTFontSymbolicTrait),
            CFString::wrap_under_get_rule(kCTFontWeightTrait),
            CFString::wrap_under_get_rule(kCTFontWidthTrait),
        )
    };
    let mut faces: Vec<(font_descriptor::CTFontDescriptor, FaceTraits)> = descriptors
        .iter()
        .filter_map(|descriptor| {
            // SAFETY: the descriptor is a live CTFontDescriptor and the
            // key is CoreText's own constant. The copy comes back
            // retained, or null when the face has no traits, and the
            // create rule wrap releases it.
            let value = unsafe {
                let raw = CTFontDescriptorCopyAttribute(
                    descriptor.as_concrete_TypeRef(),
                    kCTFontTraitsAttribute,
                );
                if raw.is_null() {
                    return None;
                }
                CFType::wrap_under_create_rule(raw)
            };
            if !value.instance_of::<CFDictionary>() {
                return None;
            }
            // SAFETY: the value is a CFDictionary, checked above, and the
            // get rule wrap retains it for as long as `traits` lives.
            let traits: CFDictionary<CFString, CFType> = unsafe {
                CFDictionary::wrap_under_get_rule(value.as_CFTypeRef() as CFDictionaryRef)
            };
            let number = |key: &CFString| traits.find(key).and_then(|n| n.downcast::<CFNumber>());
            let symbolic = number(&symbolic_key).and_then(|n| n.to_i64())?;
            let face = FaceTraits {
                symbolic,
                weight: number(&weight_key).and_then(|n| n.to_f64()).unwrap_or(0.0),
                width: number(&width_key).and_then(|n| n.to_f64()).unwrap_or(0.0),
            };
            Some((descriptor.clone(), face))
        })
        .collect();
    let traits: Vec<FaceTraits> = faces.iter().map(|(_, face)| *face).collect();
    let index = regular_face(&traits)?;
    Some(faces.swap_remove(index))
}

/// The traits of one face that pick the regular face of a family.
#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct FaceTraits {
    /// CoreText's symbolic traits, italic and monospace among them.
    symbolic: i64,
    /// Normalized weight, 0 for regular.
    weight: f64,
    /// Normalized width, 0 for normal.
    width: f64,
}

/// The index of the regular face of a family: upright before italic,
/// then the weight nearest regular, then the width nearest normal, then
/// the first listed. None for a family with no face.
#[cfg(any(test, target_os = "macos"))]
fn regular_face(faces: &[FaceTraits]) -> Option<usize> {
    // kCTFontItalicTrait, written out so the tests build on every system.
    const ITALIC: i64 = 1;
    let key = |f: &FaceTraits| (f.symbolic & ITALIC != 0, f.weight.abs(), f.width.abs());
    faces
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let (ia, wa, da) = key(a);
            let (ib, wb, db) = key(b);
            ia.cmp(&ib).then(wa.total_cmp(&wb)).then(da.total_cmp(&db))
        })
        .map(|(index, _)| index)
}

/// The monospace flag of the first face font-kit lists for `family`,
/// read by loading that face. Windows and Linux use it.
#[cfg(not(target_os = "macos"))]
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

/// The bytes of the regular face of `family`, the face
/// [`regular_descriptor`] picks, read from the file its
/// `kCTFontURLAttribute` names. A file of one face comes back whole,
/// variable or not. A collection gets the face [`collection_face`]
/// finds, cut out by [`face_bytes`]. 404 for an unknown family, a face
/// with no file, a collection with no such face, or a face CoreText
/// cannot load from memory.
#[cfg(target_os = "macos")]
fn face_for_family(family: &str) -> Result<Vec<u8>, StatusCode> {
    let (descriptor, _) = regular_descriptor(family).ok_or(StatusCode::NOT_FOUND)?;
    let path = descriptor.font_path().ok_or(StatusCode::NOT_FOUND)?;
    let data = read_font(&path, family)?;
    let index = if data.starts_with(b"ttcf") {
        collection_face(&descriptor, &data)
    } else {
        Some(0)
    };
    let Some(bytes) = index.and_then(|index| face_bytes(data, index)) else {
        tracing::warn!(family = %family, path = %path.display(), "no regular face in font file");
        return Err(StatusCode::NOT_FOUND);
    };
    // WebKit loads a web font with this same CoreText call, so a face
    // it fails on would never render. PingFang lives in such a file.
    if core_text::font_manager::create_font_descriptor(&bytes).is_err() {
        tracing::debug!(family = %family, path = %path.display(), "CoreText cannot load the face from memory");
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(bytes)
}

/// The index of the face `descriptor` names in the collection `data`,
/// the face whose `name` table holds the bytes CoreText reads from the
/// face. CoreText tells no face index. The place of the face among the
/// descriptors CoreText reads from the file is no index either, since
/// CoreText lists a variable face once per named instance. None when
/// no face of `data` has that table.
#[cfg(target_os = "macos")]
fn collection_face(
    descriptor: &core_text::font_descriptor::CTFontDescriptor,
    data: &[u8],
) -> Option<u32> {
    const NAME: [u8; 4] = *b"name";
    let table = core_text::font::new_from_descriptor(descriptor, 12.0)
        .get_font_table(u32::from_be_bytes(NAME))?;
    face_with_table(data, NAME, table.bytes())
}

/// The bytes of the face of `family` the font scheme serves on Windows
/// and Linux, the first face font-kit lists, which is not always the
/// regular one. A face of a collection is cut out by [`face_bytes`].
/// 404 for an unknown family or a face font-kit holds only in memory.
#[cfg(not(target_os = "macos"))]
fn face_for_family(family: &str) -> Result<Vec<u8>, StatusCode> {
    let source = SystemSource::new();
    let handle = source
        .select_family_by_name(family)
        .ok()
        .and_then(|handles| handles.fonts().first().cloned());
    let Some(Handle::Path { path, font_index }) = handle else {
        return Err(StatusCode::NOT_FOUND);
    };
    let data = read_font(&path, family)?;
    face_bytes(data, font_index).ok_or_else(|| {
        tracing::warn!(family = %family, font_index, path = %path.display(), "no such face in font file");
        StatusCode::NOT_FOUND
    })
}

/// The bytes of the font file at `path`, or 500 when it cannot be read.
fn read_font(path: &Path, family: &str) -> Result<Vec<u8>, StatusCode> {
    std::fs::read(path).map_err(|e| {
        tracing::warn!(error = %e, family = %family, "failed to read font file");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// The family a font scheme URL asks for, percent-decoded out of its
/// path. fontLoader.ts builds the URL with Tauri's `convertFileSrc`,
/// which puts the encoded family in the path on every platform:
/// `font://localhost/<family>` on macOS and Linux,
/// `http://font.localhost/<family>` on Windows. None for an empty path
/// or one that does not decode to UTF-8.
fn family_from_uri(uri: &tauri::http::Uri) -> Option<String> {
    let raw = uri.path().trim_start_matches('/');
    let family = urlencoding::decode(raw).ok()?.into_owned();
    (!family.is_empty()).then_some(family)
}

/// The big endian 32 bit number at `at` in `data`, or None past its end.
fn be32(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    usize::try_from(u32::from_be_bytes(bytes.try_into().ok()?)).ok()
}

/// Where the table directory of face `index` of the collection `data`
/// starts and how many tables it lists, checked to end inside the
/// file. Each table has a 16 byte record after the 12 byte directory
/// header. None for a file that is not a collection or an index it
/// does not have.
fn face_directory(data: &[u8], index: usize) -> Option<(usize, usize)> {
    if !data.starts_with(b"ttcf") || index >= be32(data, 8)? {
        return None;
    }
    let directory = be32(data, 12usize.checked_add(index.checked_mul(4)?)?)?;
    let count = data.get(directory.checked_add(4)?..directory.checked_add(6)?)?;
    let tables = usize::from(u16::from_be_bytes(count.try_into().ok()?));
    let end = directory.checked_add(12 + 16 * tables)?;
    (end <= data.len()).then_some((directory, tables))
}

/// The single face at `index` of the font file bytes in `data`. A file
/// that holds one face comes back whole at index 0. A collection
/// (`.ttc` or `.otc`) gets that face's table directory copied to the
/// front, the way font-kit loads one, which makes it a font of that
/// face alone, since each directory points at its tables by offset
/// from the start of the file. The other faces' bytes stay behind,
/// unused. None for an index the file does not have, or a collection
/// with a table where the copied directory would land.
fn face_bytes(mut data: Vec<u8>, index: u32) -> Option<Vec<u8>> {
    if !data.starts_with(b"ttcf") {
        return (index == 0).then_some(data);
    }
    let (directory, tables) = face_directory(&data, usize::try_from(index).ok()?)?;
    let len = 12 + 16 * tables;
    for table in 0..tables {
        if be32(&data, directory + 12 + 16 * table + 8)? < len {
            return None;
        }
    }
    data.copy_within(directory..directory + len, 0);
    Some(data)
}

/// The index of the first face of the collection `data` with a `tag`
/// table that holds exactly `body`. None for a file that is not a
/// collection, or when no face has such a table.
#[cfg(any(test, target_os = "macos"))]
fn face_with_table(data: &[u8], tag: [u8; 4], body: &[u8]) -> Option<u32> {
    let table_at = |record: usize| {
        let offset = be32(data, record + 8)?;
        data.get(offset..offset.checked_add(be32(data, record + 12)?)?)
    };
    let holds = |index: usize| {
        face_directory(data, index).is_some_and(|(directory, tables)| {
            (0..tables).any(|table| {
                let record = directory + 12 + 16 * table;
                data.get(record..record + 4) == Some(&tag[..]) && table_at(record) == Some(body)
            })
        })
    };
    // Each face takes four bytes of the header, which bounds the count.
    let faces = be32(data, 8)?.min(data.len() / 4);
    (0..faces)
        .find(|&index| holds(index))
        .and_then(|index| u32::try_from(index).ok())
}

/// The media type of single face font bytes, read from their first
/// four bytes. `WebKit` reads the bytes whatever the header says.
fn font_mime(bytes: &[u8]) -> &'static str {
    match bytes.get(..4) {
        Some(b"OTTO") => "font/otf",
        Some(b"wOFF") => "font/woff",
        Some(b"wOF2") => "font/woff2",
        _ => "font/ttf",
    }
}

/// The sizes a family draws at, for the Size selects. A face with
/// outlines scales to any size, half steps such as 13.5 among them. A
/// bitmap only face holds fixed sizes, its strikes, and draws only those
/// cleanly, so Settings keeps it to whole sizes, or to its strikes when
/// the face lists them.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct FontSizing {
    /// True for a face with outlines, and for a face Vosh cannot judge.
    pub half_sizes: bool,
    /// The sizes in pixels a bitmap only face holds, smallest first.
    /// Empty for a face with outlines or one that lists none.
    pub strikes: Vec<u32>,
}

impl FontSizing {
    /// A face that takes any size.
    const SCALABLE: Self = Self {
        half_sizes: true,
        strikes: Vec::new(),
    };

    /// A bitmap only face with these strikes.
    fn bitmap(mut strikes: Vec<u32>) -> Self {
        strikes.sort_unstable();
        strikes.dedup();
        Self {
            half_sizes: false,
            strikes,
        }
    }
}

/// The sizes `family` draws at, read from its regular face on the
/// blocking pool. A family Vosh cannot find or read takes half sizes.
pub(crate) async fn sizing(family: String) -> FontSizing {
    tauri::async_runtime::spawn_blocking(move || family_sizing(&family))
        .await
        .unwrap_or(FontSizing::SCALABLE)
}

/// The sizes the regular face of `family` draws at. macOS also asks
/// CoreText for the face's format, which names a bitmap face that does
/// not live in an sfnt file.
#[cfg(target_os = "macos")]
fn family_sizing(family: &str) -> FontSizing {
    use core_text::font_descriptor::kCTFontFormatBitmap;
    if let Some(sizing) = face_for_family(family)
        .ok()
        .and_then(|face| face_sizing(&face))
    {
        return sizing;
    }
    let bitmap = regular_descriptor(family)
        .and_then(|(descriptor, _)| descriptor.font_format())
        .is_some_and(|format| format == kCTFontFormatBitmap);
    if bitmap {
        FontSizing::bitmap(Vec::new())
    } else {
        FontSizing::SCALABLE
    }
}

/// The sizes the face of `family` the font scheme serves draws at.
#[cfg(not(target_os = "macos"))]
fn family_sizing(family: &str) -> FontSizing {
    face_for_family(family)
        .ok()
        .and_then(|face| face_sizing(&face))
        .unwrap_or(FontSizing::SCALABLE)
}

/// The sizes the single face in `face` draws at, from its sfnt tables:
/// any size for a face with `glyf`, `CFF ` or `CFF2` outlines, else its
/// strikes from `EBLC`, `CBLC`, `bloc` or `sbix`. A PCF or BDF file, the
/// bitmap formats Linux keeps, holds fixed sizes too, and Vosh reads no
/// strikes from them. None for a face it cannot judge.
fn face_sizing(face: &[u8]) -> Option<FontSizing> {
    // PCF, PCF packed with gzip, which is how Linux ships most of them,
    // and BDF.
    if face.starts_with(b"\x01fcp")
        || face.starts_with(&[0x1f, 0x8b])
        || face.starts_with(b"STARTFONT")
    {
        return Some(FontSizing::bitmap(Vec::new()));
    }
    let tables = sfnt_tables(face)?;
    let find = |tag: &[u8; 4]| tables.iter().find(|(t, _)| t == tag).map(|(_, body)| *body);
    if [b"glyf", b"CFF ", b"CFF2"]
        .iter()
        .any(|tag| find(tag).is_some())
    {
        return Some(FontSizing::SCALABLE);
    }
    if let Some(body) = [b"EBLC", b"CBLC", b"bloc"].iter().find_map(|tag| find(tag)) {
        return Some(FontSizing::bitmap(bitmap_location_strikes(body)));
    }
    find(b"sbix").map(|body| FontSizing::bitmap(sbix_strikes(body)))
}

/// The tables of the single face sfnt `face`, by tag. None for bytes that
/// are no sfnt face.
fn sfnt_tables(face: &[u8]) -> Option<Vec<([u8; 4], &[u8])>> {
    let magic = face.get(..4)?;
    if ![&[0, 1, 0, 0][..], b"true", b"OTTO", b"typ1"].contains(&magic) {
        return None;
    }
    let count = usize::from(u16::from_be_bytes(face.get(4..6)?.try_into().ok()?));
    (0..count)
        .map(|index| {
            let record = 12 + 16 * index;
            let tag: [u8; 4] = face.get(record..record + 4)?.try_into().ok()?;
            let offset = be32(face, record + 8)?;
            let len = be32(face, record + 12)?;
            Some((tag, face.get(offset..offset.checked_add(len)?)?))
        })
        .collect()
}

/// The pixel sizes an `EBLC`, `CBLC` or `bloc` table lists: a count at
/// byte 4, then a 48 byte record for each strike with its vertical pixels
/// per em at byte 45.
fn bitmap_location_strikes(table: &[u8]) -> Vec<u32> {
    let count = be32(table, 4).unwrap_or(0).min(table.len() / 48);
    (0..count)
        .filter_map(|index| table.get(8 + 48 * index + 45).copied())
        .filter(|&ppem| ppem > 0)
        .map(u32::from)
        .collect()
}

/// The pixel sizes an `sbix` table lists: a count at byte 4, then an
/// offset for each strike, which starts with its pixels per em.
fn sbix_strikes(table: &[u8]) -> Vec<u32> {
    let count = be32(table, 4).unwrap_or(0).min(table.len() / 4);
    (0..count)
        .filter_map(|index| {
            let at = be32(table, 8 + 4 * index)?;
            let ppem = table.get(at..at + 2)?;
            Some(u32::from(u16::from_be_bytes(ppem.try_into().ok()?)))
        })
        .filter(|&ppem| ppem > 0)
        .collect()
}

/// The font scheme handler. Serves the face [`face_for_family`] finds
/// for the family in the URL path, or 404 when there is none. It reads
/// the font file and asks CoreText about the face, so run it on the
/// blocking pool, never on the main thread. lib.rs registers it that
/// way.
pub(crate) fn handle_font_uri(uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let status = |code: StatusCode| Response::builder().status(code).body(Vec::new()).unwrap();
    let Some(family) = family_from_uri(uri) else {
        return status(StatusCode::NOT_FOUND);
    };
    match face_for_family(&family) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", font_mime(&bytes))
            .header("access-control-allow-origin", "*")
            .body(bytes)
            .unwrap(),
        Err(code) => status(code),
    }
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

    /// A single face sfnt with these tables, laid out after the
    /// directory in order.
    fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0, 1, 0, 0];
        out.extend_from_slice(&u16::try_from(tables.len()).unwrap().to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut offset = 12 + 16 * tables.len();
        let mut bodies = Vec::new();
        for (tag, body) in tables {
            out.extend_from_slice(&tag[..]);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes());
            out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_be_bytes());
            offset += body.len();
            bodies.extend_from_slice(body);
        }
        out.extend(bodies);
        out
    }

    /// An `EBLC` table with one strike for each of `ppems`.
    fn eblc(ppems: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 2, 0, 0];
        out.extend_from_slice(&u32::try_from(ppems.len()).unwrap().to_be_bytes());
        for &ppem in ppems {
            let mut record = [0u8; 48];
            record[44] = ppem;
            record[45] = ppem;
            out.extend_from_slice(&record);
        }
        out
    }

    /// An `sbix` table with one empty strike for each of `ppems`.
    fn sbix(ppems: &[u16]) -> Vec<u8> {
        let mut out = vec![0, 1, 0, 1];
        out.extend_from_slice(&u32::try_from(ppems.len()).unwrap().to_be_bytes());
        let first = 8 + 4 * ppems.len();
        for index in 0..ppems.len() {
            out.extend_from_slice(&u32::try_from(first + 4 * index).unwrap().to_be_bytes());
        }
        for &ppem in ppems {
            out.extend_from_slice(&ppem.to_be_bytes());
            out.extend_from_slice(&72u16.to_be_bytes());
        }
        out
    }

    #[test]
    fn a_face_with_outlines_takes_half_sizes() {
        for tag in [b"glyf", b"CFF ", b"CFF2"] {
            let face = sfnt(&[(b"head", vec![0; 54]), (tag, vec![0; 8])]);
            assert_eq!(face_sizing(&face), Some(FontSizing::SCALABLE), "{tag:?}");
        }
        // Outlines win over the bitmaps some faces carry for small sizes.
        let face = sfnt(&[(b"EBLC", eblc(&[12, 14])), (b"glyf", vec![0; 8])]);
        assert_eq!(face_sizing(&face), Some(FontSizing::SCALABLE));
    }

    #[test]
    fn a_bitmap_only_face_keeps_to_its_strikes() {
        let face = sfnt(&[(b"EBDT", vec![0; 4]), (b"EBLC", eblc(&[16, 12, 14, 12]))]);
        assert_eq!(
            face_sizing(&face),
            Some(FontSizing::bitmap(vec![12, 14, 16]))
        );
        let face = sfnt(&[(b"bdat", vec![0; 4]), (b"bloc", eblc(&[10]))]);
        assert_eq!(face_sizing(&face), Some(FontSizing::bitmap(vec![10])));
        let face = sfnt(&[(b"CBLC", eblc(&[109])), (b"CBDT", vec![0; 4])]);
        assert_eq!(face_sizing(&face), Some(FontSizing::bitmap(vec![109])));
        let face = sfnt(&[(b"sbix", sbix(&[20, 40]))]);
        assert_eq!(face_sizing(&face), Some(FontSizing::bitmap(vec![20, 40])));
        // A strike table that lists more than it holds keeps what it holds.
        let mut table = eblc(&[13]);
        table[7] = 9;
        let face = sfnt(&[(b"EBLC", table)]);
        assert_eq!(face_sizing(&face), Some(FontSizing::bitmap(vec![13])));
    }

    #[test]
    fn linux_bitmap_files_snap_to_whole_sizes() {
        for head in [
            &b"\x01fcp\x09\0\0\0"[..],
            &[0x1f, 0x8b, 8, 0][..],
            b"STARTFONT 2.1\n",
        ] {
            assert_eq!(face_sizing(head), Some(FontSizing::bitmap(Vec::new())));
        }
    }

    #[test]
    fn a_face_vosh_cannot_read_is_not_judged() {
        assert_eq!(face_sizing(b""), None);
        assert_eq!(face_sizing(b"wOF2 and the rest"), None);
        assert_eq!(face_sizing(&sfnt(&[(b"head", vec![0; 54])])), None);
        // A directory that points past the end of the file.
        let mut face = sfnt(&[(b"glyf", vec![0; 8])]);
        face.truncate(face.len() - 4);
        assert_eq!(face_sizing(&face), None);
    }

    #[test]
    fn the_bundled_jetbrains_mono_takes_half_sizes() {
        for file in [
            "JetBrainsMonoNerdFont-Regular.ttf",
            "JetBrainsMonoNerdFont-Bold.ttf",
        ] {
            let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../public/fonts");
            let face = std::fs::read(Path::new(dir).join(file)).expect("the repo holds it");
            assert_eq!(face_sizing(&face), Some(FontSizing::SCALABLE), "{file}");
        }
    }

    // Slow, since it reads every installed font file. Run it with
    // --ignored --nocapture to see which families snap to whole sizes.
    #[test]
    #[ignore = "reads every installed font file"]
    fn list_the_installed_families_that_snap() {
        let snapping: Vec<(String, FontSizing)> = enumerate_fonts()
            .into_iter()
            .map(|entry| {
                let sizing = family_sizing(&entry.family);
                (entry.family, sizing)
            })
            .filter(|(_, sizing)| !sizing.half_sizes)
            .collect();
        for (family, sizing) in &snapping {
            println!("{family}: {:?}", sizing.strikes);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_outline_fonts_take_half_sizes() {
        for family in ["Menlo", "Monaco", "Courier New", "No Such Family Vosh Test"] {
            assert_eq!(family_sizing(family), FontSizing::SCALABLE, "{family}");
        }
        // macOS ships one bitmap only family, with a single 16 px strike.
        if regular_descriptor("GB18030 Bitmap").is_some() {
            assert_eq!(
                family_sizing("GB18030 Bitmap"),
                FontSizing::bitmap(vec![16])
            );
        }
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
    // check or a macOS update. Both sides read the regular face.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "reads every installed font file"]
    fn core_text_agrees_with_font_kit_on_every_family() {
        use font_kit::family_name::FamilyName;
        use font_kit::properties::Properties;
        let source = SystemSource::new();
        let families = source.all_families().expect("font-kit lists the families");
        let font_kit_regular = |family: &str| {
            source
                .select_best_match(&[FamilyName::Title(family.to_string())], &Properties::new())
                .ok()
                .and_then(|handle| handle.load().ok())
                .is_some_and(|font| font.is_monospace())
        };
        let differ: Vec<(&String, bool)> = families
            .iter()
            .map(|family| (family, is_family_monospace(family)))
            .filter(|(family, mono)| *mono != font_kit_regular(family))
            .collect();
        assert!(
            differ.is_empty(),
            "CoreText and font-kit disagree on {differ:?}"
        );
    }

    fn face(italic: bool, weight: f64, width: f64, mono: bool) -> FaceTraits {
        FaceTraits {
            symbolic: i64::from(italic) | if mono { 1 << 10 } else { 0 },
            weight,
            width,
        }
    }

    #[test]
    fn the_regular_face_is_upright_with_normal_weight_and_width() {
        // A family that lists Italic first, where only the Regular face
        // says it is monospace.
        let faces = [
            face(true, 0.0, 0.0, false),
            face(false, 0.4, 0.0, false),
            face(false, 0.0, 0.0, true),
            face(true, 0.4, 0.0, false),
        ];
        assert_eq!(regular_face(&faces), Some(2));
        // Nearest normal width among upright regular weight faces.
        let condensed = [face(false, 0.0, -0.2, false), face(false, 0.0, 0.0, true)];
        assert_eq!(regular_face(&condensed), Some(1));
        // A family of italics only takes the nearest regular weight.
        let italics = [face(true, 0.3, 0.0, false), face(true, -0.1, 0.0, true)];
        assert_eq!(regular_face(&italics), Some(1));
        // Equal faces keep the first listed.
        let same = [face(false, 0.0, 0.0, true), face(false, 0.0, 0.0, false)];
        assert_eq!(regular_face(&same), Some(0));
        assert_eq!(regular_face(&[]), None);
    }

    #[test]
    fn the_list_reads_off_the_main_thread() {
        let first = tauri::async_runtime::block_on(list());
        let again = tauri::async_runtime::block_on(list());
        assert!(same(&first, &enumerate_fonts()));
        assert!(same(&first, &again));
    }

    #[test]
    fn the_family_comes_from_the_url_path_on_every_platform() {
        let family = |url: &str| family_from_uri(&url.parse().unwrap());
        // convertFileSrc on macOS and Linux, then on Windows.
        assert_eq!(
            family("font://localhost/Fira%20Code").as_deref(),
            Some("Fira Code")
        );
        assert_eq!(
            family("http://font.localhost/Fira%20Code").as_deref(),
            Some("Fira Code")
        );
        let encoded = urlencoding::encode("游ゴシック");
        assert_eq!(
            family(&format!("https://font.localhost/{encoded}")).as_deref(),
            Some("游ゴシック")
        );
        // The family in the host, the URL fontLoader.ts built before,
        // names nothing.
        assert_eq!(family("font://Menlo"), None);
        assert_eq!(family("font://localhost/"), None);
        assert_eq!(family("font://localhost/%FF"), None);
    }

    /// A font collection of `faces`, each a face with one table holding
    /// `body`, laid out the way real collections are: the header, then
    /// every table directory, then the tables.
    fn collection(faces: &[(&[u8; 4], &[u8; 4], &[u8])]) -> Vec<u8> {
        let be = |n: usize| u32::try_from(n).unwrap().to_be_bytes();
        let header = 12 + 4 * faces.len();
        let directory = 12 + 16;
        let mut table_at = header + directory * faces.len();
        let mut out = b"ttcf".to_vec();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&be(faces.len()));
        for i in 0..faces.len() {
            out.extend_from_slice(&be(header + directory * i));
        }
        let mut bodies = Vec::new();
        for (version, tag, body) in faces {
            out.extend_from_slice(*version);
            out.extend_from_slice(&1u16.to_be_bytes());
            out.extend_from_slice(&[0; 6]);
            out.extend_from_slice(*tag);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&be(table_at));
            out.extend_from_slice(&be(body.len()));
            table_at += body.len();
            bodies.extend_from_slice(body);
        }
        out.extend_from_slice(&bodies);
        out
    }

    #[test]
    fn a_collection_face_comes_out_as_a_font_of_its_own() {
        let ttc = collection(&[
            (&[0, 1, 0, 0], b"bold", b"BOLD"),
            (b"OTTO", b"regu", b"REGULAR!"),
        ]);
        let face = face_bytes(ttc.clone(), 1).expect("the second face");
        // The second face's directory now opens the file, and its table
        // offset still finds its table.
        assert_eq!(&face[..4], b"OTTO");
        assert_eq!(u16::from_be_bytes([face[4], face[5]]), 1);
        assert_eq!(&face[12..16], b"regu");
        let at = |range: std::ops::Range<usize>| {
            u32::from_be_bytes(face[range].try_into().unwrap()) as usize
        };
        let (offset, length) = (at(20..24), at(24..28));
        assert_eq!(&face[offset..offset + length], b"REGULAR!");
        assert_eq!(font_mime(&face), "font/otf");

        let first = face_bytes(ttc.clone(), 0).expect("the first face");
        assert_eq!(&first[12..16], b"bold");
        assert_eq!(font_mime(&first), "font/ttf");
        assert!(face_bytes(ttc.clone(), 2).is_none());

        // A table inside the bytes the directory would land on.
        let mut overlapping = ttc.clone();
        overlapping[40..44].copy_from_slice(&4u32.to_be_bytes());
        assert!(face_bytes(overlapping, 0).is_none());
        // A directory that runs past the end of the file.
        assert!(face_bytes(ttc[..40].to_vec(), 0).is_none());
        assert!(face_bytes(b"ttcf".to_vec(), 0).is_none());
    }

    #[test]
    fn a_collection_face_is_found_by_the_bytes_of_one_of_its_tables() {
        let ttc = collection(&[
            (&[0, 1, 0, 0], b"name", b"BOLD"),
            (b"OTTO", b"name", b"REGULAR!"),
            (b"OTTO", b"head", b"ITALIC"),
            (b"OTTO", b"name", b"REGULAR!"),
        ]);
        assert_eq!(face_with_table(&ttc, *b"name", b"REGULAR!"), Some(1));
        assert_eq!(face_with_table(&ttc, *b"name", b"BOLD"), Some(0));
        // The tag counts, not only the bytes.
        assert_eq!(face_with_table(&ttc, *b"name", b"ITALIC"), None);
        assert_eq!(face_with_table(&ttc, *b"head", b"ITALIC"), Some(2));
        // A table that runs past the end of the file matches nothing.
        let cut = &ttc[..ttc.len() - 1];
        assert_eq!(face_with_table(cut, *b"name", b"REGULAR!"), Some(1));
        assert_eq!(face_with_table(cut, *b"head", b"ITALIC"), Some(2));
        // The bodies sit in face order at the end, so this cut ends one
        // byte into the body of the second face.
        let short = &ttc[..ttc.len() - b"ITALIC".len() - b"REGULAR!".len() - 1];
        assert_eq!(face_with_table(short, *b"name", b"REGULAR!"), None);
        // A face count far past what the file holds ends the search.
        let mut huge = ttc.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(face_with_table(&huge, *b"name", b"ITALIC"), None);
        // A file of one face is no collection.
        assert_eq!(
            face_with_table(b"OTTO\x00\x01rest", *b"name", b"rest"),
            None
        );
    }

    #[test]
    fn a_single_face_file_is_served_whole() {
        let font = b"OTTO\x00\x01rest".to_vec();
        assert_eq!(face_bytes(font.clone(), 0), Some(font.clone()));
        assert!(face_bytes(font, 1).is_none());
        assert_eq!(font_mime(b"wOF2...."), "font/woff2");
        assert_eq!(font_mime(b"wOFF...."), "font/woff");
        assert_eq!(font_mime(&[0, 1, 0, 0]), "font/ttf");
    }

    /// The PostScript name of the face `descriptor` names, or None when
    /// CoreText gives it none.
    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    fn postscript_name(
        descriptor: &core_text::font_descriptor::CTFontDescriptor,
    ) -> Option<String> {
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::string::{CFString, CFStringRef};
        use core_text::font_descriptor::{kCTFontNameAttribute, CTFontDescriptorCopyAttribute};
        // SAFETY: the descriptor is live and the key is CoreText's own
        // constant. The copy comes back retained, or null, and the create
        // rule wrap releases it.
        let value = unsafe {
            let raw = CTFontDescriptorCopyAttribute(
                descriptor.as_concrete_TypeRef(),
                kCTFontNameAttribute,
            );
            if raw.is_null() {
                return None;
            }
            CFType::wrap_under_create_rule(raw)
        };
        if !value.instance_of::<CFString>() {
            return None;
        }
        // SAFETY: the value is a CFString, checked above, and the get rule
        // wrap retains it for as long as the string lives.
        let name = unsafe { CFString::wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef) };
        Some(name.to_string())
    }

    /// The PostScript names CoreText reads from font bytes in memory,
    /// the way `WebKit` loads a web font: one name for a face, one per
    /// named instance for a variable face, none for bytes CoreText
    /// cannot read.
    #[cfg(target_os = "macos")]
    #[allow(unsafe_code)]
    fn names_in(bytes: &[u8]) -> Vec<String> {
        use core_foundation::array::{CFArray, CFArrayRef};
        use core_foundation::base::TCFType;
        use core_foundation::data::{CFData, CFDataRef};
        use core_text::font_descriptor::CTFontDescriptor;
        #[link(name = "CoreText", kind = "framework")]
        extern "C" {
            fn CTFontManagerCreateFontDescriptorsFromData(data: CFDataRef) -> CFArrayRef;
        }
        let data = CFData::from_buffer(bytes);
        // SAFETY: the data is live. The array comes back retained, or
        // null for bytes CoreText cannot read, and the create rule wrap
        // releases it. CoreText fills it with font descriptors.
        let faces: CFArray<CTFontDescriptor> = unsafe {
            let raw = CTFontManagerCreateFontDescriptorsFromData(data.as_concrete_TypeRef());
            if raw.is_null() {
                return Vec::new();
            }
            CFArray::wrap_under_create_rule(raw)
        };
        faces
            .iter()
            .filter_map(|face| postscript_name(&face))
            .collect()
    }

    /// The names CoreText reads from the bytes the font scheme serves
    /// for `family`, or None when the scheme answers anything but 200.
    #[cfg(target_os = "macos")]
    fn served_names(family: &str) -> Option<Vec<String>> {
        let url = format!("font://localhost/{}", urlencoding::encode(family));
        let response = handle_font_uri(&url.parse().unwrap());
        (response.status() == StatusCode::OK).then(|| names_in(response.body()))
    }

    /// The PostScript name of the regular face of `family`, and whether
    /// CoreText reads its whole font file from memory. On macOS 27 it
    /// reads `PingFangUI.ttc`, the file of `PingFang SC`, only from disk,
    /// so no web font can use it. None for an unknown family.
    #[cfg(target_os = "macos")]
    fn regular_and_readable(family: &str) -> Option<(String, bool)> {
        let (descriptor, _) = regular_descriptor(family)?;
        let regular = postscript_name(&descriptor)?;
        let file = std::fs::read(descriptor.font_path()?).ok()?;
        Some((regular, !names_in(&file).is_empty()))
    }

    /// Whether the font scheme answers right for `family`: the bytes of
    /// its regular face, or 404 when CoreText cannot read its font file
    /// from memory.
    #[cfg(target_os = "macos")]
    fn served_right(family: &str) -> bool {
        let Some((regular, readable)) = regular_and_readable(family) else {
            return false;
        };
        match served_names(family) {
            Some(names) => readable && names.contains(&regular),
            None => !readable,
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_font_scheme_serves_the_regular_face_of_a_family() {
        // Menlo and Avenir Next live in collections, and Avenir Next
        // lists Bold first. Geneva is a file of one face. Skia and Noto
        // Sans Syriac are variable fonts of one face, which CoreText
        // lists once per named instance, Regular not first.
        for (family, postscript) in [
            ("Menlo", "Menlo-Regular"),
            ("Avenir Next", "AvenirNext-Regular"),
            ("Geneva", "Geneva"),
            ("Skia", "Skia-Regular"),
            ("Noto Sans Syriac", "NotoSansSyriac-Regular"),
        ] {
            let names = served_names(family).unwrap_or_else(|| panic!("{family} is served"));
            assert!(
                names.iter().any(|name| name == postscript),
                "{family} serves {names:?}"
            );
        }
        // PingFang SC is a face of a variable collection, which the
        // scheme serves where CoreText reads it from memory.
        assert!(
            served_right("PingFang SC"),
            "PingFang SC serves {:?}",
            served_names("PingFang SC")
        );
        let missing = "font://localhost/No%20Such%20Family%20Vosh%20Test";
        assert_eq!(
            handle_font_uri(&missing.parse().unwrap()).status(),
            StatusCode::NOT_FOUND
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(unsafe_code)]
    fn every_descriptor_of_a_collection_finds_its_own_face() {
        use core_foundation::array::CFArray;
        use core_foundation::base::TCFType;
        use core_foundation::url::CFURL;
        use core_text::font_descriptor::CTFontDescriptor;
        use core_text::font_manager::CTFontManagerCreateFontDescriptorsFromURL;
        // ThonburiUI is a variable collection of two faces that CoreText
        // lists as six named instances. Avenir Next lists Bold first.
        for file in [
            "/System/Library/Fonts/ThonburiUI.ttc",
            "/System/Library/Fonts/Avenir Next.ttc",
        ] {
            let path = Path::new(file);
            let Ok(data) = std::fs::read(path) else {
                continue;
            };
            let url = CFURL::from_path(path, false).expect("a file URL");
            // SAFETY: the URL is live. The array comes back retained, or
            // null for a file CoreText cannot read, and the create rule
            // wrap releases it. CoreText fills it with font descriptors.
            let descriptors: CFArray<CTFontDescriptor> = unsafe {
                let raw = CTFontManagerCreateFontDescriptorsFromURL(url.as_concrete_TypeRef());
                assert!(!raw.is_null(), "CoreText reads {file}");
                CFArray::wrap_under_create_rule(raw)
            };
            assert!(descriptors.len() > 1, "{file} is a collection");
            for descriptor in descriptors.iter() {
                let name = postscript_name(&descriptor).expect("a PostScript name");
                let face = collection_face(&descriptor, &data)
                    .and_then(|index| face_bytes(data.clone(), index))
                    .unwrap_or_else(|| panic!("{name} finds a face in {file}"));
                let names = names_in(&face);
                assert!(names.contains(&name), "{name} cut out as {names:?}");
            }
        }
    }

    // Slow, since it reads the font file of every family twice, and
    // PingFang alone is 60 MB. Run it with --ignored after a change to the font
    // scheme or a macOS update.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "reads every installed font file"]
    fn the_font_scheme_serves_the_regular_face_of_every_family() {
        let wrong: Vec<(String, Option<Vec<String>>)> = enumerate_fonts()
            .into_iter()
            .filter(|entry| !served_right(&entry.family))
            .map(|entry| {
                let names = served_names(&entry.family);
                (entry.family, names)
            })
            .collect();
        assert!(wrong.is_empty(), "wrong or no face for {wrong:?}");
    }
}
