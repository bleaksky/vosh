// Settings for the shared CodeMirror editor (src/components/CodeEditor.tsx)
// that do not need the DOM, so tests can check them.

/** How a code editor names itself to a screen reader. */
export interface CodeEditorLabel {
  /** A name to read when no visible label names the editor. */
  ariaLabel?: string | undefined;
  /** The id of a visible label, like a settings row label. It wins over
   *  `ariaLabel`, so the name a screen reader reads matches the page. */
  ariaLabelledBy?: string | undefined;
  /** The id of text that describes the editor, like a row description. */
  ariaDescribedBy?: string | undefined;
}

/** The attributes for the editor's text box. CodeMirror puts them on its
 *  contenteditable element, the one with the textbox role that takes
 *  focus, since a label on a wrapper names nothing a screen reader
 *  reaches. */
export function codeEditorAttributes(label: CodeEditorLabel): Record<string, string> {
  const attrs: Record<string, string> = {};
  if (label.ariaLabelledBy) attrs['aria-labelledby'] = label.ariaLabelledBy;
  else if (label.ariaLabel) attrs['aria-label'] = label.ariaLabel;
  if (label.ariaDescribedBy) attrs['aria-describedby'] = label.ariaDescribedBy;
  return attrs;
}
