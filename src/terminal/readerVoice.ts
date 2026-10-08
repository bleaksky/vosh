import { getReader } from '../stores/session/readerStore';

// What the screen reader feed says and when (R21 and R25 review, board
// 13, Q19 to Q21): the pulse that joins lines, the words of one
// announcement, and the voice the prompt key and its palette row speak
// through while ScreenReaderFeed is mounted.

/** How long a pulse gathers lines, from its first, the game's own
 *  pulse. */
export const PULSE_MS = 250;

/** What the prompt key says before the session has shown a prompt. */
export const NO_PROMPT = 'Vosh has not seen your prompt yet.';

/** The mounted feed's voice, while the reader is on. */
let speak: ((parts: string[]) => void) | null = null;

/** Give the mounted feed's voice, and hand back how to take it away. */
export function setVoice(voice: (parts: string[]) => void): () => void {
  speak = voice;
  return () => {
    if (speak === voice) speak = null;
  };
}

/** Read the selected session's latest prompt aloud, for the prompt key
 *  and its palette row. Does nothing while the reader is off. */
export function readPrompt(): void {
  const prompt = getReader().prompt;
  speak?.(prompt === null ? [NO_PROMPT] : prompt.split('\n'));
}

/** What one pulse says, a part for each line it reads: every line, or
 *  past `burst` how many came and the last, then the prompt's lines
 *  when there is one to read. */
export function announcement(
  lines: readonly string[],
  count: number,
  prompt: string | null,
  burst: number,
): string[] {
  const parts = count > burst ? [`${count} lines.`, ...lines.slice(-1)] : [...lines];
  return prompt === null ? parts : [...parts, ...prompt.split('\n')];
}
