//! The commands the page sends the native terminal surface. A build
//! without the surface keeps every one, so the page calls them the same
//! way on every platform, and there each one does nothing. A command that
//! reads or changes what a grid holds names the session whose grid it
//! acts on, or acts on the selected session's when it names none. The
//! pointer and the wheel act on the grid that shows.

use tauri::{AppHandle, State};

use crate::app::state::SharedState;
use crate::sessions::SessionId;

/// Native renderer (macOS). The frontend reports the terminal
/// pane's screen rectangle (CSS pixels, top-left origin, relative to the
/// window) and device pixel ratio so the native wgpu surface can track
/// it. NSView/Metal must be touched on the main thread, so the work is
/// dispatched there. A no-op on other platforms and when the surface is
/// not installed. `lent` is the rows at the pane's bottom the pinned
/// prompt band borrows while your prompt takes more than one row: the
/// grid gives them up, and the game keeps the size it was told. A page
/// that sends none lends none.
#[tauri::command]
pub(crate) fn native_surface_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    dpr: f64,
    lent: Option<u32>,
) {
    #[cfg(native_surface)]
    {
        let lent = lent.unwrap_or(0);
        let _ = app.run_on_main_thread(move || {
            crate::native::surface::set_bounds(x, y, width, height, dpr, lent);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, x, y, width, height, dpr, lent);
    }
}

/// Native renderer, underlay mode (macOS). The webview sits above
/// the surface and receives every click, so the page forwards pointer
/// events over the terminal here. `x` and `y` are CSS px from the pane's
/// top-left corner. `kind` is "down", "drag", "up", "move", "leave", or
/// "middle". `open` carries the Cmd modifier for opening links. The work
/// runs on the main thread, which the renderer requires. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_pointer(app: AppHandle, kind: String, x: f64, y: f64, open: bool) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native::surface::pointer::forward_pointer(&kind, x, y, open);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, kind, x, y, open);
    }
}

/// Native renderer: true once the surface installed and its GPU came
/// up. The page leaves the terminal pane transparent only after this, so a
/// failed install falls back to xterm. False elsewhere.
#[tauri::command]
pub(crate) fn native_surface_ready() -> bool {
    #[cfg(native_surface)]
    {
        crate::native::surface::is_ready()
    }
    #[cfg(not(native_surface))]
    {
        false
    }
}

/// Native renderer, underlay mode (macOS): a wheel delta forwarded
/// from the page. Positive reveals older lines. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_wheel(app: AppHandle, delta_y: f64) {
    #[cfg(native_surface)]
    {
        let _ = app.run_on_main_thread(move || {
            crate::native::surface::pointer::forward_wheel(delta_y);
        });
    }
    #[cfg(not(native_surface))]
    {
        let _ = (&app, delta_y);
    }
}

/// Native renderer (macOS): copy the current selection to the
/// clipboard. Used by the Cmd+C / Ctrl+C path; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_copy(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    #[cfg(native_surface)]
    crate::native::surface::pointer::request_copy(session.id);
    #[cfg(not(native_surface))]
    let _ = session;
    Ok(())
}

/// Native renderer: select everything in the grid, scrollback
/// included, for the terminal menu's Select all and Cmd+A on an empty
/// command line. Repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_select_all(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    #[cfg(native_surface)]
    {
        crate::native::grid::select_all(session.id);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    let _ = session;
    Ok(())
}

/// Parse a `#rrggbb` (or `rrggbb`) hex color.
#[cfg(native_surface)]
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() < 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ))
}

/// Native renderer (macOS): set the surface theme colors so the
/// background, foreground, and selection follow the active Vosh theme.
/// Colors are `#rrggbb`. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_theme(
    background: String,
    foreground: String,
    selection: String,
    ansi: Vec<String>,
) {
    #[cfg(native_surface)]
    {
        if let (Some(bg), Some(fg), Some(sel)) = (
            parse_hex(&background),
            parse_hex(&foreground),
            parse_hex(&selection),
        ) {
            crate::native::gpu::style::set_theme(bg, fg, sel);
            let palette: Vec<(u8, u8, u8)> = ansi.iter().filter_map(|s| parse_hex(s)).collect();
            if palette.len() == 16 {
                crate::native::gpu::style::set_palette(&palette);
            }
            crate::native::surface::request_redraw();
        }
    }
    #[cfg(not(native_surface))]
    {
        let _ = (background, foreground, selection, ansi);
    }
}

/// Native renderer: apply the split divider color setting to the
/// surface renderer (hex or `rgb()`/`rgba()`; None restores the default).
#[tauri::command]
pub(crate) fn native_surface_set_divider_color(color: Option<String>) {
    #[cfg(native_surface)]
    {
        let parsed = color.as_deref().and_then(crate::color::parse_css_color);
        crate::native::gpu::style::set_divider_color(parsed);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = color;
    }
}

/// Native renderer: the chrome colors the page derives with its
/// theme tokens, as CSS colors (hex, or `rgb()`/`rgba()` with alpha). The
/// split divider, the selection and its text, every find match, the
/// current match, a hovered link, the scrollbar thumb, and the selected
/// row fill a lifted prompt's band takes. Without a selection text a
/// selected cell keeps its own color. `appearance` is the theme's, and a
/// light one gives the band its inset ring. Each call replaces the whole
/// set, and a missing or unreadable color falls back to one derived from
/// the terminal palette. The divider setting still wins over `divider`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn native_surface_set_tokens(
    divider: Option<String>,
    selection: Option<String>,
    selection_text: Option<String>,
    find_match: Option<String>,
    current_match: Option<String>,
    link: Option<String>,
    scrollbar: Option<String>,
    selrow: Option<String>,
    appearance: Option<String>,
) {
    #[cfg(native_surface)]
    {
        let parse = |v: Option<String>| v.as_deref().and_then(crate::color::parse_css_color);
        crate::native::gpu::style::set_tokens(crate::native::gpu::style::ChromeTokens {
            divider: parse(divider),
            selection: parse(selection),
            selection_text: parse(selection_text),
            find_match: parse(find_match),
            current_match: parse(current_match),
            link: parse(link),
            scrollbar: parse(scrollbar),
            selrow: parse(selrow),
            light: appearance.as_deref() == Some("light"),
        });
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = (
            divider,
            selection,
            selection_text,
            find_match,
            current_match,
            link,
            scrollbar,
            selrow,
            appearance,
        );
    }
}

/// Native renderer: draw a band under each lifted prompt of the
/// session while your prompt shows lifted there. The grid tags a lift's
/// cells either way.
#[tauri::command]
pub(crate) fn native_surface_set_prompt_bands(
    state: State<'_, SharedState>,
    on: bool,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    #[cfg(native_surface)]
    {
        crate::native::grid::set_prompt_bands(session.id, on);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = (on, session);
    }
    Ok(())
}

/// Native renderer: widen the band under the open row by `px` CSS
/// px, so it holds the prompt card's line break mark and caret past the
/// row's last glyph. 0 while the card is closed.
#[tauri::command]
pub(crate) fn native_surface_set_prompt_reach(px: f64) {
    #[cfg(native_surface)]
    {
        #[allow(clippy::cast_possible_truncation)]
        crate::native::gpu::bands::set_prompt_reach(px as f32);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = px;
    }
}

/// Native renderer (macOS): toggle drawing bright (ANSI 8-15) colored
/// text with the bold font weight. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_bright_bold(on: bool) {
    #[cfg(native_surface)]
    {
        crate::native::gpu::style::set_bright_bold(on);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Native renderer: turn blinking text on or off, from the Blinking
/// text setting and the system's reduce motion setting. Off, every
/// blinking cell draws steady. A no-op without the native surface.
#[tauri::command]
pub(crate) fn native_surface_set_blink_text(on: bool) {
    #[cfg(native_surface)]
    {
        crate::native::gpu::style::set_blink_text(on);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = on;
    }
}

/// Native renderer (macOS): report xterm's device cell size so the
/// surface grid matches the webview's spacing exactly instead of deriving it
/// from font metrics. `char_height` is xterm's device glyph box, which it
/// centers in a cell taller than the box, so the surface can put its
/// baseline in the same place at every line height. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_cell_metrics(width: u32, height: u32, char_height: Option<u32>) {
    #[cfg(native_surface)]
    crate::native::surface::device::set_cell_metrics(width, height, char_height.unwrap_or(0));
    #[cfg(not(native_surface))]
    {
        let _ = (width, height, char_height);
    }
}

/// Native renderer (macOS): search the grid and step to the next (or
/// previous) match, scrolling it into view and highlighting all matches.
/// Returns `[current, total]` (1-based; `[0, 0]` when no match). A no-op
/// returning `[0, 0]` elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find(
    state: State<'_, SharedState>,
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    forward: bool,
    session: Option<SessionId>,
) -> Result<(usize, usize), String> {
    let session = state.session(session)?;
    #[cfg(native_surface)]
    {
        let result = crate::native::grid::find::find_run(
            session.id,
            &query,
            regex,
            case_sensitive,
            whole_word,
            forward,
        );
        crate::native::surface::request_redraw();
        Ok(result)
    }
    #[cfg(not(native_surface))]
    {
        let _ = (session, query, regex, case_sensitive, whole_word, forward);
        Ok((0, 0))
    }
}

/// Native renderer (macOS): clear the find highlight. A no-op
/// elsewhere.
#[tauri::command]
pub(crate) fn native_surface_find_clear(
    state: State<'_, SharedState>,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?;
    #[cfg(native_surface)]
    {
        crate::native::grid::find::find_clear(session.id);
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    let _ = session;
    Ok(())
}

/// Native renderer (macOS): rebuild the surface atlas at a new font
/// list and size (CSS px) so it matches the configured Vosh font. `family`
/// is the CSS font list xterm draws with. A no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_set_font(family: String, size: u32) {
    #[cfg(native_surface)]
    crate::native::surface::device::request_set_font(family, size);
    #[cfg(not(native_surface))]
    {
        let _ = (family, size);
    }
}

/// Native renderer (macOS): keyboard scroll. `kind` is "pageup",
/// "pagedown", "bottom", or "toggle". Toggle opens or closes the split
/// the way a middle click does: scrolled back it snaps to the live
/// tail, at the tail it pages up into scrollback. Scrolls the grid and
/// repaints; a no-op elsewhere.
#[tauri::command]
pub(crate) fn native_surface_scroll(
    state: State<'_, SharedState>,
    kind: String,
    session: Option<SessionId>,
) -> Result<(), String> {
    let session = state.session(session)?.id;
    #[cfg(native_surface)]
    {
        use crate::native::grid::{scroll_metrics, scroll_page, scroll_to_bottom};
        use crate::native::surface::pointer::split_ratio;
        match kind.as_str() {
            "pageup" => scroll_page(session, true, split_ratio()),
            "pagedown" => scroll_page(session, false, split_ratio()),
            "bottom" => scroll_to_bottom(session),
            "toggle" => {
                let (offset, _) = scroll_metrics(session);
                if offset > 0 {
                    scroll_to_bottom(session);
                } else {
                    scroll_page(session, true, split_ratio());
                }
            }
            _ => {}
        }
        crate::native::surface::request_redraw();
    }
    #[cfg(not(native_surface))]
    {
        let _ = (kind, session);
    }
    Ok(())
}
