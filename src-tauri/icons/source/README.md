# Vosh app icon

Moonpath. Lysenties hangs over a sea of scrollback, and its light selects one or two words on every line, a single streak from the horizon down to your cursor.

This folder holds every master of the icon and the script that turns them into the files Tauri bundles. Never edit the files in `src-tauri/icons` by hand. Change a master here and run the build.

## Masters

- `vosh.icon/` is the Icon Composer document for macOS 26 and later, with `icon.json` and its layers in `Assets/`. It carries a Default look (blue hour) and a Dark look on the Kanso ground `#090e13`. macOS derives Clear and Tinted from them.
- `vosh-flat.svg` is every layer flattened on the Dark ground, on the full 1024 canvas. This is the approved design.
- `vosh-flat-default.svg` is the same on the Default ground.
- `vosh-tile.svg` is the Windows and Linux tile. It sets the Dark art inside the macOS squircle with a bone rim, so the icon keeps its edge on a dark taskbar.
- `vosh-macos-legacy.svg` is the Big Sur grid for `icon.icns`, an 824 px body inset 100 px in 1024 with a soft shadow in the margin.
- `vosh-16.svg`, `vosh-20.svg`, `vosh-24.svg` and `vosh-32.svg` are the tile drawn by hand on the pixel grid for `icon.ico` and `32x32.png`.
- `vosh-macos-16.svg` and `vosh-macos-32.svg` are the Big Sur grid drawn by hand on the pixel grid for `icon.icns` at 16 and 32.
- `generator/` holds the Python that writes `vosh.icon` and the four large SVG masters. `masters.py` runs it. `cand.py` holds the approved parameters (`FINAL`), `gen.py` draws the art, `layout_dp.py` picks the line breaks and the lit run of every row, `glyphs.py` turns JetBrains Mono into path data, and `moon_all.txt` lists the moon echoes from the game. `ico.py` packs `icon.ico`.
- `build-icons.sh` builds every generated file from the masters.
- `OFL-JetBrainsMono.txt` is the licence of the font behind the lettering.

The squircle in the tile, the legacy master and the small masters is the shape macOS 27 draws every app icon in, measured through Icon Services.

## Generated files

`build-icons.sh` writes these into `src-tauri/icons`.

- `Assets.car` serves macOS 26 and later. The bundler copies it into the app and sets `CFBundleIconName` to `Vosh`, so you get the Liquid Glass icon in every look.
- `icon.icns` is `CFBundleIconFile` for macOS 11 to 15, the DMG volume, and the Dock icon under `tauri dev`.
- `icon.ico` serves the Windows exe, window, taskbar and installers. It holds frames at 32, 16, 20, 24, 40, 48, 64 and 256. The 32 frame comes first because Tauri takes the first frame as the window icon.
- `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png` and `icon.png` are the Linux desktop icons, and `32x32.png` is also the Linux window icon.

`tauri icon` also writes Windows Store logos and iOS and Android sets. Vosh ships only for the desktop, so the script leaves them in its scratch folder.

`Assets.car` is committed prebuilt. Tauri can compile a `.icon` itself, but that path failed every time on macOS 27 with Xcode 26.5 when the icon was set up, while the same actool run by hand works.

## Rebuild

You need macOS with Xcode 26 or later, Python 3, and `npm install` done at the repo root.

```sh
src-tauri/icons/source/build-icons.sh
```

Run it after you change any master, then commit what changed. It renders every SVG with the resvg inside the local Tauri CLI, packs `icon.icns` with `iconutil`, packs `icon.ico` with `generator/ico.py`, and compiles `vosh.icon` with `actool`.

```sh
src-tauri/icons/source/build-icons.sh --art
```

This rebuilds the art masters with the Python generator first. Use it after you change `generator/cand.py` or `generator/gen.py`. It needs fontTools (`pip install fonttools`).

```sh
src-tauri/icons/source/build-icons.sh --check
```

This builds everything into a scratch folder and names each generated file that differs from the committed one. When fontTools is installed it checks the art masters against the generator too. It writes nothing. actool stamps every car with a new time and new rendition names, so the script compares `Assets.car` by its content, and a normal run keeps the committed car when nothing in it changed.

If actool fails with an `NSPlaceholderArray` exception, its helper is stuck. Quit it with `pkill -f 'ibtoold --sending-client-environment'` and run the script again.

To preview a look of the document without building an app, use `ictool` from Icon Composer.

```sh
"/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool" \
  src-tauri/icons/source/vosh.icon --export-image --output-file dark.png \
  --platform macOS --rendition Dark --width 1024 --height 1024 --scale 1
```

The renditions are `Default`, `Dark`, `ClearLight`, `ClearDark`, `TintedLight` and `TintedDark`.

## Lettering licence

The words in the art are JetBrains Mono outlines, turned into paths by `generator/glyphs.py` from the copy Vosh bundles in `public/fonts`. JetBrains Mono is under the SIL Open Font License 1.1. That licence governs the font, and it says the requirement to stay under the OFL does not apply to any document created using the font, so the icon stays under the Vosh GPL v3. The SVG masters carry the outlines, so the licence text and its copyright notice sit next to them in `OFL-JetBrainsMono.txt`. JetBrains Mono is a trademark of JetBrains s.r.o., and the icon never uses the name.
