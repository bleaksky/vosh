"""Line breaks and the lit run of every row, chosen together by a beam search over rows.

Rows too far out to read run the messages together in the game's order and start anywhere.
Readable rows hold words of one message each (a message always ends its line, as the game sends it),
may wrap a word or two early, and take any message not already shown in a readable row.
Rows that can be read at 1024 take only Lysenties's own messages.
Each row lights one run of whole words, the spaces between them included, as a terminal selection does.
A row may be set a hair off its perspective size (row scale within +-jitter) so the run centres on the axis.
The cost of a row: the run's distance from the axis, its width against one even taper, and how much the
width steps against the row above (narrower than the row above costs a lot, so the column never bulges)."""

OTHER = ("Nercuros", "Dyphrities", "Triad", "three", "Three", "Two", "moons")
# words that say moonlight, preferred in the lit runs of the nearest rows
MOONLIT = ("silver", "Silver", "pale", "light", "Lysenties", "Moon", "moon", "dawn", "warm", "night", "sky", "rises",
           "whole", "glow", "high", "shape", "sphere")


def target_w(p, d, D):
    u = d / D
    return p["w_top"] + (p["w_bot"] - p["w_top"]) * u ** p["w_curve"]


def runs(p, line, W, maxw, cw, readable, keep=4):
    """The best few lit runs on a line: (cost, (i, j, pad_left, pad_right, scale), width)."""
    tc = max(p["tol_c"], p["tol_c_cw"] * cw * (p["near_tight"] if readable else 1.0))
    x0, mx, jt = p["x0"], p["mx"], p["jitter"]
    sz = max(1.0, (cw / p["ref_cw"]) ** p["size_pow"]) if readable else 1.0
    out = []
    pads = ((0, 0),) if (readable or not p["pad"]) else ((0, 0), (1, 1), (0, 1), (1, 0))
    for i in range(len(line)):
        for j in range(i, min(len(line), i + maxw)):
            for pl, pr in pads:
                if pl and i == 0:
                    continue
                a, b = line[i][0] - pl * cw, line[j][1] + pr * cw
                c = (a + b) / 2
                sc = min(1 + jt, max(1 - jt, (mx - x0) / (c - x0))) if c > x0 else 1.0
                c2, w2 = x0 + (c - x0) * sc, (b - a) * sc
                cost = sz * (((c2 - mx) / tc) ** 2 + p["w_weight"] * ((w2 - W) / (p["tol_w"] * W + 1.5)) ** 2)
                cost += p["pad_cost"] * (pl + pr) + p["jitter_cost"] * abs(sc - 1) / max(jt, 1e-9)
                if readable:
                    for k in range(i, j + 1):
                        if any(o in line[k][2] for o in OTHER):
                            cost += p["topic_lit"]
                    if cw >= p["vocab_cw"] and any(v in line[k][2] for k in range(i, j + 1) for v in MOONLIT):
                        cost -= p["vocab_bonus"]
                out.append((cost, (i, j, pl, pr, sc), w2))
            if line[j][1] > mx + W * 1.3:
                break
    out.sort(key=lambda t: t[0])
    return out[:keep]


def far_line(p, r, M, m, w):
    cw, x, line = r["cw"], p["x0"], []
    while True:
        word = M[m][w]
        wx = len(word) * cw
        if x + wx > p["xwrap"] and line:
            break
        line.append((x, x + wx, word)); x += wx + cw
        w += 1
        if w == len(M[m]):
            m, w = (m + 1) % len(M), 0
    return line, (m, w)


def near_lines(p, r, M, m, w):
    cw, x, line = r["cw"], p["x0"], []
    words = M[m]
    k = w
    while k < len(words):
        wx = len(words[k]) * cw
        if x + wx > p["xwrap"] and line:
            break
        line.append((x, x + wx, words[k])); x += wx + cw; k += 1
    n = len(line)
    width = p["xwrap"] - p["x0"]
    out = []
    for cut in range(max(1, n - p["early"]), n + 1):
        ln = line[:cut]
        end = w + cut == len(words)
        if cut < n and (ln[-1][1] - p["x0"]) / width < p["min_fill"]:
            continue
        out.append((ln, None if end else w + cut))
    return out


def step_cost(p, wprev, wnew, dW):
    """Width against the row above: it should grow by about the taper's own step, and never shrink."""
    if wprev is None:
        return 0.0
    g = wnew - wprev
    c = ((g - dW) / (p["step_tol"] * abs(dW) + p["step_px"])) ** 2
    if g < -p["shrink_px"]:
        c += p["shrink_cost"] * ((-g - p["shrink_px"]) / 4.0 + 1)
    return p["step_weight"] * c


def layout_rows(p, rows, MESSAGES, lys=None, core=None):
    M = [m.split() for m in MESSAGES]
    lys = set(range(len(M))) if lys is None else set(lys)
    core = lys if core is None else set(core)
    D = rows[-1]["d"]
    nm = len(M)
    Wt = [target_w(p, r["d"], D) for r in rows]
    Q = p["wq"]
    # state: (message, next word or None at a boundary, mask of messages in readable rows, last width bucket)
    cur = {(m, w, 0, -1): 0.0 for m in range(nm) for w in range(len(M[m]))}
    back = []
    memo = {}
    for ri, r in enumerate(rows):
        W = Wt[ri]
        dW = W - Wt[ri - 1] if ri else 0.0
        legible = r["emy"] >= p["letters_min"]
        readable = r["emy"] >= p["read_min"]
        nxt, bp = {}, {}
        for (m, w, mask, wb), c0 in cur.items():
            wprev = wb * Q if wb >= 0 else None
            if not legible:
                key = (ri, m, w if w is not None else 0)
                if key not in memo:
                    line, st2 = far_line(p, r, M, *key[1:])
                    memo[key] = (line, runs(p, line, W, p["far_words"], r["cw"], False), st2)
                line, rs, (m2, w2) = memo[key]
                cand = [((m2, w2, mask), line, rr) for rr in rs]
            else:
                if w is None:
                    # readable rows take Lysenties's messages; when the pool is mixed, the rows between keep to the rest
                    starts = [(mm, 0) for mm in range(nm) if not (mask >> mm) & 1 and
                              ((mm in lys) if readable else (mm not in lys or len(lys) == nm))]
                else:
                    if readable and m not in lys:
                        continue
                    starts = [(m, w)]
                cand = []
                for sm, sw in starts:
                    key = (ri, sm, sw)
                    if key not in memo:
                        memo[key] = [(ln, runs(p, ln, W, p["max_words"] if readable else p["mid_words"],
                                               r["cw"], readable), nw)
                                     for ln, nw in near_lines(p, r, M, sm, sw)]
                    mk = mask | (1 << sm)
                    extra = p["near_core_cost"] if (r["emy"] >= p["near_core_emy"] and sm not in core) else 0.0
                    for ln, rs, nw in memo[key]:
                        for c_, lit_, wid_ in rs:
                            cand.append(((sm, nw, mk), ln, (c_ + extra, lit_, wid_)))
            for (m2, w2, mk2), line, (c, lit, wid) in cand:
                tot = c0 + c + step_cost(p, wprev, wid, dW)
                st = (m2, w2, mk2, int(round(wid / Q)))
                if tot < nxt.get(st, 1e30):
                    nxt[st] = tot
                    bp[st] = ((m, w, mask, wb), line, lit, c)
        if len(nxt) > p["beam"]:
            keep = sorted(nxt, key=nxt.get)[:p["beam"]]
            nxt = {k: nxt[k] for k in keep}
        back.append(bp)
        cur = nxt
    ends = [st for st in cur if st[1] is None]
    best = min(ends, key=lambda st: cur[st])
    total = cur[best]
    st = best
    for ri in range(len(rows) - 1, -1, -1):
        prev, line, lit, c = back[ri][st]
        sc = lit[4]
        x0 = p["x0"]
        rows[ri]["words"] = [(x0 + (xa - x0) * sc, x0 + (xb - x0) * sc, w) for xa, xb, w in line]
        rows[ri]["cw"] *= sc; rows[ri]["emx"] *= sc
        rows[ri]["lit"] = lit[:4]; rows[ri]["scale"] = sc; rows[ri]["cost"] = c
        st = prev
    return total
