// A code block in a help topic, read into colored pieces the way the
// code editor reads Lua (src/ui/CodeEditor.tsx), so a script in Help
// colors as it does under Scripts. help.css gives each class the
// editor's theme token (src/ui/codeEditorStyle.ts).

import { StreamLanguage } from '@codemirror/language';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { classHighlighter, highlightCode } from '@lezer/highlight';

/** One run of a code block, with the highlight class it takes, or an
 *  empty class for plain text and line breaks. */
export interface CodePiece {
  text: string;
  cls: string;
}

const luaParser = StreamLanguage.define(lua).parser;

/** The pieces of `code`, in order, that join back into `code`. Only
 *  Lua gets colors, and any other language reads as one plain piece. */
export function codePieces(code: string, lang: string): CodePiece[] {
  if (lang !== 'lua') return [{ text: code, cls: '' }];
  const out: CodePiece[] = [];
  highlightCode(
    code,
    luaParser.parse(code),
    classHighlighter,
    (text, cls) => out.push({ text, cls }),
    () => out.push({ text: '\n', cls: '' }),
  );
  return out;
}
