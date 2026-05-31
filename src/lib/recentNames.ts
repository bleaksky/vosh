// Bounded recency cache of capitalized name-like tokens seen in MUD
// output, used as a Tab-completion source so the user can complete
// names that are not currently in Room.Chars (people on the who
// list, folks who just spoke in a comm channel, players the user
// just considered, etc.).
//
// The scrape is intentionally broad: any word that starts with an
// uppercase letter and is 4+ chars passes through. False positives
// (proper-noun place names, capitalized words at sentence start)
// end up in the candidate list but rarely shadow real player names
// because completion is prefix-based — typing more characters
// narrows down the list.
//
// Cost per chunk: one regex pass + Map.set per match. Sub-millisecond
// for typical MUD output sizes, so this adds no perceptible latency
// to the output pipeline.

const MAX_AGE_MS = 30 * 60 * 1000;
const MAX_ENTRIES = 1024;
const TOKEN_RE = /\b[A-Z][a-zA-Z]{3,}\b/g;

// Common English words that pass the regex but are never useful as
// a completion candidate. Trimmed to the highest-frequency offenders;
// adding more is fine if specific words show up too often in
// completion cycles.
const STOPLIST = new Set([
  'Your',
  'You',
  'They',
  'Them',
  'Their',
  'This',
  'That',
  'These',
  'Those',
  'When',
  'Where',
  'What',
  'Which',
  'Will',
  'Would',
  'Could',
  'Should',
  'There',
  'Then',
  'With',
  'From',
  'Have',
  'Been',
  'Were',
  'Was',
  'Are',
  'But',
  'Not',
  'For',
  'And',
  'The',
  'Has',
  'Had',
  'Did',
  'Does',
  'Done',
  'Doing',
]);

// name -> last-seen timestamp (ms)
const seen = new Map<string, number>();

/** Scrape capitalized name-like tokens from `text` and bump their
 *  last-seen timestamp. Call this from anywhere that sees MUD output
 *  text (the Terminal output subscription, the chat panel, etc.).
 *  Idempotent and cheap. */
export function ingestRecentNames(text: string): void {
  if (!text) return;
  const now = Date.now();
  TOKEN_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = TOKEN_RE.exec(text)) !== null) {
    const name = match[0];
    if (STOPLIST.has(name)) continue;
    seen.set(name, now);
  }
  if (seen.size > MAX_ENTRIES) pruneOldestPastCap(now);
}

/** Names seen in the last MAX_AGE_MS, ordered most-recent first.
 *  Used by Input.tsx Tab completion as a source after the typed-
 *  history words and the live Room.Chars list. */
export function recentNames(): string[] {
  const now = Date.now();
  const fresh: { name: string; ts: number }[] = [];
  for (const [name, ts] of seen) {
    if (now - ts <= MAX_AGE_MS) fresh.push({ name, ts });
  }
  fresh.sort((a, b) => b.ts - a.ts);
  return fresh.map((e) => e.name);
}

function pruneOldestPastCap(now: number): void {
  // Prune by age first.
  for (const [name, ts] of seen) {
    if (now - ts > MAX_AGE_MS) seen.delete(name);
  }
  if (seen.size <= MAX_ENTRIES) return;
  // Still over cap: drop the oldest entries until we fit.
  const sorted = [...seen.entries()].sort((a, b) => a[1] - b[1]);
  const toRemove = sorted.length - MAX_ENTRIES;
  for (let i = 0; i < toRemove; i++) seen.delete(sorted[i][0]);
}
