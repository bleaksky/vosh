// Tokenizer for the vitals template string.
//
// The template is plain text with `%name` interpolations. `%%` emits
// a literal `%`. Unknown tokens fall through as literal `%foo` so a
// typo is visible to the user instead of silently disappearing.
//
// Returned segments are either { kind: 'text' } or { kind: 'token' }.
// The renderer (TemplateVitalsRow in VitalsBar.tsx) walks the array
// and produces React spans; this module is pure and tests cleanly.

export type TemplateSegment =
  | { kind: 'text'; text: string }
  | { kind: 'token'; name: string; raw: string };

// Curated tokens with dedicated render logic (computed values like
// `%pct_hp`, glyph bars like `%bar_hp`, tintin aliases like `%mn`
// for mana). Listed here so the Settings UI can surface them as
// suggestions; the tokenizer itself accepts ANY `%name` and lets the
// renderer try curated first, then fall through to Char.Vitals /
// Char.Worth field lookups.
export const VITALS_TEMPLATE_TOKENS = [
  'hp',
  'mhp',
  'mn',
  'mmn',
  'mv',
  'mmv',
  'pct_hp',
  'pct_mn',
  'pct_mv',
  'dhp',
  'dmn',
  'dmv',
  'bar_hp',
  'bar_mn',
  'bar_mv',
  'tick',
  'time',
] as const;

// Walk the template string left-to-right, splitting it into text and
// token segments. Two token forms are supported:
//   `%name`    — greedy [a-z0-9_] after the `%`. Use when the token
//                is followed by whitespace, punctuation that breaks
//                the char class (`(`, `)`, `:`, `.`), or end of
//                string. Example: `%hp/%mhp`, `%pct_hp%%`.
//   `%{name}`  — explicit braces around the name. Use when the token
//                is immediately followed by alphanumeric text that
//                would otherwise be consumed by the greedy form.
//                Example: `%{cp}cp` renders the cp value followed by
//                literal `cp`, where `%cpcp` would try to look up
//                the (probably non-existent) `cpcp` field.
// Any name is accepted as a token; the renderer decides whether to
// resolve to a value or render a red `%name` literal.
export function tokenizeTemplate(template: string): TemplateSegment[] {
  const out: TemplateSegment[] = [];
  let buffer = '';
  let i = 0;
  while (i < template.length) {
    const ch = template[i];
    if (ch !== '%') {
      buffer += ch;
      i += 1;
      continue;
    }
    // `%%` → literal `%`.
    if (template[i + 1] === '%') {
      buffer += '%';
      i += 2;
      continue;
    }
    // `%{name}` — explicit braces force the token boundary.
    if (template[i + 1] === '{') {
      const closeIdx = template.indexOf('}', i + 2);
      if (closeIdx > i + 2) {
        const rawName = template.slice(i + 2, closeIdx);
        // Allow `:` `,` `#` so color directives like `%{c:196}`,
        // `%{c:255,128,0}`, and `%{c:#ff8800}` tokenize. Plain names
        // (the only thing earlier templates used) still match.
        if (/^[a-z0-9_:,#]+$/i.test(rawName)) {
          if (buffer.length > 0) {
            out.push({ kind: 'text', text: buffer });
            buffer = '';
          }
          out.push({
            kind: 'token',
            name: rawName.toLowerCase(),
            raw: template.slice(i, closeIdx + 1),
          });
          i = closeIdx + 1;
          continue;
        }
      }
      // Malformed `%{...}` — keep literal so the user sees their typo.
      buffer += '%';
      i += 1;
      continue;
    }
    // Greedy `%name` form.
    let j = i + 1;
    while (j < template.length && /[a-z0-9_]/i.test(template[j])) {
      j += 1;
    }
    const name = template.slice(i + 1, j).toLowerCase();
    if (name.length === 0) {
      // Bare `%` at end of string or before whitespace — keep literal.
      buffer += '%';
      i += 1;
      continue;
    }
    if (buffer.length > 0) {
      out.push({ kind: 'text', text: buffer });
      buffer = '';
    }
    out.push({ kind: 'token', name, raw: template.slice(i, j) });
    i = j;
  }
  if (buffer.length > 0) {
    out.push({ kind: 'text', text: buffer });
  }
  return out;
}
