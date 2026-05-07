import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface OutputPayload {
  bytes: number[];
}

export type StatePayload =
  | { kind: 'connecting'; host: string; port: number; tls: boolean }
  | { kind: 'connected'; host: string; port: number; tls: boolean }
  | { kind: 'disconnected'; reason: string | null };

export async function connectSession(host: string, port: number, tls: boolean): Promise<void> {
  await invoke('session_connect', { host, port, tls });
}

export async function disconnectSession(): Promise<void> {
  await invoke('session_disconnect');
}

export async function sendBytes(bytes: Uint8Array): Promise<void> {
  await invoke('session_send', { bytes: Array.from(bytes) });
}

export async function sendLine(line: string): Promise<void> {
  const encoder = new TextEncoder();
  const payload = encoder.encode(line + '\r\n');
  await sendBytes(payload);
}

export async function onOutput(cb: (bytes: Uint8Array) => void): Promise<UnlistenFn> {
  return listen<OutputPayload>('session://output', (event) => {
    cb(new Uint8Array(event.payload.bytes));
  });
}

export async function onState(cb: (state: StatePayload) => void): Promise<UnlistenFn> {
  return listen<StatePayload>('session://state', (event) => {
    cb(event.payload);
  });
}
