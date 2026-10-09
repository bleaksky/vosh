// The chat test. Spell check and the coloring as you type both read it,
// so they agree on what a chat line is.

// Regex set for "is this line chat-like?" — when the toggle in
// Settings is on and one of these matches the current input, the
// webview's native spell-check flips on for the prompt. Otherwise
// MUD verbs like `kill` / `oload` would light up red on every line.
const CHAT_PREFIXES: RegExp[] = [
  /^say\b/i,
  /^'/, // `'hello` = say hello (FL-style say shortcut)
  /^"/, // `"hello` = say hello on some MUDs
  /^tell\s+\S+\s/i,
  /^t\s+\S+\s/i,
  /^reply\b/i,
  /^r\s+/i,
  /^whisper\s+\S+\s/i,
  /^chat\b/i,
  /^gossip\b/i,
  /^;/, // `;hello` = gossip on some servers
  /^ooc\b/i,
  /^clan\b/i,
  /^cb\b/i,
  /^imm(talk)?\b/i,
  /^immchat\b/i,
  /^immtell\b/i,
  /^quote\b/i,
  /^emote\b/i,
  /^pmote\b/i,
];

/** True when `line` starts with a chat verb. */
export function looksLikeChat(line: string): boolean {
  const trimmed = line.trimStart();
  if (trimmed.length === 0) return false;
  return CHAT_PREFIXES.some((re) => re.test(trimmed));
}
