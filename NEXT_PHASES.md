## Where We Are

Phase 9 complete. Phase 11 in-app polish complete. Phase 12 stretch
work mostly skipped (only macro recorder shipped — TTS / plugin
marketplace / local LLM all explicitly deferred by the user). Combat
visualization, target HUD, group roster, room info, sidebar
reordering, ROM `\n\r` line accumulator fix, palette match to
Ghostty, and high-contrast theme extension all landed during this
session.

Recent commits worth noting:

- `30015ca` macro recorder (`#record` / `#endrec`)
- `209f0ca` terminal bottom padding so descenders clear the input seam
- `afdf06a` combat `You%1` prefix on outgoing damage and miss
- `6f15917` off-floor cells uniform alpha (no per-z distance fade)
- `cfa6b3a` orphan preset trigger sweep on startup
- `ef0a933` `tauri-plugin-window-state` for position/size persistence
- `f9ecdd2` high-contrast overrides across Phase 12 chrome
- `17506cf` TinTin palette match + crit wrappers + affect level
- `1354689` RoomInfoBar, sortable side-panel sections, wealth/group tabs
- `e41efb2` ROM `\n\r` line terminator handling in the accumulator
- `614091f` target HP bar + per-update vitals deltas

## What's Actually Left

Most of the formal phase plan is done. Three buckets remain:

### Blocked on real game capture

These require pasting actual Aabahran log lines so the patterns
aren't guesses. The previous wave of hallucinated triggers got
removed; no new ones should land without verifiable wording.

1. Recall preset (self-recall, other-player recall, `utters the
words 'recall'`).
2. Debuff fade preset (sanc dispelled, bless drops, haste drops, etc.).
3. Weapon proc highlights — poison, deadly, vampiric, frost, etc.
   The flaming/shocking samples the user pasted aren't covered yet
   because the proc flavor lines start with the target name, not a
   damage verb.

### User-owned infrastructure (Phase 11 finishers)

Cannot be done without certs / accounts / physical machines.

1. macOS notarization (Apple Developer ID).
2. Windows Authenticode signing.
3. AppImage, .deb, .rpm packaging in CI matrix.
4. Real updater endpoint + minisign keypair in `tauri.conf.json`
   (`endpoints` and `pubkey` currently placeholder strings).
5. Demo installs on each platform.

### Skipped Phase 12 stretch

Explicitly deferred. None of these are blocking.

1. ~~Plugin marketplace manifest~~ — needs a curated source the user
   would run.
2. ~~OS-native TTS triggers~~ — user declined.
3. ~~Optional local LLM~~ — user declined.
4. ✅ Macro recorder — shipped (`#record` / `#endrec`).

## Suggested Next Steps

In rough priority order, things that are still actionable:

1. Capture the messages above (recall, debuff fades, weapon procs)
   and ship the missing presets.
2. Iterate on whatever surfaces during real gameplay — combat color
   tweaks, panel layouts, new patterns the user notices.
3. Pick up Phase 11 packaging when the user has dev certs ready.
