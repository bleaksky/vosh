"""Moonpath, the Vosh app icon. Builds the .icon layers and the flat master SVGs from one parameter set.

masters.py is the entry point. It takes the approved parameter set from cand.py (FINAL) and writes every
master into the folder above this one. This module holds the art.

How the art is made:
- JetBrains Mono outlines (OFL, bundled with Vosh) set the text.
- The readable scrollback is Lysenties's own moon.c echoes, in the order the game sends them. The far rows
  add calm echoes that name no other moon (pool "quiet", moon_all.txt holds every moon.c echo).
- Line breaks and the lit run of each row are chosen together by dynamic programming, so the path is one
  selection per row (one or two whole words, the space between them lit too, as a terminal selects) whose
  centre sits on the moon's axis and whose width follows one even taper.
- The dim scrollback runs up to the horizon, fading as it goes, and far rows keep a minimum glyph height
  so the far lit dashes sit inside lines of text.
"""
import json, math, os, re, shutil, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from glyphs import word_path

HERE = os.path.dirname(os.path.abspath(__file__))
ICTOOL = "/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool"
LOOKS = ["Default", "Dark", "ClearLight", "ClearDark", "TintedLight", "TintedDark"]

# Kanso Zen
GROUND = "#090e13"; BONE = "#c5c9c7"; ACC = "#b0c8d4"

# JetBrains Mono on a 0.6 x 1.2 em cell. Line pitch stays 1.2 em. The baseline sits 0.94 em below the
# cell top, so caps (0.73) and the g descender (0.18) both fit inside a reverse video cell.
CELL_W, LINE_H, ASC = 0.6, 1.2, 0.94

# moon.c, Lysenties only, colour codes stripped, in the order the game sends them:
# the return (lines 90 to 93), the full moon (125 to 127) and the wane (132).
MESSAGES_ALL = [
    "A pale light ignites in the eastern sky, slow as dawn.",
    "Where there has been nothing for an age, a new sphere takes shape. Silver, unwavering, warm.",
    "Lysenties, the Silver Moon, has returned.",
    "Warmth spreads across the continent. Every hearth fire flares for one breath. Every healer's hands tingle as though newly filled.",
    "Lysenties climbs silver and high, and the night softens at its edges.",
    "A warm pressure settles over the wounded. Stitches pull tighter. Fevers break without reason.",
    "In graves and crypts, the dead feel the silver like a brand pressed against their stillness.",
    "Lysenties dims toward the horizon, smaller now. The silver weight lifts and healing finds its old pace again.",
]
MESSAGES = list(MESSAGES_ALL)
# moon.c echoes of other moons that name no other moon and keep a calm night: they may fill readable rows too
# (indices into moon_all.txt: a faint hum, a low tone, the sky has never looked like this, a chill settles,
# cold stones, the weave of magic, every caster, whatever came close in the dark retreats)
CALM = [1, 14, 15, 18, 19, 26, 27, 36]


def col(h, a=1.0):
    h = h.lstrip('#')
    r, g, b = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return f"srgb:{r:.5f},{g:.5f},{b:.5f},{a:.5f}"


def mix(a, b, t):
    a = [int(a.lstrip('#')[i:i + 2], 16) for i in (0, 2, 4)]
    b = [int(b.lstrip('#')[i:i + 2], 16) for i in (0, 2, 4)]
    return '#' + ''.join(f"{round(x + (y - x) * t):02x}" for x, y in zip(a, b))


def svg(body, defs=""):
    d = f"<defs>{defs}</defs>" if defs else ""
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">{d}{body}</svg>'


def lit_path(cx, cy, R, phase, term=0.4):
    """moonPhase.ts, scaled: the lit part as one closed shape."""
    ang = phase * math.pi * 2 / 8
    c = math.cos(ang)
    half = abs(c) < 1e-9
    trx = 0 if half else R * term
    top = f"{cx:.2f} {cy - R:.2f}"; bot = f"{cx:.2f} {cy + R:.2f}"
    ls = 1 if phase < 4 else 0
    ts = (1 - ls) if c > 0 else ls
    t = f"L{top}" if half else f"A{trx:.2f} {R:.2f} 0 0 {ts} {top}"
    return f"M{top}A{R:.2f} {R:.2f} 0 0 {ls} {bot}{t}Z"


def rect(x, y, w, h, fill, op=1.0):
    return f'<rect x="{x:.2f}" y="{y:.2f}" width="{w:.2f}" height="{h:.2f}" fill="{fill}" fill-opacity="{op:.3f}"/>'


DEFAULT = dict(
    # geometry
    yh=486, ybot=930,
    ax=1.0, ay=0.6, s_bot=0.5, s_pitch=0.75, s_typed=0.72, min_emy=0.15, min_pitch=14.0,
    emy_floor=3.0,                # far rows keep glyphs at least this tall (em in px) so they stay lines of text
    letters_min=2.0,              # below this em height a row's letters cannot be seen, so its lit run is a plain bar
    streak_min=2.2,
    x0=80, xwrap=984,
    mx=512, my=235, R=168, phase=3,
    weight="Regular", lit_weight="Regular", cell_fill=1.0,
    # the path: one lit run per row, whole words, centred on the axis, width on one taper
    w_top=26.0, w_bot=210.0, w_curve=1.0,   # target run width at the horizon and at the nearest scrollback row
    tol_c=6.0, tol_c_cw=0.3, tol_w=0.10, pad=True, pad_cost=0.6, read_min=6.0, near_tight=1.0, topic_lit=40.0, topic_dim=4.0, jitter=0.025, jitter_cost=0.3, far_words=999, mid_words=4, pool="lys", lit_band=0.0, knock_far=False,
    w_weight=1.0, sb_scale=1.0, near_core_emy=8.0, near_core_cost=2.0, ref_cw=12.0, size_pow=1.5, vocab_cw=15.0, vocab_bonus=0.0, step_tol=0.6, step_px=4.0, shrink_px=2.0, shrink_cost=20.0, step_weight=1.0, wq=3.0, beam=4000,        # centre tolerance in px, width tolerance as a share of the target
    max_words=2, early=2, min_fill=0.62,
    glade_top=0.92, glade_bot=0.78,
    glade_col="#e2e6e4", sea_col=BONE, sea_op=0.16,
    sea_fade=0.45, sea_hz=0.50, sea_gamma=1.0,   # dim text fades toward the horizon to sea_hz of its near level
    wash=0.30, wash_w=1.25,
    # moon
    moon_col="#e4e7e5", disc_op=0.2,
    # cursor
    cur_col=ACC, cur_h=1.0,
    input_gap=0.35,
    typed="look moon", typed_op=0.55,
    # sky
    sky_top=0.06, sky_hz=0.10, sky_curve=1.0, glow=0.10, glow_r=0.55,
    hair_op=0.30, hair_h=2.4, glint=0.5,
    # light look
    light=dict(sky_top="#7f98a8", sky_hz="#dbe4e9", sea="#566b7a", sea_bot="#445663",
               sea_op=0.22, moon_col="#ffffff", disc_col="#ffffff", disc_op=0.28,
               glade_col="#ffffff", hair="#ffffff", hair_op=0.55, wash=0.16, typed_op=0.5),
    fill_default="#566b7a",
)


def fit_typed(p):
    """The typed line runs from the margin and ends one space before the cursor, whose cell centre is the axis."""
    phrase = p["typed"].split()
    n = sum(len(w) for w in phrase) + len(phrase) - 1
    cw = (p["mx"] - p["x0"]) / (n + 1 + 0.5)
    return phrase, cw


def rows_geometry(p):
    yh = p["yh"]
    phrase, cw_t = fit_typed(p)
    em_t = cw_t / CELL_W
    d_t = p["ybot"] - yh

    def ems(d, k=1.0):
        ex = k * em_t * (d / d_t) ** p["ax"]
        f = (d / d_t) ** p["ay"]
        return ex, ex * p["s_bot"] * f, ex * p["s_pitch"] * f
    ex, ey, ep = ems(d_t)
    typed = dict(y=p["ybot"], d=d_t, emx=ex, emy=ex * p["s_typed"], cw=cw_t, words=[], typed=True)
    x = p["x0"]
    for w in phrase:
        typed["words"].append((x, x + len(w) * cw_t, w)); x += (len(w) + 1) * cw_t
    typed["cursor_x"] = x
    rows = []
    y = p["ybot"] - LINE_H * ep * (1 + p["input_gap"])
    while True:
        d = y - yh
        if d <= 0:
            break
        ex, ey, ep = ems(d, p["sb_scale"])
        if ey < p["min_emy"]:
            break
        rows.append(dict(y=y, d=d, emx=ex, emy=ey, cw=ex * CELL_W, words=[], lit=None))
        y -= max(LINE_H * ep, p["min_pitch"])
    rows.reverse()
    return rows, typed


def pool(p):
    """The message pool and which of its messages are Lysenties's own. "mix": every moon.c echo may fill
    the rows too far out to read, and the readable rows take only Lysenties's."""
    if p["pool"] in ("all", "mix", "calm", "quiet"):
        allm = [l.strip() for l in open(os.path.join(HERE, "moon_all.txt")) if l.strip()]
        core = [i for i, m in enumerate(allm) if m in MESSAGES_ALL]
        if p["pool"] == "all":
            return allm, None, None
        if p["pool"] == "mix":
            return allm, core, core
        if p["pool"] == "calm":
            return allm, sorted(set(core) | set(CALM)), core
        # "quiet": nothing on the water names another moon, not even in rows too small to read
        keep = sorted(set(core) | set(CALM))
        msgs = [allm[i] for i in keep]
        return msgs, None, [k for k, i in enumerate(keep) if i in core]
    return list(MESSAGES_ALL), None, None


def layout(p):
    from layout_dp import layout_rows
    rows, typed = rows_geometry(p)
    msgs, lys, core = pool(p)
    total = layout_rows(p, rows, msgs, lys, core)
    return rows + [typed], total


def cell(p, r):
    emy = max(r["emy"], p["emy_floor"])
    return emy, LINE_H * emy * p["cell_fill"]


def glyphs(p, r, xa, w, weight=None):
    emy, ch = cell(p, r)
    top = r["y"] - LINE_H * emy / 2
    return word_path(w, xa, top + ASC * emy, r["emx"], emy, weight or p["weight"])


def bands(p, rows):
    """Each scrollback row's full line box, from the midpoint to the row above to the midpoint to the row
    below, as a terminal draws a selection: consecutive selected lines touch."""
    sb = [r for r in rows if not r.get("typed")]
    for k, r in enumerate(sb):
        up = sb[k - 1]["y"] if k else None
        dn = sb[k + 1]["y"] if k + 1 < len(sb) else None
        top = (up + r["y"]) / 2 if up is not None else r["y"] - (dn - r["y"]) / 2
        bot = (dn + r["y"]) / 2 if dn is not None else r["y"] + (r["y"] - up) / 2
        top = max(top, p["yh"] + p["hair_h"])
        h = (bot - top) * p["lit_band"]
        c = (top + bot) / 2
        r["band"] = (c - h / 2, c + h / 2)


def build_art(p):
    rows, total = layout(p)
    if p["lit_band"]:
        bands(p, rows)
    yh, mx = p["yh"], p["mx"]
    D = rows[-2]["d"]
    sea, glade, cur = [], [], []
    for r in rows:
        u = min(1.0, r["d"] / D)
        peak = p["glade_top"] + (p["glade_bot"] - p["glade_top"]) * u
        legible = r["emy"] >= p["letters_min"]
        if r.get("typed"):
            d = "".join(glyphs(p, r, xa, w) for xa, xb, w in r["words"])
            sea.append(f'<path d="{d}" fill="TYPED" fill-opacity="1.000"/>')
            ch = LINE_H * r["emy"] * p["cell_fill"] * p["cur_h"]
            cur.append(rect(r["cursor_x"], r["y"] - ch / 2, r["cw"], ch, p["cur_col"]))
            continue
        i, j, pl, pr = r["lit"]
        # dim text: fades toward the horizon but reaches it
        t = min(1.0, u / p["sea_fade"]) ** p["sea_gamma"]
        fade = p["sea_hz"] + (1 - p["sea_hz"]) * t
        dim = "".join(glyphs(p, r, xa, w) for k, (xa, xb, w) in enumerate(r["words"]) if not (i <= k <= j))
        if dim:
            sea.append(f'<path d="{dim}" fill="SEA" fill-opacity="{fade:.3f}"/>')
        a, b = r["words"][i][0] - pl * r["cw"], r["words"][j][1] + pr * r["cw"]
        emy, ch = cell(p, r)
        if "band" in r:
            top, bot = r["band"]
        else:
            h = ch if legible else max(ch, p["streak_min"])
            top, bot = r["y"] - h / 2, r["y"] + h / 2
        box = f"M{a:.2f} {top:.2f}H{b:.2f}V{bot:.2f}H{a:.2f}Z"
        knock = legible or p["knock_far"]
        letters = "".join(glyphs(p, r, xa, w, p["lit_weight"]) for xa, xb, w in r["words"][i:j + 1]) if knock else ""
        glade.append(f'<path d="{box}{letters}" fill-rule="evenodd" fill="{p["glade_col"]}" fill-opacity="{peak:.3f}"/>')
    return rows, sea, glade, cur


def sky_svg(p, light=False):
    yh, mx, my = p["yh"], p["mx"], p["my"]
    L = p["light"]
    stops = []
    for j in range(9):
        t = j / 8
        e = t ** p["sky_curve"]
        if light:
            stops.append(f'<stop offset="{t:.3f}" stop-color="{mix(L["sky_top"], L["sky_hz"], e)}"/>')
        else:
            stops.append(f'<stop offset="{t:.3f}" stop-color="{ACC}" stop-opacity="{p["sky_top"] + (p["sky_hz"] - p["sky_top"]) * e:.4f}"/>')
    defs = f'<linearGradient id="sk" x1="0" y1="0" x2="0" y2="1">{"".join(stops)}</linearGradient>'
    glow_c = "#ffffff" if light else ACC
    defs += (f'<radialGradient id="gl" cx="{mx}" cy="{my}" r="{p["glow_r"] * 1024:.0f}" gradientUnits="userSpaceOnUse">'
             f'<stop offset="0" stop-color="{glow_c}" stop-opacity="{p["glow"] * (1.4 if light else 1):.3f}"/>'
             f'<stop offset="0.5" stop-color="{glow_c}" stop-opacity="{p["glow"] * 0.35 * (1.4 if light else 1):.3f}"/>'
             f'<stop offset="1" stop-color="{glow_c}" stop-opacity="0"/></radialGradient>')
    body = f'<rect x="0" y="0" width="1024" height="{yh}" fill="url(#sk)"/>'
    body += f'<rect x="0" y="0" width="1024" height="{yh}" fill="url(#gl)"/>'
    wc = "#ffffff" if light else ACC
    wa = L["wash"] if light else p["wash"]
    ww = p["wash_w"] * p["R"]
    defs += (f'<radialGradient id="wa" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" '
             f'gradientTransform="translate({mx} {yh}) scale({ww:.1f} {1024 - yh + 60})">'
             f'<stop offset="0" stop-color="{wc}" stop-opacity="{wa:.3f}"/>'
             f'<stop offset="0.45" stop-color="{wc}" stop-opacity="{wa * 0.45:.3f}"/>'
             f'<stop offset="1" stop-color="{wc}" stop-opacity="0"/></radialGradient>')
    body += f'<rect x="0" y="{yh}" width="1024" height="{1024 - yh}" fill="url(#wa)"/>'
    hc, ho = (L["hair"], L["hair_op"]) if light else (BONE, p["hair_op"])
    body += rect(0, yh - p["hair_h"] / 2, 1024, p["hair_h"], hc, ho)
    gw = max(p["w_top"] * 2.2, 60)
    defs += (f'<linearGradient id="gt" x1="{mx - gw}" y1="0" x2="{mx + gw}" y2="0" gradientUnits="userSpaceOnUse">'
             f'<stop offset="0" stop-color="{hc}" stop-opacity="0"/><stop offset="0.5" stop-color="{hc}" stop-opacity="{p["glint"]:.2f}"/>'
             f'<stop offset="1" stop-color="{hc}" stop-opacity="0"/></linearGradient>')
    body += f'<rect x="{mx - gw:.1f}" y="{yh - p["hair_h"] / 2:.1f}" width="{2 * gw:.1f}" height="{p["hair_h"]:.1f}" fill="url(#gt)"/>'
    return body, defs


def assets_for(p):
    rows, sea, glade, cur = build_art(p)
    L = p["light"]
    mx, my, R = p["mx"], p["my"], p["R"]
    sky_d, sky_defs = sky_svg(p)
    sky_l, sky_ldefs = sky_svg(p, light=True)

    def seacol(s, colr, op):
        return re.sub(r'fill="SEA" fill-opacity="([0-9.]+)"', lambda m: f'fill="{colr}" fill-opacity="{float(m.group(1)) * op:.3f}"', s)

    def typed(s, colr, op):
        return s.replace('fill="TYPED" fill-opacity="1.000"', f'fill="{colr}" fill-opacity="{op:.3f}"')
    sea_d = svg(sky_d + typed(seacol("".join(sea), p["sea_col"], p["sea_op"]), p["sea_col"], p["typed_op"]), sky_defs)
    seag = (f'<linearGradient id="sg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{L["sea"]}"/>'
            f'<stop offset="1" stop-color="{L["sea_bot"]}"/></linearGradient>')
    sea_l = svg(f'<rect x="0" y="{p["yh"]}" width="1024" height="{1024 - p["yh"]}" fill="url(#sg)"/>' + sky_l +
                typed(seacol("".join(sea), "#ffffff", L["sea_op"]), "#ffffff", L["typed_op"]),
                sky_ldefs + seag)
    disc_d = svg(f'<circle cx="{mx}" cy="{my}" r="{R}" fill="{BONE}" fill-opacity="{p["disc_op"]}"/>')
    disc_l = svg(f'<circle cx="{mx}" cy="{my}" r="{R}" fill="{L["disc_col"]}" fill-opacity="{L["disc_op"]}"/>')
    lit_d = svg(f'<path d="{lit_path(mx, my, R, p["phase"])}" fill="{p["moon_col"]}"/>')
    lit_l = svg(f'<path d="{lit_path(mx, my, R, p["phase"])}" fill="{L["moon_col"]}"/>')
    glade_d = svg("".join(glade))
    glade_l = svg("".join(glade).replace(p["glade_col"], L["glade_col"]))
    return {
        "scene.svg": sea_d, "scene-light.svg": sea_l,
        "moon-disc.svg": disc_d, "moon-disc-light.svg": disc_l,
        "moon-lit.svg": lit_d, "moon-lit-light.svg": lit_l,
        "glade.svg": glade_d, "glade-light.svg": glade_l,
        "cursor.svg": svg("".join(cur)),
    }, rows


def layer(name, light=None, glass=False, **kw):
    L = {"image-name": f"{name}.svg", "name": name, "glass": glass}
    if light:
        # Default (light) takes the blue hour art. Dark and tinted take the night art.
        # A plain image-name overrides every specialization, so the layer carries none.
        del L["image-name"]
        L["image-name-specializations"] = [{"value": f"{light}.svg"},
                                           {"appearance": "dark", "value": f"{name}.svg"},
                                           {"appearance": "tinted", "value": f"{name}.svg"}]
    L.update(kw)
    return L


NOSHADOW = {"kind": "none", "opacity": 0.5}


def groups_for(p):
    return [
        {"layers": [layer("cursor")], "shadow": NOSHADOW, "specular": False},
        {"layers": [layer("glade", "glade-light")], "shadow": NOSHADOW, "specular": False},
        {"layers": [layer("moon-lit", "moon-lit-light"), layer("moon-disc", "moon-disc-light")],
         "shadow": NOSHADOW, "specular": False},
        {"layers": [layer("scene", "scene-light")], "shadow": NOSHADOW, "specular": False},
    ]


def write_icon(p, path):
    assets, rows = assets_for(p)
    shutil.rmtree(path, ignore_errors=True)
    os.makedirs(path + "/Assets")
    for k, v in assets.items():
        open(f"{path}/Assets/{k}", "w").write(v)
    doc = {
        "fill-specializations": [
            {"value": {"solid": col(p["fill_default"])}},
            {"appearance": "dark", "value": {"solid": col(GROUND)}},
        ],
        "groups": groups_for(p),
        "supported-platforms": {"squares": ["macOS"]},
    }
    open(path + "/icon.json", "w").write(json.dumps(doc, indent=2))
    return assets, rows


def flat_master(p, assets, light=False):
    """The flat master: every layer stacked in paint order on the Dark (or Default) ground."""
    order = ["scene", "moon-disc", "moon-lit", "glade", "cursor"]
    bg = p["fill_default"] if light else GROUND
    parts = [f'<rect width="1024" height="1024" fill="{bg}"/>']
    defs = []
    for n in order:
        key = f"{n}-light.svg" if light and f"{n}-light.svg" in assets else f"{n}.svg"
        s = assets[key]
        m = re.search(r"<defs>(.*?)</defs>", s)
        if m:
            defs.append(m.group(1)); s = s.replace(m.group(0), "")
        inner = re.sub(r"^<svg[^>]*>", "", s); inner = re.sub(r"</svg>$", "", inner)
        parts.append(f'<g id="{n}">{inner}</g>')
    return svg("".join(parts), "".join(defs))


def render(icon, out, look="Dark", size=1024):
    r = subprocess.run([ICTOOL, icon, "--export-image", "--output-file", out, "--platform", "macOS",
                        "--rendition", look, "--width", str(size), "--height", str(size), "--scale", "1"],
                       capture_output=True, text=True)
    if r.returncode or not os.path.exists(out):
        raise RuntimeError(r.stdout + r.stderr)
    return out


def P(**kw):
    import copy
    p = copy.deepcopy(DEFAULT)
    for k, v in kw.items():
        if isinstance(v, dict) and isinstance(p.get(k), dict):
            p[k].update(v)
        else:
            p[k] = v
    return p


def report(rows):
    for r in rows:
        if r.get("typed"):
            print(f"y={r['y']:.0f} typed {' '.join(w for *_, w in r['words'])}")
            continue
        i, j, pl, pr = r["lit"]
        a, b = r["words"][i][0], r["words"][j][1]
        lit = " ".join(w for *_, w in r["words"][i:j + 1]) + " " * pr
        txt = " ".join(w for *_, w in r["words"])
        print(f"y={r['y']:.0f} s={r.get('scale', 1):.3f} cw={r['cw']:.1f} emy={r['emy']:.1f} c={(a + b) / 2 - 512:+.1f} w={b - a:.0f} cost={r['cost']:.2f} "
              f"[{lit}]  | {txt[:90]}")
