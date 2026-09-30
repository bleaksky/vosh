"""Write every generated art master of the Vosh app icon into the source folder.

    python3 -B masters.py            write the masters next to this folder (src-tauri/icons/source)
    python3 -B masters.py --out DIR  write them into DIR instead
    python3 -B masters.py --check    build them in a temporary folder and report any that differ

Masters written:
    vosh.icon/                 Icon Composer document for macOS 26 and later (compiled into Assets.car)
    vosh-flat.svg              every layer flattened on the Dark ground, full 1024 canvas
    vosh-flat-default.svg      the same on the Default (blue hour) ground
    vosh-tile.svg              Windows and Linux tile: the Dark art in the macOS squircle with a bone rim
    vosh-macos-legacy.svg      Big Sur grid for icon.icns: the tile at 824 px inset 100 px, with a soft shadow

The small masters (vosh-16.svg and friends) are drawn by hand and are not written here.
Needs Python 3 with fontTools. The glyph outlines come from src/assets/fonts."""
import filecmp
import os
import shutil
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen  # noqa: E402
from cand import FINAL  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.dirname(HERE)

# The macOS app icon squircle, measured on macOS 27 from an Icon Composer canvas rendered through Icon Services
# (the same shape frames legacy icons on the Big Sur grid). Top left corner, in units of the box side, from the
# point where the left edge stops being straight up to the diagonal. The other half of the corner mirrors it.
CORNER = [
    (0.00000, 0.32568), (0.00001, 0.32471), (0.00003, 0.32275), (0.00015, 0.31689), (0.00039, 0.31104),
    (0.00069, 0.30518), (0.00092, 0.29932), (0.00101, 0.29346), (0.00127, 0.28760), (0.00163, 0.28174),
    (0.00191, 0.27588), (0.00206, 0.27002), (0.00247, 0.26416), (0.00288, 0.25830), (0.00314, 0.25244),
    (0.00373, 0.24658), (0.00409, 0.24072), (0.00479, 0.23486), (0.00536, 0.22900), (0.00602, 0.22314),
    (0.00687, 0.21729), (0.00780, 0.21143), (0.00880, 0.20557), (0.00989, 0.19971), (0.01115, 0.19385),
    (0.01257, 0.18799), (0.01401, 0.18213), (0.01567, 0.17627), (0.01748, 0.17041), (0.01943, 0.16455),
    (0.02155, 0.15869), (0.02385, 0.15283), (0.02635, 0.14697), (0.02904, 0.14111), (0.03194, 0.13525),
    (0.03506, 0.12939), (0.03845, 0.12354), (0.04206, 0.11768), (0.04594, 0.11182), (0.05012, 0.10596),
    (0.05453, 0.10010), (0.05928, 0.09424), (0.06438, 0.08838), (0.06978, 0.08252), (0.07549, 0.07666),
]

# Tile edge. The Kanso ground has a contrast of about 1.19 against a dark Windows taskbar (#202020), so the tile
# carries a rim in the horizon's bone, brighter at the top where the moon is, that holds the silhouette there.
RIM = dict(width=14.0, top=0.40, bottom=0.24)
# Big Sur grid: 824 px body inset 100 px in 1024. The rim is a touch lighter there, since the Dock and Finder
# sit on their own grounds, and the shadow sits in the margin (macOS 26 and later draw their own and ignore it).
GRID = dict(inset=100, body=824, rim_width=12.0, rim_top=0.32, rim_bottom=0.18,
            shadow_dy=10, shadow_blur=12, shadow_op=0.32)


def squircle(x0, y0, s):
    """The squircle filling the square at (x0, y0) with side s, as SVG path data."""
    half = CORNER + [(y, x) for x, y in reversed(CORNER)]                            # up the left edge, onto the top
    pts = [(x0 + px * s, y0 + py * s) for px, py in half]                            # top left
    pts += [(x0 + s - py * s, y0 + px * s) for px, py in half]                       # top right
    pts += [(x0 + s - px * s, y0 + s - py * s) for px, py in half]                   # bottom right
    pts += [(x0 + py * s, y0 + s - px * s) for px, py in half]                       # bottom left
    return "M" + "L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + "Z"


def split(svg):
    """(defs, body) of an SVG written by gen.svg."""
    inner = svg[svg.index(">") + 1:svg.rindex("</svg>")]
    if inner.startswith("<defs>"):
        end = inner.index("</defs>")
        return inner[len("<defs>"):end], inner[end + len("</defs>"):]
    return "", inner


def rim_gradient(gid, top, bottom, y0, y1):
    return (f'<linearGradient id="{gid}" x1="0" y1="{y0}" x2="0" y2="{y1}" gradientUnits="userSpaceOnUse">'
            f'<stop offset="0" stop-color="{gen.BONE}" stop-opacity="{top:.3f}"/>'
            f'<stop offset="1" stop-color="{gen.BONE}" stop-opacity="{bottom:.3f}"/></linearGradient>')


def tile(flat):
    """Windows and Linux tile master: the Dark flat art inside the squircle, with the rim along the inside edge."""
    defs, body = split(flat)
    sq = squircle(0, 0, 1024)
    defs += f'<clipPath id="tile"><path d="{sq}"/></clipPath>'
    defs += rim_gradient("rim", RIM["top"], RIM["bottom"], 0, 1024)
    body = (f'<g clip-path="url(#tile)">{body}'
            f'<path d="{sq}" fill="none" stroke="url(#rim)" stroke-width="{2 * RIM["width"]:.1f}"/></g>')
    return gen.svg(body, defs)


def legacy(flat):
    """macOS legacy master on the Big Sur grid, the source of icon.icns."""
    defs, body = split(flat)
    i, b = GRID["inset"], GRID["body"]
    k = b / 1024
    sq = squircle(0, 0, 1024)
    rim = GRID["rim_width"] / k                 # the rim is measured on the 1024 canvas, the art is scaled by k
    defs += f'<clipPath id="tile"><path d="{sq}"/></clipPath>'
    defs += rim_gradient("rim", GRID["rim_top"], GRID["rim_bottom"], 0, 1024)
    defs += (f'<filter id="shadow" x="-10%" y="-10%" width="120%" height="120%">'
             f'<feGaussianBlur stdDeviation="{GRID["shadow_blur"]}"/></filter>')
    shadow = (f'<path d="{squircle(i, i + GRID["shadow_dy"], b)}" fill="#000000" '
              f'fill-opacity="{GRID["shadow_op"]:.2f}" filter="url(#shadow)"/>')
    art = (f'<g transform="translate({i} {i}) scale({k:.7f})"><g clip-path="url(#tile)">{body}'
           f'<path d="{sq}" fill="none" stroke="url(#rim)" stroke-width="{2 * rim:.2f}"/></g></g>')
    return gen.svg(shadow + art, defs)


def build(out):
    p = gen.P(**FINAL)
    work = tempfile.mkdtemp(prefix="vosh-icon-")
    try:
        assets, _ = gen.write_icon(p, os.path.join(work, "vosh.icon"))
        flat = gen.flat_master(p, assets)
        files = {
            "vosh-flat.svg": flat,
            "vosh-flat-default.svg": gen.flat_master(p, assets, light=True),
            "vosh-tile.svg": tile(flat),
            "vosh-macos-legacy.svg": legacy(flat),
        }
        for name, text in files.items():
            with open(os.path.join(work, name), "w") as f:
                f.write(text)
        os.makedirs(out, exist_ok=True)
        for name in sorted(os.listdir(work)):
            src, dst = os.path.join(work, name), os.path.join(out, name)
            if os.path.isdir(src):
                shutil.copytree(src, dst, dirs_exist_ok=True)
            else:
                shutil.copyfile(src, dst)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def check():
    tmp = tempfile.mkdtemp(prefix="vosh-icon-check-")
    try:
        build(tmp)
        bad = []
        for root, _, names in os.walk(tmp):
            for n in names:
                rel = os.path.relpath(os.path.join(root, n), tmp)
                ref = os.path.join(SOURCE, rel)
                if not os.path.exists(ref) or not filecmp.cmp(os.path.join(root, n), ref, shallow=False):
                    bad.append(rel)
        for rel in sorted(bad):
            print(f"differs: {rel}")
        print("masters match" if not bad else f"{len(bad)} masters differ")
        return 1 if bad else 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    args = sys.argv[1:]
    if args[:1] == ["--check"]:
        sys.exit(check())
    out = args[1] if args[:1] == ["--out"] and len(args) > 1 else SOURCE
    build(out)
    print(f"masters written to {out}")
