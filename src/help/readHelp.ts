// Reads HELP.md, the only source of the in-client help, into its
// sections and topics. Everything before the first `## ` heading is for
// readers of the file and never shows. Each `## Section` heading opens
// a section. Each `### 1.3 Title` heading opens a topic, with a line of
// its own under it that holds the topic id, the key every link into
// Help names:
//
//   ### 1.3 Save your profile
//
//   <!-- id: get-connected.profile-save -->
//
// Markdown viewers hide that line. The body runs from there to the
// next heading, in the format helpContent.ts describes. This module
// imports nothing, so vite.config.ts runs it on HELP.md at build start
// and a broken file stops the build.

export interface HelpTopic {
  /** Stable id, used as the key in the topic rail. */
  id: string;
  /** Display number, like "1.3". Used in the rail and as a search target. */
  number: string;
  /** Short title shown in the rail and as the body heading. */
  title: string;
  /** Section heading the topic groups under in the rail. */
  section: string;
  /** Help body in the format helpContent.ts describes. */
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
