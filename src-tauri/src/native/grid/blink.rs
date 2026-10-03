//! Blinking text. The grid keeps the blink of SGR 5, which alacritty
//! drops, as a flag on each cell.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::cursor_icon::CursorIcon;
use alacritty_terminal::vte::ansi::{
    Attr, CharsetIndex, ClearMode, CursorShape, CursorStyle, Handler, Hyperlink as LinkSpec,
    KeyboardModes, KeyboardModesApplyBehavior, LineClearMode, Mode, ModifyOtherKeys, PrivateMode,
    Rgb, StandardCharset, TabulationClearMode,
};

use super::NoopListener;

/// The blink of SGR 5 as a mark on the cell. `alacritty_terminal` reads
/// SGR 5 and 25 and drops them, and leaves the top bit of its `Flags`
/// free, so the grid keeps blink there. A cell written while the
/// cursor's template holds the bit carries it through scrolling, reflow
/// and a saved cursor, and a reset clears it with every other style.
/// The rapid blink of SGR 6 draws steady, as xterm draws it.
pub(crate) const BLINK: Flags = Flags::from_bits_retain(1 << 15);

/// The terminal, with the blink it drops kept on the cursor's template.
/// alacritty's parser hands SGR 5 and 25 to its handler as `BlinkSlow`
/// and `CancelBlink`, in the order the parameters come, so a 5 inside a
/// color stays part of the color and a reset in the same sequence clears
/// what came before it. Every other call goes to the terminal as it
/// comes. On an update of `alacritty_terminal`, check its `Handler` for
/// new methods, since one missing here falls to the trait's empty
/// default instead of the terminal.
pub(super) struct Blinking<'a>(pub(super) &'a mut Term<NoopListener>);

/// Hand each listed `Handler` method to the terminal.
macro_rules! to_term {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(
            #[inline]
            fn $name(&mut self, $($arg: $ty),*) {
                Handler::$name(&mut *self.0, $($arg),*);
            }
        )*
    };
}

impl Handler for Blinking<'_> {
    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        match attr {
            Attr::BlinkSlow => self.0.grid_mut().cursor.template.flags.insert(BLINK),
            Attr::CancelBlink => self.0.grid_mut().cursor.template.flags.remove(BLINK),
            attr => Handler::terminal_attribute(&mut *self.0, attr),
        }
    }

    to_term! {
        set_title(title: Option<String>);
        set_cursor_style(style: Option<CursorStyle>);
        set_cursor_shape(shape: CursorShape);
        input(c: char);
        goto(line: i32, col: usize);
        goto_line(line: i32);
        goto_col(col: usize);
        insert_blank(count: usize);
        move_up(rows: usize);
        move_down(rows: usize);
        identify_terminal(intermediate: Option<char>);
        device_status(arg: usize);
        move_forward(col: usize);
        move_backward(col: usize);
        move_down_and_cr(row: usize);
        move_up_and_cr(row: usize);
        put_tab(count: u16);
        backspace();
        carriage_return();
        linefeed();
        bell();
        substitute();
        newline();
        set_horizontal_tabstop();
        scroll_up(rows: usize);
        scroll_down(rows: usize);
        insert_blank_lines(rows: usize);
        delete_lines(rows: usize);
        erase_chars(count: usize);
        delete_chars(count: usize);
        move_backward_tabs(count: u16);
        move_forward_tabs(count: u16);
        save_cursor_position();
        restore_cursor_position();
        clear_line(mode: LineClearMode);
        clear_screen(mode: ClearMode);
        clear_tabs(mode: TabulationClearMode);
        reset_state();
        reverse_index();
        set_mode(mode: Mode);
        unset_mode(mode: Mode);
        report_mode(mode: Mode);
        set_private_mode(mode: PrivateMode);
        unset_private_mode(mode: PrivateMode);
        report_private_mode(mode: PrivateMode);
        set_scrolling_region(top: usize, bottom: Option<usize>);
        set_keypad_application_mode();
        unset_keypad_application_mode();
        set_active_charset(index: CharsetIndex);
        configure_charset(index: CharsetIndex, charset: StandardCharset);
        set_color(index: usize, color: Rgb);
        dynamic_color_sequence(prefix: String, index: usize, terminator: &str);
        reset_color(index: usize);
        clipboard_store(clipboard: u8, base64: &[u8]);
        clipboard_load(clipboard: u8, terminator: &str);
        decaln();
        push_title();
        pop_title();
        text_area_size_pixels();
        text_area_size_chars();
        set_hyperlink(link: Option<LinkSpec>);
        set_mouse_cursor_icon(icon: CursorIcon);
        report_keyboard_mode();
        push_keyboard_mode(mode: KeyboardModes);
        pop_keyboard_modes(to_pop: u16);
        set_keyboard_mode(mode: KeyboardModes, behavior: KeyboardModesApplyBehavior);
        set_modify_other_keys(mode: ModifyOtherKeys);
        report_modify_other_keys();
    }
}
