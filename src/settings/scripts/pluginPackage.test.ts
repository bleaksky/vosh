import { describe, expect, it } from 'vitest';
import { droppedPackage, folderPackage, zipPackage } from './pluginPackage';

// What Install reads from a .zip or a dropped folder. The entries stand
// in for the ones a drop hands the page, and each folder reader hands
// its entries over two at a time, as a reader does in batches.

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));

function fileEntry(name: string, content: BlobPart = '') {
  return {
    isFile: true,
    isDirectory: false,
    name,
    file: (ok: (file: File) => void) => ok(new File([content], name)),
  } as unknown as FileSystemFileEntry;
}

function folderEntry(name: string, children: FileSystemEntry[]) {
  return {
    isFile: false,
    isDirectory: true,
    name,
    createReader: () => {
      let at = 0;
      return {
        readEntries: (ok: (batch: FileSystemEntry[]) => void) => {
          ok(children.slice(at, at + 2));
          at += 2;
        },
      };
    },
  } as unknown as FileSystemDirectoryEntry;
}

const MANIFEST = '[plugin]\nname = "weather_pane"\nversion = "0.2.0"\nauthor = "Tolliver"\n';
const MAIN = '-- weather_pane\n';
const UTIL = 'return {}\n';

describe('a dropped folder', () => {
  it('reads every file under it by its path from the folder on', async () => {
    const dropped = folderEntry('weather_pane', [
      fileEntry('manifest.toml', MANIFEST),
      fileEntry('main.lua', MAIN),
      folderEntry('lib', [fileEntry('util.lua', UTIL)]),
    ]);
    expect(await droppedPackage(dropped)).toEqual({
      fileName: 'weather_pane',
      files: [
        { path: 'weather_pane/manifest.toml', bytes: bytes(MANIFEST) },
        { path: 'weather_pane/main.lua', bytes: bytes(MAIN) },
        { path: 'weather_pane/lib/util.lua', bytes: bytes(UTIL) },
      ],
    });
  });

  it('reads an empty folder as no files, which Rust answers', async () => {
    expect(await folderPackage(folderEntry('weather_pane', []))).toEqual({
      fileName: 'weather_pane',
      files: [],
    });
  });

  it('stops at more than 200 files with the sentence Rust gives', async () => {
    const many = Array.from({ length: 201 }, (_, i) => fileEntry(`${i}.lua`));
    await expect(folderPackage(folderEntry('weather_pane', many))).rejects.toThrow(
      'Vosh did not install weather_pane. It holds more than 5 MB or 200 files.',
    );
  });

  it('says Vosh could not read it when the browser cannot read a file in it', async () => {
    const gone = {
      isFile: true,
      isDirectory: false,
      name: 'main.lua',
      file: (_ok: unknown, fail: (e: Error) => void) => fail(new Error('NotFoundError')),
    } as unknown as FileSystemFileEntry;
    await expect(folderPackage(folderEntry('weather_pane', [gone]))).rejects.toThrow(
      'Vosh could not read weather_pane.',
    );
  });

  it('stops at more than 5 MB', async () => {
    const big = fileEntry('map.dat', new Uint8Array(5 * 1024 * 1024 + 1));
    await expect(folderPackage(folderEntry('weather_pane', [big]))).rejects.toThrow(
      'Vosh did not install weather_pane. It holds more than 5 MB or 200 files.',
    );
  });
});

describe('a .zip', () => {
  it('goes to Rust as its bytes under its file name, picked or dropped', async () => {
    const zip = new File([MAIN], 'weather_pane.zip');
    const read = { fileName: 'weather_pane.zip', bytes: bytes(MAIN) };
    expect(await zipPackage(zip)).toEqual(read);
    expect(await droppedPackage(fileEntry('weather_pane.zip', MAIN))).toEqual(read);
  });

  it('is refused unread over 5 MB', async () => {
    const zip = new File([new Uint8Array(5 * 1024 * 1024 + 1)], 'weather_pane.zip');
    await expect(zipPackage(zip)).rejects.toThrow(
      'Vosh did not install weather_pane.zip. It holds more than 5 MB or 200 files.',
    );
  });
});
