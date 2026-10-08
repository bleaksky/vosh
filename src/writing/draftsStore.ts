import { useSyncExternalStore } from 'react';
import {
  writingCharacterSet,
  writingFileGet,
  writingSwitchesSet,
  type Draft,
  type WritingCharacter,
  type WritingFile,
  type WritingKind,
} from '../ipc/writing';
import { createDebouncedWrite, pendingWrites, type DebouncedWrite } from '../lib/pendingWrites';
import { createStore } from '../stores/store';

// Your drafts and posts for the writing card, as writing.toml keeps
// them for each character on each world. The card changes a character
// here as you type, and each change reaches the file a moment later, or
// at once when the window flushes its writes on quit. A draft saves as
// you type, so closing the card never asks.

/** Where a character plays. */
export interface World {
  host: string;
  port: number;
}

const EMPTY: WritingFile = { version: 1, spelling: true, guide: true, characters: {} };

const store = createStore<WritingFile>(EMPTY);
let loaded: Promise<void> | null = null;

/** Whether `character` is `name` on `world`. The game spells a name
 *  with its first letter up, so the case of the rest decides nothing. */
function isCharacter(character: WritingCharacter, world: World, name: string): boolean {
  return (
    character.host === world.host &&
    character.port === world.port &&
    character.name.toLowerCase() === name.toLowerCase()
  );
}

/** The key the file holds `name` on `world` under, or a new one. */
function keyOf(file: WritingFile, world: World, name: string): string {
  const held = Object.entries(file.characters).find(([, c]) => isCharacter(c, world, name));
  return held ? held[0] : `${world.host}:${world.port} ${name.toLowerCase()}`;
}

/** Read writing.toml once. A file Vosh cannot read leaves the card
 *  working from memory. */
export function loadWriting(): Promise<void> {
  loaded ??= writingFileGet()
    .then((file) => store.set(file))
    .catch((e: unknown) => console.error('[writing] reading writing.toml failed', e));
  return loaded;
}

export function getWritingFile(): WritingFile {
  return store.get();
}

export function useWritingFile(): WritingFile {
  return useSyncExternalStore(store.subscribe, store.get, store.get);
}

/** A character's writing, or an empty one. */
export function characterOf(file: WritingFile, world: World, name: string): WritingCharacter {
  return (
    Object.values(file.characters).find((c) => isCharacter(c, world, name)) ?? {
      host: world.host,
      port: world.port,
      name,
      drafts: [],
      sent: [],
    }
  );
}

/** One debounced write for each character. */
const writes = new Map<string, DebouncedWrite<WritingCharacter>>();

function writeOf(key: string): DebouncedWrite<WritingCharacter> {
  let write = writes.get(key);
  if (!write) {
    write = createDebouncedWrite((character) =>
      writingCharacterSet(character).catch((e: unknown) =>
        console.error('[writing] keeping a draft failed', e),
      ),
    );
    writes.set(key, write);
    pendingWrites.register(() => write?.flush());
  }
  return write;
}

/** Keep `character` now, and in writing.toml a moment later. */
export function keepCharacter(character: WritingCharacter): void {
  const file = store.get();
  const key = keyOf(file, character, character.name);
  store.set({ ...file, characters: { ...file.characters, [key]: character } });
  writeOf(key).schedule(() => character, 600);
}

/** Keep the card's Check spelling and whether its guide shows. */
export function keepSwitches(spelling: boolean, guide: boolean): void {
  const file = store.get();
  if (file.spelling === spelling && file.guide === guide) return;
  store.set({ ...file, spelling, guide });
  void writingSwitchesSet(spelling, guide).catch((e: unknown) =>
    console.error('[writing] keeping the switches failed', e),
  );
}

let nextId = 0;

/** A new draft of `kind`. */
export function newDraft(kind: WritingKind, room: string | null = null): Draft {
  nextId += 1;
  return {
    id: `${Date.now().toString(36)}-${nextId}`,
    kind,
    text: [],
    at: Date.now(),
    ...(room ? { room } : {}),
  };
}

/** `character` with `draft` in place of the draft it replaces, newest
 *  first. */
export function withDraft(character: WritingCharacter, draft: Draft): WritingCharacter {
  const others = character.drafts.filter((d) => d.id !== draft.id);
  return { ...character, drafts: [{ ...draft, at: Date.now() }, ...others] };
}

/** `character` without the draft `id`. */
export function withoutDraft(character: WritingCharacter, id: string): WritingCharacter {
  return { ...character, drafts: character.drafts.filter((d) => d.id !== id) };
}

/** `character` with `draft` moved from the drafts to Sent, as it posted. */
export function posted(character: WritingCharacter, draft: Draft): WritingCharacter {
  return {
    ...withoutDraft(character, draft.id),
    sent: [{ ...draft, at: Date.now() }, ...character.sent].slice(0, 20),
  };
}

/** The draft of a text the game holds one of, your description, beast,
 *  history, personality or purpose, or a new one. */
export function onlyDraft(character: WritingCharacter, kind: WritingKind): Draft {
  return character.drafts.find((d) => d.kind === kind) ?? newDraft(kind);
}
