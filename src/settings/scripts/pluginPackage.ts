// What Install hands Rust: the bytes of a .zip you picked, or the files
// of a folder you dropped on the Scripts list, each by its path inside
// what you dropped. Rust checks all of it again. The page holds the
// same caps as it reads, so a folder dropped by mistake, like
// your Downloads, is never read whole into the window.

import type { DroppedFile, PluginPackage } from '../../ipc/scripts';

/** The caps of src-tauri/src/app/plugins/archive.rs. */
const MAX_BYTES = 5 * 1024 * 1024;
const MAX_FILES = 200;

/** The refusal Rust gives a plugin over the caps, word for word. */
function tooBig(fileName: string): Error {
  return new Error(`Vosh did not install ${fileName}. It holds more than 5 MB or 200 files.`);
}

/** `step`, or the sentence Rust gives a file it cannot read when the
 *  page cannot read `fileName` either, in place of the browser's. */
async function read<T>(fileName: string, step: Promise<T>): Promise<T> {
  try {
    return await step;
  } catch {
    throw new Error(`Vosh could not read ${fileName}.`);
  }
}

async function bytesOf(file: Blob): Promise<number[]> {
  return Array.from(new Uint8Array(await file.arrayBuffer()));
}

function fileOf(entry: FileSystemFileEntry): Promise<File> {
  return new Promise((resolve, reject) => entry.file(resolve, reject));
}

/** Every entry in `dir`. A reader hands them over in batches and an
 *  empty batch ends them. */
async function entriesOf(dir: FileSystemDirectoryEntry): Promise<FileSystemEntry[]> {
  const reader = dir.createReader();
  const all: FileSystemEntry[] = [];
  for (;;) {
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
      reader.readEntries(resolve, reject),
    );
    if (batch.length === 0) return all;
    all.push(...batch);
  }
}

/** A .zip you picked or dropped. */
export async function zipPackage(file: File): Promise<PluginPackage> {
  if (file.size > MAX_BYTES) throw tooBig(file.name);
  return { fileName: file.name, bytes: await read(file.name, bytesOf(file)) };
}

/** A folder you dropped, with every file under it by its path from the
 *  folder's own name on, like `weather_pane/main.lua`. */
export async function folderPackage(dir: FileSystemDirectoryEntry): Promise<PluginPackage> {
  const found: { path: string; file: File }[] = [];
  let total = 0;
  const walk = async (folder: FileSystemDirectoryEntry, path: string) => {
    for (const entry of await read(dir.name, entriesOf(folder))) {
      const at = `${path}/${entry.name}`;
      if (entry.isDirectory) {
        await walk(entry as FileSystemDirectoryEntry, at);
        continue;
      }
      const file = await read(dir.name, fileOf(entry as FileSystemFileEntry));
      total += file.size;
      found.push({ path: at, file });
      if (found.length > MAX_FILES || total > MAX_BYTES) throw tooBig(dir.name);
    }
  };
  await walk(dir, dir.name);
  const files: DroppedFile[] = await Promise.all(
    found.map(async ({ path, file }) => ({ path, bytes: await read(dir.name, bytesOf(file)) })),
  );
  return { fileName: dir.name, files };
}

/** What you dropped: a folder, or a file Rust reads as a .zip. */
export async function droppedPackage(entry: FileSystemEntry): Promise<PluginPackage> {
  if (entry.isDirectory) return folderPackage(entry as FileSystemDirectoryEntry);
  return zipPackage(await read(entry.name, fileOf(entry as FileSystemFileEntry)));
}
