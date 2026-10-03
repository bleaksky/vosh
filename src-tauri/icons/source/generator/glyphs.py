"""JetBrains Mono glyph outlines as SVG path data, placed on the monospace cell with any x and y scale.

The font is the copy Vosh bundles (public/fonts), SIL Open Font License 1.1, so its outlines are safe
to embed in the app icon (see OFL-JetBrainsMono.txt next to the masters). Advance 600 units, x height 550,
cap 730, g descender 180, 1000 units = em."""
import os
from functools import lru_cache
from fontTools.ttLib import TTFont
from fontTools.pens.recordingPen import DecomposingRecordingPen

# src-tauri/icons/source/generator -> repo root -> public/fonts
FONTS = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..", "public", "fonts"))


@lru_cache(None)
def font(weight="Regular"):
    return TTFont(f"{FONTS}/JetBrainsMonoNerdFont-{weight}.ttf")


@lru_cache(None)
def outline(ch, weight="Regular"):
    f = font(weight)
    gs = f.getGlyphSet()
    name = f.getBestCmap().get(ord(ch))
    if not name:
        return []
    pen = DecomposingRecordingPen(gs)
    gs[name].draw(pen)
    return pen.value


def word_path(word, x, baseline, em_x, em_y, weight="Regular"):
    """Path data for a word whose first cell starts at x, on the baseline."""
    sx, sy = em_x / 1000, em_y / 1000
    out = []
    for i, ch in enumerate(word):
        ox = x + i * 600 * sx
        for op, pts in outline(ch, weight):
            P = [f"{ox + q[0] * sx:.2f} {baseline - q[1] * sy:.2f}" for q in pts if q is not None]
            if op == "moveTo":
                out.append("M" + P[0])
            elif op == "lineTo":
                out.append("L" + P[0])
            elif op == "qCurveTo":
                pts_ = list(pts)
                if pts_[-1] is None:  # a contour of off-curve points only: start at an implied on-curve point
                    offs = pts_[:-1]
                    st = ((offs[-1][0] + offs[0][0]) / 2, (offs[-1][1] + offs[0][1]) / 2)
                    out.append(f"M{ox + st[0] * sx:.2f} {baseline - st[1] * sy:.2f}")
                    pts_ = offs + [st]
                while len(pts_) > 2:
                    (ax, ay), (bx, by) = pts_[0], pts_[1]
                    mx_, my_ = (ax + bx) / 2, (ay + by) / 2
                    out.append(f"Q{ox + ax * sx:.2f} {baseline - ay * sy:.2f} {ox + mx_ * sx:.2f} {baseline - my_ * sy:.2f}")
                    pts_ = pts_[1:]
                (ax, ay), (bx, by) = pts_
                out.append(f"Q{ox + ax * sx:.2f} {baseline - ay * sy:.2f} {ox + bx * sx:.2f} {baseline - by * sy:.2f}")
            elif op == "curveTo":
                out.append("C" + " ".join(P))
            elif op in ("closePath", "endPath"):
                out.append("Z")
    return "".join(out)
