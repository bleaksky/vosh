import { onOutput, onState } from './session';

// Raw-log tail for the well's log pane: the last N lines of session
// output, ANSI-stripped, each stamped with arrival time. Fed from the
// same session://output stream the terminal renders, so the pane
// shows exactly what came over the wire without a second data path.

export interface LogLine {
  ts: number;
  text: string;
}

const MAX_LINES = 400;

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;]*[A-Za-z]|\x1b\][^\x07]*\x07/g;

let lines: LogLine[] = [];
let partial = '';
let started = false;
const subs = new Set<() => void>();

function push(text: string): void {
  lines.push({ ts: Date.now(), text });
  if (lines.length > MAX_LINES) lines = lines.slice(-MAX_LINES);
}

function ingest(chunk: string): void {
  const clean = (partial + chunk.replace(ANSI_RE, '')).replace(/\r/g, '');
  const parts = clean.split('\n');
  partial = parts.pop() ?? '';
  for (const line of parts) {
    if (line.trim().length > 0) push(line);
  }
  if (parts.length > 0) {
    for (const cb of subs) cb();
  }
}

const decoder = new TextDecoder();

export function startLogTail(): void {
  if (started) return;
  started = true;
  void onOutput((bytes) => ingest(decoder.decode(bytes, { stream: true })));
  void onState((payload) => {
    if (payload.kind === 'disconnected') {
      push(`[disconnected${payload.reason ? `: ${payload.reason}` : ''}]`);
      for (const cb of subs) cb();
    }
  });
}

export function getLogLines(): LogLine[] {
  return lines;
}

export function subscribeLogTail(cb: () => void): () => void {
  subs.add(cb);
  return () => {
    subs.delete(cb);
  };
}
