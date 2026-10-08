// The in-client help. HELP.md at the repo root is the only source: the
// Help window reads it when Vosh is built, and readHelp below turns it
// into the sections and topics the window shows. Everything before the
// first `## ` heading is for readers of the file and never shows.
//
// Each `## Section` heading opens a section. Each `### 1.3 Title`
// heading opens a topic, with a line of its own under it that holds the
// topic id, the key every link into Help names:
//
//   ### 1.3 Save your profile
//
//   <!-- id: get-connected.profile-save -->
//
// Markdown viewers hide that line. The body runs from there to the
// next heading. Bodies use the lightweight format the Help window
// parses:
//   - Paragraphs separated by a blank line (\n\n).
//   - Lines starting with "- " render as bullet list items.
//   - A block whose lines all start with "|" renders as a table, its
//     first row the head and its row of dashes left out.
//   - A block of one line like `[Open Get started](vosh:get-started)`
//     renders as a button that runs that Vosh action.
//   - Backticks delimit inline code.
//   - A block fenced by ``` lines, like ```lua, renders as a code
//     block, blank lines and all.

import helpMd from '../../HELP.md?raw';

export interface HelpTopic {
  /** Stable id, used as the key in the topic rail. */
  id: string;
  /** Display number, like "1.3". Used in the rail and as a search target. */
  number: string;
  /** Short title shown in the rail and as the body heading. */
  title: string;
  /** Section heading the topic groups under in the rail. */
  section: string;
  /** Help body in the lightweight markdown subset described above. */
  body: string;
}

/** Read the help file into its sections, in order, and its topics, in
 *  the format at the top of this file. Headings count only outside
 *  ``` fences. A topic with no id line, or an id or number used twice,
 *  throws, so a broken HELP.md fails the tests and the build. */
export function readHelp(md: string): { sections: string[]; topics: HelpTopic[] } {
  const sections: string[] = [];
  const topics: HelpTopic[] = [];
  let open: { number: string; title: string; section: string; lines: string[] } | null = null;
  let fenced = false;

  const close = () => {
    if (!open) return;
    const { number, title, section, lines } = open;
    const at = lines.findIndex((l) => l.trim().length > 0);
    const id = /^<!-- id: (\S+) -->$/.exec(lines[at] ?? '')?.[1];
    if (!id) throw new Error(`HELP.md: ${number} ${title} has no id line under its heading`);
    if (topics.some((t) => t.id === id)) throw new Error(`HELP.md: the id ${id} is used twice`);
    if (topics.some((t) => t.number === number)) {
      throw new Error(`HELP.md: the number ${number} is used twice`);
    }
    topics.push({
      id,
      number,
      title,
      section,
      body: lines
        .slice(at + 1)
        .join('\n')
        .trim(),
    });
    open = null;
  };

  for (const line of md.split('\n')) {
    if (!fenced) {
      const heading = /^## (.+)$/.exec(line);
      if (heading) {
        close();
        sections.push(heading[1].trim());
        continue;
      }
      const topic = /^### (\S+) (.+)$/.exec(line);
      const section = sections[sections.length - 1];
      if (topic && section !== undefined) {
        close();
        open = { number: topic[1], title: topic[2].trim(), section, lines: [] };
        continue;
      }
    }
    if (line.startsWith('```')) fenced = !fenced;
    open?.lines.push(line);
  }
  close();
  return { sections, topics };
}

/** The sections in the order the rail lists them, and every topic. */
export const { sections: HELP_SECTIONS, topics: HELP_TOPICS } = readHelp(helpMd);

/** What a help button does. */
export type HelpAction = 'get-started';

const HELP_ACTIONS: readonly HelpAction[] = ['get-started'];

/** One block of a help body. */
export type HelpBlock =
  | { kind: 'paragraph'; text: string }
  | { kind: 'list'; items: string[] }
  | { kind: 'table'; head: string[]; rows: string[][] }
  | { kind: 'code'; lang: string; text: string }
  | { kind: 'action'; label: string; action: HelpAction };

/** The button a block like `[Open Get started](vosh:get-started)`
 *  draws, or null when the block is no button. */
function actionBlock(block: string): HelpBlock | null {
  const m = /^\[([^\]\n]+)\]\(vosh:([a-z-]+)\)$/.exec(block);
  const action = HELP_ACTIONS.find((a) => a === m?.[2]);
  return m && action ? { kind: 'action', label: m[1], action } : null;
}

/** The cells of a table line, `| a | b |` as `a` and `b`. */
function tableCells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim());
}

/** Read a help body into its blocks, in the format at the top of this
 *  file. */
export function parseHelpBody(body: string): HelpBlock[] {
  return body.split(/^(```[^\n]*\n[\s\S]*?\n```)$/m).flatMap((chunk, i): HelpBlock[] => {
    // The split puts each fenced block at an odd index.
    if (i % 2 === 1) {
      const lines = chunk.split('\n');
      return [
        { kind: 'code', lang: lines[0].slice(3).trim(), text: lines.slice(1, -1).join('\n') },
      ];
    }
    return proseBlocks(chunk);
  });
}

/** The paragraphs, lists and tables of a stretch of body with no code. */
function proseBlocks(body: string): HelpBlock[] {
  return body
    .split(/\n\n+/)
    .map((b) => b.trim())
    .filter((b) => b.length > 0)
    .map((block): HelpBlock => {
      const action = actionBlock(block);
      if (action) return action;
      const lines = block.split('\n');
      if (lines.every((l) => l.startsWith('- '))) {
        return { kind: 'list', items: lines.map((l) => l.slice(2)) };
      }
      if (lines.every((l) => l.startsWith('|'))) {
        const rows = lines.filter((l) => !/^\|[\s|:-]+\|?$/.test(l)).map(tableCells);
        return { kind: 'table', head: rows[0] ?? [], rows: rows.slice(1) };
      }
      return { kind: 'paragraph', text: block };
    });
}

export function searchTopics(query: string, topics: HelpTopic[] = HELP_TOPICS): HelpTopic[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return topics;
  return topics.filter((t) => {
    if (t.number.toLowerCase().includes(q)) return true;
    if (t.title.toLowerCase().includes(q)) return true;
    if (t.section.toLowerCase().includes(q)) return true;
    if (t.body.toLowerCase().includes(q)) return true;
    return false;
  });
}
