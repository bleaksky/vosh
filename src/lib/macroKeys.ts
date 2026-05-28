// Canonical key-string format for macro bindings. The frontend
// converts a KeyboardEvent into a stable string like "F1",
// "Ctrl+N", "Shift+F5", "Ctrl+Alt+Numpad7" so the backend (and
// any settings UI) compares by string equality without caring
// about browser/OS quirks.
//
// Format rules:
//   - Modifier order: Ctrl, Alt, Shift, Meta (cmd/win), joined by "+"
//   - Letter keys are uppercase ("A".."Z")
//   - Number keys use their digit ("0".."9")
//   - Function keys keep their browser name ("F1".."F24")
//   - Navigation keys: ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
//     Home, End, PageUp, PageDown, Insert, Delete, Tab, Enter, Escape
//   - Numpad keys: Numpad0..Numpad9, NumpadAdd, NumpadSubtract,
//     NumpadMultiply, NumpadDivide, NumpadDecimal, NumpadEnter
//   - Other punctuation: a single-char `key` ("/", ";", "=")

export interface CanonicalKeyOptions {
  // When true, plain printable keys (no modifier) are still
  // returned. Default false — the input field needs to type
  // text without firing macros every keystroke.
  allowPlainPrintable?: boolean;
}

// Convert a DOM KeyboardEvent into the canonical macro-key string
// (or null if the event represents a modifier-only keypress, which
// we never want to bind on its own).
export function canonicalKeyFromEvent(
  event: KeyboardEvent | React.KeyboardEvent,
  opts: CanonicalKeyOptions = {},
): string | null {
  const native = (event as KeyboardEvent).key
    ? (event as KeyboardEvent)
    : (event as unknown as { nativeEvent: KeyboardEvent }).nativeEvent;
  const keyCode = native.code;
  const keyName = native.key;

  // Reject pure modifier presses.
  if (keyName === 'Control' || keyName === 'Shift' || keyName === 'Alt' || keyName === 'Meta') {
    return null;
  }

  const base = canonicalBase(keyCode, keyName);
  if (!base) return null;

  const hasModifier = native.ctrlKey || native.altKey || native.metaKey;
  // Plain printable letters/digits without a modifier are typing,
  // not a macro press. The Settings tab passes allowPlainPrintable
  // when capturing a binding so the user CAN bind to "a" if they
  // really want to.
  if (!hasModifier && isPrintableSingleChar(base) && !opts.allowPlainPrintable) {
    return null;
  }

  const parts: string[] = [];
  if (native.ctrlKey) parts.push('Ctrl');
  if (native.altKey) parts.push('Alt');
  if (native.shiftKey) parts.push('Shift');
  if (native.metaKey) parts.push('Meta');
  parts.push(base);
  return parts.join('+');
}

// Map the raw event to the base token (everything after the
// modifier prefix). Returns null for keys we do not bind (e.g.
// dead/compose keys, unidentified).
function canonicalBase(code: string, key: string): string | null {
  // Function keys -- "F1" through "F24"
  if (/^F([1-9]|1\d|2[0-4])$/.test(key)) return key;

  // Numpad keys use `code` (not `key`) because NumLock changes
  // the `key` from "Numpad1" to "End" etc.
  if (code.startsWith('Numpad')) {
    // "Numpad1" -> "Numpad1", "NumpadAdd" -> "NumpadAdd"
    return code;
  }

  // Navigation block
  const navByKey: Record<string, string> = {
    ArrowUp: 'ArrowUp',
    ArrowDown: 'ArrowDown',
    ArrowLeft: 'ArrowLeft',
    ArrowRight: 'ArrowRight',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    Insert: 'Insert',
    Delete: 'Delete',
    Tab: 'Tab',
    Enter: 'Enter',
    Escape: 'Escape',
    Backspace: 'Backspace',
  };
  if (navByKey[key]) return navByKey[key];

  // Single-char letter -> uppercase.
  if (key.length === 1) {
    const upper = key.toUpperCase();
    if (/^[A-Z]$/.test(upper)) return upper;
    if (/^[0-9]$/.test(upper)) return upper;
    // Other printable punctuation kept as-is (",", ".", "/", etc.)
    return upper;
  }

  // Anything we do not recognize (Dead, Unidentified, Compose...)
  return null;
}

function isPrintableSingleChar(base: string): boolean {
  return base.length === 1;
}

// Human-friendly label for displaying a canonical key in the UI.
// Just returns the canonical form for now; could expand to render
// platform-specific symbols (⌘ for Meta on macOS) later.
export function labelForKey(canonical: string): string {
  return canonical;
}
