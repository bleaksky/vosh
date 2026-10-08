// The screen reader's events (R21 and R25 review, board 13, Q19 to
// Q21). The session sends what each read shows while Read new game
// lines is on, and Settings tells every window your four reader choices
// as one.

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SCREEN_READER, SCREEN_READER_CHANGED } from './events';
import { sessionOf } from './session';
import { normalizeScreenReaderBurst, type UiConfig } from './uiConfig';

/** What a screen reader reads of one read while Read new game lines is
 *  on: the plain text of each line that shows, the last 500 at most, how
 *  many showed, your prompt's text when the read brought one, and
 *  whether Vosh is in the background. */
export interface ScreenReaderFeed {
  lines: string[];
  count: number;
  prompt: string | null;
  away: boolean;
}

/** Hear what a screen reader reads of each read of a session, with that
 *  session. */
export async function onScreenReader(
  cb: (feed: ScreenReaderFeed, session: number) => void,
): Promise<UnlistenFn> {
  return listen<ScreenReaderFeed & { session?: number }>(SCREEN_READER, (event) => {
    const { lines, count, prompt, away } = event.payload;
    cb({ lines, count, prompt, away }, sessionOf(event.payload));
  });
}

/** Your four choices under Screen reader in Settings, Accessibility. */
export type ScreenReaderOptions = Pick<
  UiConfig,
  'screen_reader' | 'screen_reader_background' | 'screen_reader_prompt' | 'screen_reader_burst'
>;

/** The screen reader choices a config holds. */
export function screenReaderOf(config: ScreenReaderOptions): ScreenReaderOptions {
  return {
    screen_reader: config.screen_reader,
    screen_reader_background: config.screen_reader_background,
    screen_reader_prompt: config.screen_reader_prompt,
    screen_reader_burst: config.screen_reader_burst,
  };
}

/** The choices until you make any: off, and a burst of 8. */
export const DEFAULT_SCREEN_READER: ScreenReaderOptions = normalizeScreenReader({});

/** Read the choices off the bus as the config reads them, so anything
 *  missing or unknown is off, or a burst of 8. */
export function normalizeScreenReader(raw: unknown): ScreenReaderOptions {
  const o = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  return {
    screen_reader: o.screen_reader === true,
    screen_reader_background: o.screen_reader_background === true,
    screen_reader_prompt: o.screen_reader_prompt === true,
    screen_reader_burst: normalizeScreenReaderBurst(o.screen_reader_burst),
  };
}

/** Hear your screen reader choices after Settings saves any of them,
 *  or the ones a profile switch brings. */
export async function subscribeScreenReaderChanged(
  cb: (options: ScreenReaderOptions) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(SCREEN_READER_CHANGED, (event) => {
    cb(normalizeScreenReader(event.payload));
  });
}
