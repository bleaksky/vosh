"""The parameter sets of the polish rounds, one place. masters.py builds FINAL with gen.P(**FINAL)."""
# Round 1 of the polish: the lit column as one terminal selection per row, centred by the DP, bands touching at 0.82
F1 = dict(early=3, min_fill=0.55, near_tight=0.6, pool="calm", knock_far=True, s_pitch=0.78, ax=1.05,
          w_top=40, w_bot=140, read_min=2.0, vocab_bonus=3, tol_c=4, sb_scale=0.74, x0=68, xwrap=1000,
          jitter=0.065, lit_band=0.82)
# Round 2: fuller bands, the far scrollback a little stronger toward the horizon
F2 = dict(F1, lit_band=0.86, sea_hz=0.65)
# Round 3: the light fades toward you (0.95 at the horizon to 0.62 at the nearest row) and the bands open to 0.78,
# so at 128 px the column reads as rippled light lying on the water, not a solid white spire standing on it
F3 = dict(F2, lit_band=0.78, glade_top=0.95, glade_bot=0.62)
# Round 4: F3 was a touch faint at 64 px 1x. Brighter at the near end and bands at 0.8 keep the streak
# clear at 64 while it still reads as rippled light at 128
F4 = dict(F3, lit_band=0.8, glade_top=0.95, glade_bot=0.72)
# Text pass: the far rows too keep to Lysenties's echoes and the calm lines, so no other moon is named anywhere
F5 = dict(F4, pool="quiet")
FINAL = F5
