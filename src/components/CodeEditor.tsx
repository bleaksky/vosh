import { useMemo, type CSSProperties } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { EditorView } from '@codemirror/view';
import { StreamLanguage, type LanguageSupport } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';

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
  /** ARIA label so screen readers announce the editor purpose. */
  ariaLabel?: string;
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
}: Props) {
  // Language extensions. Lua comes from @codemirror/legacy-modes
  // wrapped via StreamLanguage — the modern CodeMirror 6 dedicated
  // Lua package does not exist (only JS / CSS / HTML / etc. have
  // first-class @codemirror/lang-* packages).
  const extensions = useMemo<LanguageSupport[] | unknown[]>(() => {
    if (language === 'lua') return [StreamLanguage.define(lua)];
    return [];
  }, [language]);
  const theme = useMemo(
    () =>
      EditorView.theme(
        {
          '&': {
            background: 'var(--c-surface, #1c1d24)',
            color: 'var(--c-text, #cdd0d6)',
            fontFamily:
              "'BerkeleyMono Bundled', 'JetBrainsMono Bundled', Menlo, Consolas, ui-monospace, monospace",
            fontSize: '13px',
            borderRadius: '3px',
            border: '1px solid var(--c-border, #2a2c34)',
            ...(fill ? { height: '100%' } : {}),
          },
          '&.cm-focused': {
            outline: 'none',
            borderColor: 'var(--c-accent, #87a987)',
          },
          '.cm-scroller': {
            ...(fill ? { height: '100%', overflow: 'auto' } : { minHeight, maxHeight }),
            fontFamily: 'inherit',
          },
          '.cm-content': {
            padding: inline ? '3px 6px' : '6px',
            caretColor: 'var(--c-text, #cdd0d6)',
          },
          '.cm-gutters': {
            background: 'var(--c-bg, #15161b)',
            color: 'var(--c-text-faint, #6b6f78)',
            border: 'none',
            borderRight: '1px solid var(--c-border, #2a2c34)',
          },
          '.cm-activeLine, .cm-activeLineGutter': {
            background: 'transparent',
          },
          '.cm-selectionBackground, ::selection': {
            background: 'var(--c-accent-soft, rgba(135, 169, 135, 0.25))',
          },
          '.cm-cursor': {
            borderLeftColor: 'var(--c-text, #cdd0d6)',
          },
        },
        { dark: true },
      ),
    [inline, minHeight, maxHeight, fill],
  );

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
    <div className={className} style={wrapperStyle} aria-label={ariaLabel}>
      <CodeMirror
        value={value}
        onChange={onChange}
        {...(placeholder !== undefined ? { placeholder } : {})}
        theme={theme}
        extensions={extensions as never[]}
        readOnly={readOnly}
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
