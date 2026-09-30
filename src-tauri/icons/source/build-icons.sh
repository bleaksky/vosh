#!/usr/bin/env bash
# Build every generated file in src-tauri/icons from the masters in this folder.
#
#   src-tauri/icons/source/build-icons.sh           write the generated files
#   src-tauri/icons/source/build-icons.sh --art     rebuild the art masters with the Python generator first
#   src-tauri/icons/source/build-icons.sh --check   build into a scratch folder and report what differs,
#                                                   the art masters too when fontTools is installed
#
# You need macOS with Xcode 26 or later (actool compiles Assets.car, iconutil packs icon.icns), Python 3,
# and the npm dependencies (the local tauri CLI renders every SVG with resvg). --art also needs fontTools.
#
# actool writes a fresh timestamp and fresh rendition names into Assets.car on every run, so the car is never
# identical byte for byte. The script compares cars by their content (assetutil --info without those fields)
# and keeps the committed car when nothing in it changed.
set -euo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICONS="$(dirname "$SRC")"
REPO="$(cd "$ICONS/../.." && pwd)"
TAURI="$REPO/node_modules/.bin/tauri"
GROUND="#090e13"
export PYTHONDONTWRITEBYTECODE=1

ART=0
CHECK=0
for arg in "$@"; do
  case "$arg" in
    --art) ART=1 ;;
    --check) CHECK=1 ;;
    *) echo "unknown option $arg" >&2; exit 2 ;;
  esac
done
[ "$ART$CHECK" != 11 ] || { echo "--check never writes, so run --check on its own" >&2; exit 2; }

[ "$(uname)" = Darwin ] || { echo "build-icons.sh needs macOS (actool and iconutil)" >&2; exit 1; }
[ -x "$TAURI" ] || { echo "run npm install first, $TAURI is missing" >&2; exit 1; }
ACTOOL_MAJOR="$(xcrun actool --version | python3 -c '
import plistlib, sys
print(plistlib.loads(sys.stdin.buffer.read())["com.apple.actool.version"]["short-bundle-version"].split(".")[0])')"
[ "$ACTOOL_MAJOR" -ge 26 ] || { echo "Assets.car needs actool 26 or later (Xcode 26), found $ACTOOL_MAJOR" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/vosh-icons.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

if [ "$ART" = 1 ]; then
  python3 "$SRC/generator/masters.py"
fi

if [ "$CHECK" = 1 ]; then OUT="$WORK/out"; else OUT="$ICONS"; fi
mkdir -p "$OUT"

# render <svg> <out dir> <size>... : one PNG per size, named <size>x<size>.png
render() {
  local svg="$1" dir="$2"
  shift 2
  local args=()
  for s in "$@"; do args+=(-p "$s"); done
  tauri icon "$svg" "${args[@]}" -o "$dir"
}

# tauri <args>... : the local tauri CLI, run from the repo, quiet unless it fails
tauri() {
  (cd "$REPO" && "$TAURI" "$@") > "$WORK/tauri.log" 2>&1 || { cat "$WORK/tauri.log" >&2; exit 1; }
}

# 1. The tauri set from the Windows and Linux tile: PNGs, Square logos, StoreLogo, ios and android.
#    Its icon.icns and icon.ico are replaced below.
tauri icon "$SRC/vosh-tile.svg" --ios-color "$GROUND" -o "$WORK/tauri"
for f in 64x64.png 128x128.png 128x128@2x.png icon.png StoreLogo.png "$WORK"/tauri/Square*Logo.png; do
  cp "$WORK/tauri/$(basename "$f")" "$OUT/"
done
mkdir -p "$OUT/ios" "$OUT/android"
cp -R "$WORK/tauri/ios/." "$OUT/ios/"
cp -R "$WORK/tauri/android/." "$OUT/android/"

# 2. Hand drawn small masters and the tile at the sizes the .ico needs.
for s in 16 20 24 32; do render "$SRC/vosh-$s.svg" "$WORK/small" "$s"; done
render "$SRC/vosh-tile.svg" "$WORK/tile" 40 48 64 256
cp "$WORK/small/32x32.png" "$OUT/32x32.png"

# 3. icon.ico. 32 first, since tauri takes the first frame as the Windows window icon.
python3 "$SRC/generator/ico.py" "$OUT/icon.ico" \
  "$WORK/small/32x32.png" "$WORK/small/16x16.png" "$WORK/small/20x20.png" "$WORK/small/24x24.png" \
  "$WORK/tile/40x40.png" "$WORK/tile/48x48.png" "$WORK/tile/64x64.png" "$WORK/tile/256x256.png"

# 4. icon.icns from the Big Sur grid masters: hand drawn at 16 and 32, the legacy master above.
render "$SRC/vosh-macos-16.svg" "$WORK/mac" 16
render "$SRC/vosh-macos-32.svg" "$WORK/mac" 32
render "$SRC/vosh-macos-legacy.svg" "$WORK/mac" 64 128 256 512 1024
SET="$WORK/vosh.iconset"
mkdir -p "$SET"
cp "$WORK/mac/16x16.png" "$SET/icon_16x16.png"
cp "$WORK/mac/32x32.png" "$SET/icon_16x16@2x.png"
cp "$WORK/mac/32x32.png" "$SET/icon_32x32.png"
cp "$WORK/mac/64x64.png" "$SET/icon_32x32@2x.png"
cp "$WORK/mac/128x128.png" "$SET/icon_128x128.png"
cp "$WORK/mac/256x256.png" "$SET/icon_128x128@2x.png"
cp "$WORK/mac/256x256.png" "$SET/icon_256x256.png"
cp "$WORK/mac/512x512.png" "$SET/icon_256x256@2x.png"
cp "$WORK/mac/512x512.png" "$SET/icon_512x512.png"
cp "$WORK/mac/1024x1024.png" "$SET/icon_512x512@2x.png"
iconutil -c icns "$SET" -o "$OUT/icon.icns"

# 5. Assets.car from vosh.icon. The --app-icon name must match the document name, and it becomes
#    CFBundleIconName. stdin stays attached, since actool fails when its helper starts with stdin closed.
CAR="$WORK/car"
mkdir -p "$CAR/out"
cp -R "$SRC/vosh.icon" "$CAR/Vosh.icon"
if ! xcrun actool "$CAR/Vosh.icon" --compile "$CAR/out" --app-icon Vosh --include-all-app-icons \
  --platform macosx --target-device mac --minimum-deployment-target 11.0 \
  --output-partial-info-plist "$CAR/partial.plist" --output-format human-readable-text \
  --notices --warnings --enable-on-demand-resources NO --development-region en < /dev/null > "$CAR/actool.log" 2>&1; then
  cat "$CAR/actool.log" >&2
  echo "actool failed. If the log shows an NSPlaceholderArray exception, quit the stuck helper with" >&2
  echo "pkill -f 'ibtoold --sending-client-environment' and run the script again." >&2
  exit 1
fi

# car_content <car> : assetutil --info without the fields actool changes on every run
car_content() {
  xcrun assetutil --info "$1" | python3 -c '
import json, re, sys
info = json.load(sys.stdin)
for entry in info:
    entry.pop("Timestamp", None)
    name = entry.get("RenditionName", "")
    if re.search(r"_[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}-", name):
        entry["RenditionName"] = re.sub(r"_[0-9A-F]{8}-.*(\.[a-z]+)$", r"\1", name)
        entry.pop("SHA1Digest", None)
print(json.dumps(info, indent=1, sort_keys=True))'
}

car_content "$CAR/out/Assets.car" > "$CAR/new.json"
if [ -f "$ICONS/Assets.car" ] && car_content "$ICONS/Assets.car" > "$CAR/old.json" && cmp -s "$CAR/new.json" "$CAR/old.json"; then
  CAR_SAME=1
else
  CAR_SAME=0
fi
if [ "$CHECK" = 1 ] || [ "$CAR_SAME" = 0 ]; then
  cp "$CAR/out/Assets.car" "$OUT/Assets.car"
fi

GENERATED=(Assets.car icon.icns icon.ico 32x32.png 64x64.png 128x128.png 128x128@2x.png icon.png StoreLogo.png)
for f in "$WORK"/tauri/Square*Logo.png "$WORK"/tauri/ios/* "$WORK"/tauri/android/*/*; do
  GENERATED+=("${f#"$WORK"/tauri/}")
done

if [ "$CHECK" = 1 ]; then
  DIFF=0
  for f in "${GENERATED[@]}"; do
    if [ "$f" = Assets.car ]; then
      [ "$CAR_SAME" = 1 ] || { echo "differs: $f"; DIFF=1; }
    elif ! cmp -s "$OUT/$f" "$ICONS/$f"; then
      echo "differs: $f"
      DIFF=1
    fi
  done
  if [ "$DIFF" = 0 ]; then echo "all ${#GENERATED[@]} generated files match"; fi
  if python3 -c 'import fontTools' 2> /dev/null; then
    python3 "$SRC/generator/masters.py" --check || DIFF=1
  else
    echo "fontTools is missing, so the art masters were not checked"
  fi
  exit "$DIFF"
fi

if [ "$CAR_SAME" = 1 ]; then echo "Assets.car content unchanged, kept the committed car"; fi
echo "wrote ${#GENERATED[@]} files in $ICONS"
