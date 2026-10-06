import { onRouted, type RoutedPayload, type StatePayload } from '../../ipc/session';
import { createSessionStore } from '../sessionStore';

/** Which way a tell went. Aabahran marks a tell you receive
 *  `received`. It sends nothing for a tell you send, so a `sent` line
 *  comes from a trigger that routes the terminal line (parseRoutedLine),
 *  the Tells you send preset out of the box. */
export type ChatDirection = 'sent' | 'received';

export interface ChatLine {
  /** The channel, or the pane a trigger route names. */
  pane: string;
  /** Who spoke, as the game names them to you: `Tolliver`, `someone`,
   *  `a Blackwatch villager`. On a tell you send, who it went to.
   *  Null on a routed line, which keeps its own wording. */
  speaker: string | null;
  /** The message alone, or the whole routed line. */
  text: string;
  /** The language the message was spoken in, `foreign` when you did
   *  not know it. Only say, tell, yell, and gtell carry one. */
  language: string | null;
  /** False when the message reached you garbled or foreign. */
  understood: boolean | null;
  /** Set on tells only. */
  direction: ChatDirection | null;
  /** Arrival time (ms epoch). */
  ts: number;
}

const MAX_LINES = 500;

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;]*[A-Za-z]/g;

function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, '');
}

function fieldText(value: unknown): string {
  return value === undefined || value === null ? '' : stripAnsi(String(value));
}

/** A Comm.Channel(.Text) payload as a chat line, or null when it carries
 *  no message. Field names vary across ROM derivatives, so it falls back
 *  through the common alternates and as many servers as possible route
 *  here on their own. */
export function parseCommChannel(data: unknown, ts: number = Date.now()): ChatLine | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const text = fieldText(obj.text ?? obj.msg ?? obj.message);
  if (!text) return null;
  const speaker = fieldText(obj.speaker ?? obj.talker);
  const language = typeof obj.language === 'string' ? obj.language : null;
  const understood = typeof obj.understood === 'boolean' ? obj.understood : null;
  const direction = obj.direction === 'sent' || obj.direction === 'received' ? obj.direction : null;
  return {
    pane: String(obj.channel ?? obj.chan ?? 'chat'),
    speaker: speaker || null,
    text,
    language,
    understood,
    direction,
    ts,
  };
}

// The line the game prints for a tell you send (languages.c
// compose_tell): `You tell Tolliver 'text'`, with ` in Elvish` before the
// quote when you spoke anything but common, and `You project to` for a
// telepath. The recipient can run to several words, `a city guard`. A
// group tell you send prints the same way to `your group`
// (compose_grouptell_open).
const SENT_TELL_RE = /^You (?:tell|project to) (.+?)(?: in ([A-Z][\w']*))? '([\s\S]*)'$/;

/** A line a trigger routed to a pane, or null for one the pane skips.
 *  The line keeps its own wording, except the game's line for a tell you
 *  send, which reads as your side of the tell. The line for a group tell
 *  you send is skipped, because the game echoes that message back to you
 *  as a gtell packet (act_comm.c do_gtell) and the pane already has it.
 *  The Tells you send preset catches both lines. */
export function parseRoutedLine(payload: RoutedPayload, ts: number = Date.now()): ChatLine | null {
  const text = stripAnsi(payload.text).trimEnd();
  const sent = SENT_TELL_RE.exec(text);
  if (sent?.[1] === 'your group') return null;
  if (sent) {
    return {
      pane: payload.pane,
      speaker: sent[1],
      text: sent[3],
      // The game leaves the language out for common.
      language: sent[2] ?? 'common',
      understood: true,
      direction: 'sent',
      ts,
    };
  }
  return {
    pane: payload.pane,
    speaker: null,
    text,
    language: null,
    understood: null,
    direction: null,
    ts,
  };
}

/** The lines with `line` added and the oldest dropped past MAX_LINES,
 *  or the same lines when there is nothing to add. */
function append(lines: ChatLine[], line: ChatLine | null): ChatLine[] {
  if (!line) return lines;
  return lines.length >= MAX_LINES ? [...lines.slice(1), line] : [...lines, line];
}

/** Where a session's chat lines came from, as its connection dialed. */
interface World {
  host: string;
  port: number;
}

/** A session's chat lines and the world they came from, null until the
 *  page sees the session dial. */
interface ChatState {
  lines: ChatLine[];
  world: World | null;
}

const EMPTY: ChatState = { lines: [], world: null };

/** The state with `line` added. */
function addLine(state: ChatState, line: ChatLine | null): ChatState {
  const lines = append(state.lines, line);
  return lines === state.lines ? state : { ...state, lines };
}

/** The state after a change in the connection. Your Disconnect, which
 *  carries no reason, empties the lines, and so does a dial to another
 *  world. A drop keeps them, so the tells you got stay through every
 *  try of a redial. Lines heard before the page saw the session dial
 *  stay through its next dial, since their world is unknown. */
function onConnection(state: ChatState, payload: StatePayload): ChatState {
  if (payload.kind === 'disconnected') return payload.reason === null ? EMPTY : state;
  const { host, port } = payload;
  const { world } = state;
  if (world?.host === host && world.port === port) return state;
  const elsewhere = payload.kind === 'connecting' && world !== null;
  return { lines: elsewhere ? [] : state.lines, world: { host, port } };
}

// The most recent MAX_LINES chat lines of each session, from the channel
// packages and the lines triggers route. ChatPane reads the selected
// session's here, so closing and reopening the pane keeps the history.
// A session's lines go with its slot when it closes.
const store = createSessionStore<ChatState, ChatLine[]>({
  state: EMPTY,
  packages: {
    'Comm.Channel': (state, data) => addLine(state, parseCommChannel(data)),
    'Comm.Channel.Text': (state, data) => addLine(state, parseCommChannel(data)),
  },
  connection: onConnection,
  events: [
    (apply) =>
      onRouted((payload, session) =>
        apply(session, (state) => addLine(state, parseRoutedLine(payload))),
      ),
  ],
  view: (state) => state.lines,
});

export const startChatStore = store.start;

export function getChatLines(): ChatLine[] {
  store.start();
  return store.get();
}

export function subscribeChatLines(cb: (lines: ChatLine[]) => void): () => void {
  return store.subscribe(() => cb(store.get()));
}
