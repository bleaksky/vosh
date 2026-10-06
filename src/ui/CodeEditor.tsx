import { useEffect, useMemo, useState, type CSSProperties } from 'react';
import CodeMirror, { type Extension } from '@uiw/react-codemirror';
import { EditorView } from '@codemirror/view';
import { StreamLanguage, syntaxHighlighting } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import {
  codeEditorAttributes,
  codeHighlightStyle,
  marksUpdate,
  type CodeMark,
} from './codeEditorStyle';

interface Props {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  /** Compact inline mode — hides line numbers, fold gutter, active-line
   *  highlight, and the side scroll bar. Used for the alias / trigger
   *  row editors where the editor needs to feel like an upgraded text
   *  input, not a full IDE pane. */
  inline?: boolean;
  /** Minimum render height. The editor grows past this as the user adds
   *  lines, up to `maxHeight`. Default fits one line of body text. */
  minHeight?: string;
  /** Maximum render height before the editor turns into a scroll view.
   *  Default 300px keeps a long script from pushing the list around. */
  maxHeight?: string;
  /** Fill the parent container's height instead of sizing by content.
   *  Use for full-pane editors (the JSON store tab) where the parent
   *  already has a defined height via flex layout. `minHeight` and
   *  `maxHeight` are ignored in fill mode. */
  fill?: boolean;
  /** Syntax-highlight mode. `'plain'` (default) leaves the text
   *  unhighlighted. `'lua'` enables the Lua mode from
   *  @codemirror/legacy-modes — used for trigger script bodies and
   *  alias / send / replace templates so authors get the same
   *  highlighting they will see when phase A wires `mlua` to
   *  actually execute the body. */
  language?: 'plain' | 'lua';
  /** Disables editing without graying out the text. */
  readOnly?: boolean;
  /** Forwarded to the wrapper element for layout integration. */
  className?: string;
  /** The name a screen reader reads for the editor when no visible
   *  label names it. */
  ariaLabel?: string;
  /** The id of the visible label that names the editor. Wins over
   *  `ariaLabel`. */
  ariaLabelledBy?: string;
  /** The id of the text that describes the editor. */
  ariaDescribedBy?: string;
  /** The full editor a page gives a column of its own, like a plugin's
   *  code under Scripts. The line the caret is on stays clear, as on
   *  every editor, and `marks` tint their lines. Give it the `st-code`
   *  class for the settings field fill, its ring, and the 2 px accent
   *  outline 2 px out on focus. */
  page?: boolean;
  /** The lines the page surface marks as errors, each tinted danger at
   *  10% from end to end, with its message on a hover over its text.
   *  Any other editor marks nothing. */
  marks?: readonly CodeMark[];
}

/** Vosh's shared code editor. CodeMirror 6 wrapped with a theme keyed
 *  to Vosh's CSS variables so it tracks the active theme automatically.
 *  Phase C ships without a language mode (plain text + tab handling);
 *  phase A will add a Lua mode through `@codemirror/lang-lua` once the
 *  backend wires `mlua` for script execution. */
export function CodeEditor({
  value,
  onChange,
  placeholder,
  inline = false,
  minHeight = '1.6em',
  maxHeight = '300px',
  fill = false,
  language = 'plain',
  readOnly = false,
  className,
  ariaLabel,
  ariaLabelledBy,
  ariaDescribedBy,
  page = false,
  marks,
}: Props) {
  // Language extensions. Lua comes from @codemirror/legacy-modes
  // wrapped via StreamLanguage — the modern CodeMirror 6 dedicated
  // Lua package does not exist (only JS / CSS / HTML / etc. have
  // first-class @codemirror/lang-* packages). The label goes on the
  // contenteditable text box, the element a screen reader reaches. The
  // syntax colors come from the theme tokens and replace CodeMirror's
  // default palette, which is made for a white page.
  const extensions = useMemo<Extension[]>(() => {
    const out: Extension[] = [
      syntaxHighlighting(codeHighlightStyle),
      EditorView.contentAttributes.of(
        codeEditorAttributes({ ariaLabel, ariaLabelledBy, ariaDescribedBy }),
      ),
    ];
    if (language === 'lua') out.push(StreamLanguage.define(lua));
    return out;
  }, [language, ariaLabel, ariaLabelledBy, ariaDescribedBy]);
  const theme = useMemo(
    () =>
      EditorView.theme(
        {
          '&': {
            background: 'var(--bg)',
            color: 'var(--text)',
            fontFamily: "'JetBrainsMono Bundled', Menlo, Consolas, ui-monospace, monospace",
            fontSize: '13px',
            borderRadius: '3px',
            border: '1px solid var(--divider)',
            ...(fill ? { height: '100%' } : {}),
          },
          '&.cm-focused': {
            outline: 'none',
            borderColor: 'var(--accent)',
          },
          '.cm-scroller': {
            ...(fill ? { height: '100%', overflow: 'auto' } : { minHeight, maxHeight }),
            fontFamily: 'inherit',
          },
          '.cm-content': {
            padding: inline ? '3px 6px' : '6px',
            caretColor: 'var(--text)',
          },
          '.cm-gutters': {
            background: 'var(--bg)',
            color: 'var(--tertiary)',
            border: 'none',
            borderRight: '1px solid var(--divider)',
          },
          '.cm-activeLine, .cm-activeLineGutter': {
            background: 'transparent',
          },
          '.cm-selectionBackground, ::selection': {
            background: 'var(--accent-soft)',
          },
          '.cm-cursor': {
            borderLeftColor: 'var(--text)',
          },
          ...(page ? PAGE_SURFACE : {}),
        },
        { dark: true },
      ),
    [inline, minHeight, maxHeight, fill, page],
  );

  // The lint marks go on through the view, which only the page surface
  // asks for. They go on again when the text comes from outside, as on
  // Discard, which replaces every line they sat on.
  const [view, setView] = useState<EditorView | null>(null);
  useEffect(() => {
    if (!page || !view) return;
    const update = marksUpdate(view.state, marks ?? []);
    if (update) view.dispatch(update);
  }, [page, view, marks, value]);

  // In fill mode the wrapper becomes a flex column so the inner
  // CodeMirror element can stretch via height: 100% — the parent
  // already provides a definite height via flex sizing.
  const wrapperStyle: CSSProperties = fill
    ? { display: 'flex', flexDirection: 'column', minHeight: 0, flex: '1 1 auto' }
    : {};
  const mirrorStyle: CSSProperties | undefined = fill
    ? { flex: '1 1 auto', minHeight: 0 }
    : undefined;

  return (
    <div className={className} style={wrapperStyle}>
      <CodeMirror
        value={value}
        onChange={onChange}
        {...(placeholder !== undefined ? { placeholder } : {})}
        theme={theme}
        extensions={extensions}
        readOnly={readOnly}
        {...(page ? { onCreateEditor: setView } : {})}
        {...(fill ? { height: '100%' } : {})}
        {...(mirrorStyle ? { style: mirrorStyle } : {})}
        basicSetup={{
          lineNumbers: !inline,
          foldGutter: !inline,
          highlightActiveLine: !inline,
          highlightActiveLineGutter: !inline,
          searchKeymap: false,
          dropCursor: true,
          allowMultipleSelections: false,
          autocompletion: false,
        }}
      />
    </div>
  );
}

/** The page surface over the shared theme. The line numbers keep the
 *  width CodeMirror means them to have. An error mark draws no wavy
 *  underline and tints its whole line instead, the line the caret is on
 *  included. A mark on an empty line is a point, which drops its
 *  corner. Its message floats on the menu recipe in the UI font. */
const PAGE_SURFACE = {
  // CodeMirror sizes the number column for a content box, and the app
  // sizes every box by its border, which took 8 px from it.
  '.cm-lineNumbers .cm-gutterElement': {
    minWidth: '28px',
  },
  '.cm-lintRange-error': {
    backgroundImage: 'none',
  },
  '.cm-lintPoint-error:after': {
    display: 'none',
  },
  '.cm-line:has(.cm-lintRange-error)': {
    background: 'color-mix(in srgb, var(--danger) 10%, transparent)',
  },
  '.cm-line:has(.cm-lintPoint-error)': {
    background: 'color-mix(in srgb, var(--danger) 10%, transparent)',
  },
  '.cm-tooltip.cm-tooltip-hover': {
    border: 'none',
    borderRadius: 'var(--r-row)',
    background: 'var(--raised)',
    boxShadow: 'var(--shadow-float)',
    color: 'var(--text)',
  },
  '.cm-diagnostic, .cm-diagnostic-error': {
    padding: '6px 10px',
    borderLeft: 'none',
    fontFamily: 'var(--font-ui)',
    fontSize: '12px',
    lineHeight: '16px',
  },
};
