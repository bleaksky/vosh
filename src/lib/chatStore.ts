import { onGmcpPackage, onRouted, onState, type RoutedPayload } from './session';

export interface ChatLine {
  pane: string;
  text: string;
  /** Arrival time (ms epoch). The well's chat pane renders it as a
   *  dim HH:MM column, per the canvas. */
  ts: number;
}

const MAX_LINES = 500;

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;]*[A-Za-z]/g;

function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, '');
}

// Convert a Comm.Channel(.Text) GMCP payload to a ChatLine. Field
// names vary across ROM derivatives; fall back through the common
// alternates so as many servers as possible route here automatically.
function commToChatLine(data: unknown): ChatLine | null {
  if (!data || typeof data !== 'object') return null;
  const obj = data as Record<string, unknown>;
  const pane = String(obj.channel ?? obj.chan ?? 'chat');
  const speaker = obj.speaker ? String(obj.speaker) : obj.talker ? String(obj.talker) : '';
  const raw = String(obj.text ?? obj.msg ?? obj.message ?? '');
  if (!raw) return null;
  const cleaned = stripAnsi(raw);
  return { pane, text: speaker ? `${speaker}: ${cleaned}` : cleaned, ts: Date.now() };
}

// Module-level chat buffer. Subscribes to GMCP / routed / state on
// first read or first subscribe and keeps a rolling window of the
// most recent MAX_LINES entries. ChatPane reads from this store so
// closing and reopening the pane no longer drops history — only a
// disconnect clears the buffer (new session, irrelevant chat).
let lines: ChatLine[] = [];
let listeners: Array<(lines: ChatLine[]) => void> = [];
let started = false;

function notify() {
  const snapshot = lines;
  for (const l of listeners) l(snapshot);
}

function append(line: ChatLine) {
  lines = lines.length >= MAX_LINES ? [...lines.slice(1), line] : [...lines, line];
  notify();
}

export function startChatStore(): void {
  if (started) return;
  started = true;
  const handleComm = (data: unknown) => {
    const line = commToChatLine(data);
    if (line) append(line);
  };
  void onGmcpPackage<unknown>('Comm.Channel', handleComm);
  void onGmcpPackage<unknown>('Comm.Channel.Text', handleComm);
  void onRouted((payload: RoutedPayload) => {
    append({ pane: payload.pane, text: stripAnsi(payload.text), ts: Date.now() });
  });
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      lines = [];
      notify();
    }
  });
}

export function getChatLines(): ChatLine[] {
  startChatStore();
  return lines;
}

export function subscribeChatLines(cb: (lines: ChatLine[]) => void): () => void {
  startChatStore();
  listeners.push(cb);
  return () => {
    listeners = listeners.filter((l) => l !== cb);
  };
}
