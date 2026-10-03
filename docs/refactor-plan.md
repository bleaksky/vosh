# Vosh refactor plan before 1.0

Second draft, October 1, 2026. Ten area audits read the whole project at `one-window` 9656157, and a second reader confirmed every dead code claim. Two critics then checked the first draft against the code. This draft applies their corrections, folds in the Phase 10 check, and starts from the latency work, which lands on `one-window` before the refactor begins. Nothing in the repo changed.

## Status

The last commit of each phase updates this table and the Phase Status line in CLAUDE.md.

| Phase | What                                      | Status                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ----- | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R0    | Freeze and set up                         | Done. The plan is in the repo, you have the stale branch list, and one flaky test is fixed                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| R1    | Latency work                              | Done. a55a817 to 846c85d, plus 7dced45, which fixed the Windows and Linux build                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| R2    | Safety nets                               | Done. 5fdbe52 to 4882421 with R0. D37 keeps the synthetic fixtures, the dead code tools wait for R23                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| R3    | The 13 bug fixes                          | Done. db3c0a4 to b9cc972 fix bugs 1 to 12. Bug 13 leaves with its code in R13 (D9)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| R4    | Dead page code and old files              | Done. 7e8199f to e347469, with D8, D20, D27 and D31                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| R5    | Dead styles                               | Done. d2357d2 to b99934a                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| R6    | Dead backend commands and app code        | Done. ba9284b to 3ac5cb9, with D3, D5, D6, D7, D12, D14 and the D20 exporter. The stand in functions stay with D9 option B                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| R7    | Small crates tidied in place              | Done. 16e7410 to 2684e94, the review fixes included. vosh-ansi keeps plain_text and pieces, and the test kits in vosh-ansi, vosh-alias, vosh-trigger and vosh-log hold the items only the tests use                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| R8    | Crates merged, log crate split            | Done. dc8ada7 to 06655ac, the review fixes included. D2 A gives four crates by layer plus prompt. D3 already retired the map store in R6. D15 retired the mudclient folder copy and the storage key copy. readable.rs now lives in `crates/automation/src/trigger`, and pieces lives in `crates/protocol/src/ansi`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| R9    | Prompt crate                              | Done. a590a60 to bc93f95, the review fixes included. D24 drops the payload fields the page never reads and keeps each value's GMCP source as a comment. D25 renames vars to values, Vosh to ClientValues and Stage::show to show_as_sent. D26 keeps both look alike rules, with a comment at each that names the other. `crates/prompt/src/aabahran/who.rs` holds Who, who the prompt is for, and the compile errors stay in aabahran.rs. Narrowing the crate found Stage::shows and PromptEngine::cols with no caller, and both went                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| R10   | App frame and the command layer           | Done. 65fbd11 to 8a2afd7, the review fixes included. commands.rs is gone. Every command sits in `ipc/` by topic, and `ipc::handler()` in ipc.rs registers them all. `app/` holds the state, the event names, launch, exit, the Settings and Help windows and the plugins. The save engine lives in `disk/save.rs`, switching in `profile/switch.rs`, which the session now calls, the wizard's apply step and journal in `loadouts/wizard/`, and the prompt logic in `prompt.rs` and `prompt/`. The two updater commands call the updater plugin themselves, so they stay whole in `ipc/updater.rs`. The code locks the profile before the profile set, not after as R10 item 9 guessed, and docs/architecture.md records that order. R11, R12 and R13 say what R10 took early and what it left them                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| R11   | Session and input                         | Done. 0ef62ea to c29b647, the review fixes included. session.rs became `session/`, one file per job, with the loop and `Conn` in conn.rs, one socket read path in read.rs, `LogSink` in log_sink.rs, and the session tests on one harness under `session/tests`. `output.rs` is the one path that writes to the terminal. input.rs keeps the pipeline and `run_typed_line`, which runs a line through `run_lines_locked` in `session/effects.rs` as a timer, the tick and `mud.input` do, `input/` holds the commands by topic, and InputResult builds through constructors. `client_values` and `report_game_prompt_seen` sit in prompt.rs, so no prompt call goes up into the session. tick.rs reads the World.Time tick and holds the one tick settings shape. script_state.rs became script.rs, log_state.rs and forget_passwords.rs went to `logs/`, and room_block.rs and highlight_ground.rs to `session/`. The session commands in `ipc/session.rs` end thin, `spawn` takes the known host flag, so the test port seam and its test went, and latency.rs sits in `tests/`. Four stored fixtures pass unwritten, not three, since the collapse splits came after the plan. Eight `too_many_arguments` allows went, not seven, one with LogSink and seven with Conn. The render counter RENDERS stays behind `cfg(test)` in `session/prompt_view.rs`, where 3.11 had it go, since the clock and repaint tests count draws by it. The Settings timers and the tick poll stay in `session/conn.rs` beside the poll arm that runs them, since both write to the socket the loop owns. Beyond the plan, two tracing lines changed and nothing the game, the screen or the session log sees did. A drained read that fails warns as any other read does, and the `vosh::perf` debug line counts only game ticks, so an idle session logs none                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| R12   | Profiles, loadouts and files on disk      | Done. cf8a6d4 to 088d5e8. profile_config.rs, profile_set.rs, profile.rs and characters.rs became `profile/`, one file per job, loadout.rs and loadout_store.rs became `loadouts/`, and migration.rs became `loadouts/wizard/plan.rs` and `groups.rs`. The one time upgrades run in `disk/upgrades/` as one ordered list over one read of profiles.toml, with every id unchanged, the importers sit in `import/` and share one report, the affects files sit in `affects/`, and launch and a switch share one catalog overlay. Where R12 left the plan as written. loadouts.toml lives in `loadouts/set.rs`, not `loadouts/loadouts.rs`, since clippy refuses a module named like its parent (module_inception), and `set_active_loadouts` sits there beside it. SessionIdentity and its broadcast went to `session/identity.rs`, not `profile/inactive.rs`, since they describe the live connection. Item 3's one AppDataDir is the `app_data` field on AppState, which launch sets once, and `disk/paths.rs` reads it as one function of that folder for each file and folder in it, with no type of its own. `ProfileSet::read_all` took seven loops, not six, and left none out. D18 moved the generations, the dirty counter, the relaunch, save suppression and loadout mode flags and the app data folder onto AppState, and the `_with` seams that passed them in went. The unread file list stays a static in `disk/atomic.rs`, held by path, since the safe write that refuses those files takes no AppState, and `hand_out_shared_with` stays as the seam a test uses to make a save fail. Item 5 did not land. A table of the five shared categories was tried twice and set aside, so `profile/shared.rs` still spells the categories out in each place. Beyond the plan, four things you could see in a log or an error changed and nothing the game, the screen or a saved file sees did. Launch logs one error for a profiles.toml it cannot read instead of three, the one overlay logs a rejected trigger with warn! at launch too, `HeldCustomThemes::find` logs the files it cannot read in index order, not the active one first, and with no app data folder `loadouts_set_active`, `migration_analyze` and `migration_apply` answer with a plain sentence instead of the error from Tauri |
| R13   | Native renderer and windows               | Current. Stage B runs R7 to R13 as one stage                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| R14   | Connection state out of the profile       | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R14b  | Sessions. One tab per connection          | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R15   | Page command and event layer              | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R16   | Page folder moves                         | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R17   | Big components split, helpers merged      | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R18   | Styles reorganized                        | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R19   | Settings saves one field at a time        | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R20   | One store pattern                         | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R21   | Design merges                             | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R22   | Phase 10. Logs, scrollback and search     | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R23   | Docs, help and guards                     | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R24   | Phase 11. Release groundwork              | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R25   | Phase 11. Accessibility and high contrast | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| R26   | Phase 11. Signed packages                 | Not started                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

The R1 section below names d2239d7. That is the same work before it moved onto `one-window`, where it landed as 72f95e6.

Stage B. On October 2 you approved R7 to R13 as one stage, with new features frozen until the safe stopping point after R13.

Features that landed before stage B. These landed on `one-window` after R6, in the old structure, so Parts 1 to 3 do not list them. Each phase places the ones in its files into the target structure when it reaches them.

- Blinking text. The `Blinking` wrapper in term_grid.rs, cell_render.rs and xtermBlink.ts, plus the blink prompt code.
- Five themes, Solarized Dark and Light, Everforest Dark and Light and Green Screen, and the chat lift that keeps faint channel colors at 3:1 on the panel.
- Prompt features. The push to the right edge, the steps color, since, ampm, thousands and pct:game.
- The Room target with room_block.rs, the Your target match, and the Room, time and weather colors preset.
- Readable highlight colors, in readable.rs in `crates/trigger`, with the grounds fixture the page tests read too.
- Highlights drawn over the original bytes, through the `pieces` split in `crates/ansi`.
- The grey caret that marks your commands.
- Collapse repeated lines, across the prompt stage, the session, term_grid.rs and the scrollback.
- The map tile fixes, on the page only.
- The Rust 1.99.0 pin in rust-toolchain.toml.
- Smaller work, such as the affects thresholds, the Draining chips style, Settings in the terminal menu and the history drag through the split.

R16 places four page files Part 2 does not name. `src/lib/blink.ts` and `src/lib/xtermBlink.ts` came with blinking text and `src/lib/readableGrounds.test.ts` with readable highlight colors. `src/lib/fontLoader.ts` is older, but it now holds the page side of the font stacks twin.

The trigger engine reads `pieces` in `crates/ansi`, and the readable.rs tests read the SGR model there, so the ansi row in 3.7 and R7 item 3 no longer hold as written. R7 keeps `plain_text` and `pieces` and puts the SGR model behind a vosh-ansi `testkit` feature that the vosh-trigger tests turn on.

## Decisions taken

You approved this plan on October 1, 2026. Every decision takes its recommended answer from the answer sheet, except four you answered yourself the same day and D19, which changed on October 2.

- D10. Option B. Vosh no longer ships Berkeley Mono, and the bundled JetBrains Mono is the default font. A saved list that names Berkeley Mono draws with the copy installed on your computer, or with JetBrains Mono where none is installed. 9541225 to 1f99108 made the change before R4, and a fix after R6 makes the xterm renderer measure its cell again once your font loads.
- D1. Option C. Every phase lands on `one-window` on your machine, and nothing is pushed.
- D37. Option C. The wire fixtures stay synthetic and no captures land, so R2 item 8 and its R7 fallback drop. The CLAUDE.md quality bar still asks for captured bytes, and that line changes to say so once you approve the new wording.
- D20. Every part takes its recommended answer. The `VOSH_WRITE_PLAYS` exporter left in R6 with the rest of the debug tools.
- D19. Option A, whether or not R11 lands clean. You want tabs for more than one connection, and R14 gives each connection its own state as their groundwork. So R14 is required, and the Sessions phase R14b follows it.

With D1 as answered, CI never sees the refactor. A phase that touches platform code runs `cargo clippy -p vosh-app --all-targets --target x86_64-pc-windows-gnu -- -D warnings` on the Mac in place of CI. Nothing here compiles its Linux code, so whoever lands the phase reads that code through.

## Answer sheet

Answer these in the order the phases need them. Blocks 1.0 marks the eight answers 1.0 cannot ship without, because they settle whether Vosh may ship a file or what 1.0 promises against prompt.md. Every other decision only shapes the work. If you never answer one, the phase that needs it keeps today's behavior for that item and moves on.

| First needed | #   | Question                                   | Recommended                                                 | Blocks 1.0 |
| ------------ | --- | ------------------------------------------ | ----------------------------------------------------------- | ---------- |
| R0           | D10 | May Vosh ship Berkeley Mono?               | Confirm the license now, or remove the font at R0           | Yes        |
| R0           | D1  | How each phase lands                       | Merge to main, one pull request per phase, CI on every push | Yes        |
| R0           | D39 | Where this plan lives                      | `docs/refactor-plan.md`, with CLAUDE.md pointing at it      |            |
| R2           | D37 | Where real telnet and ANSI bytes come from | You record one short socat session                          |            |
| R3           | D4  | Lua alias bodies                           | Make them run                                               | Yes        |
| R3           | D9  | Native surface on Windows and Linux        | Drop it for 1.0, xterm there as today                       | Yes        |
| R3           | D16 | The catalog on a profile switch            | Keep the profile's own items, as launch does                |            |
| R4           | D8  | Desktop only                               | Drop the mobile entry and icons                             |            |
| R4           | D20 | Debug tools                                | Remove them, and tell me if the screenshot harness exists   |            |
| R4           | D27 | Char.State and weather stores              | Drop them, keep their fixtures                              |            |
| R4           | D31 | Old root files and mockups                 | Delete them, git keeps them                                 |            |
| R6           | D3  | The local map store                        | Retire it, plan speedwalk after 1.0                         | Yes        |
| R6           | D14 | Rolling back to 0.7.2                      | Keep it working through 1.0                                 |            |
| R6           | D12 | Old fields nothing reads                   | Off the page, kept on disk through 1.0                      |            |
| R6           | D5  | Plugin commands                            | Drop them, plugins still load                               |            |
| R6           | D6  | Group toggle commands                      | Drop them                                                   |            |
| R6           | D7  | The profile description setter             | Drop it, keep your text                                     |            |
| R8           | D2  | Crate layout                               | Four crates by layer, plus prompt                           |            |
| R8           | D15 | Old one time migrations                    | Retire the mudclient copy with the map store                |            |
| R9           | D24 | Prompt fields the page never reads         | Drop them                                                   |            |
| R9           | D25 | Prompt crate renames                       | Yes, with the revised names                                 |            |
| R9           | D26 | Two look alike prompt rules                | Keep both, document why                                     |            |
| R12          | D13 | The old dock layout field                  | Keep writing it through 1.0                                 |            |
| R12          | D17 | Loadout mode names in code                 | Rename, the wire field stays                                |            |
| R12          | D18 | App state instead of process switches      | Yes                                                         |            |
| R12          | D33 | Loadout World and Characters rows          | Drop the rows                                               |            |
| R14          | D19 | Connection state out of the profile        | Only if R11 lands clean                                     |            |
| R17          | D28 | Wording helpers                            | One possessive, serial comma                                |            |
| R19          | D21 | Settings saves one field at a time         | Yes                                                         |            |
| R21          | D22 | Visual merges                              | Yes, board first                                            |            |
| R21          | D23 | The migration wizard's look                | Yes, board first                                            |            |
| R22          | D29 | Log files                                  | No running text files, add Save as file                     | Yes        |
| R22          | D40 | Scrollback size and timestamps             | A size in lines, times stay in the log view                 | Yes        |
| R22          | D34 | Log retention and switches                 | Keep logs for, forever by default                           |            |
| R22          | D35 | Log search scope                           | Last 7 days by default, fix the engine                      |            |
| R22          | D36 | Search speed in dev builds                 | Optimize four crates in dev                                 |            |
| R23          | D11 | One source for help                        | HELP.md is the source                                       |            |
| R23          | D30 | Project docs                               | A requirements doc replaces the phase model                 |            |
| R23          | D32 | Dead code guards in CI                     | Add knip and the CSS check                                  |            |
| R26          | D38 | Windows signing                            | Approve a certificate service                               | Yes        |

40 decisions in all. The numbers match the first draft. D29 now holds the Phase 10 check's answer, and D34 to D40 are new.

## The phases at a glance

| Phase | Stage | What                                      | 1.0            | Needs                    | Rough size             |
| ----- | ----- | ----------------------------------------- | -------------- | ------------------------ | ---------------------- |
| R0    | A     | Freeze and set up                         | Required       | D1, D10, D39             | no code                |
| R1    | A     | Latency work, already landed              | Required, done |                          | no new code            |
| R2    | A     | Safety nets                               | Required       | D37                      | 1,100 test lines added |
| R3    | A     | The 13 bug fixes                          | Required       | D4, D9, D16              | 700 changed            |
| R4    | A     | Dead page code and old files              | Required       | D8, D20, D27, D31        | 9,400 removed          |
| R5    | A     | Dead styles                               | Required       |                          | 5,900 removed          |
| R6    | A     | Dead backend commands and app code        | Required       | D3, D5, D6, D7, D12, D14 | 1,500 removed          |
| R7    | B     | Small crates tidied in place              | Required       |                          | 1,000 removed          |
| R8    | B     | Crates merged, log crate split            | Required       | D2, D15                  | 6,000 moved            |
| R9    | B     | Prompt crate                              | Required       | D24, D25, D26            | 21,000 moved           |
| R10   | B     | App frame and the command layer           | Required       |                          | 11,000 moved           |
| R11   | B     | Session and input                         | Required       |                          | 11,000 moved           |
| R12   | B     | Profiles, loadouts and files on disk      | Required       | D13, D17, D18, D33       | 12,000 moved           |
| R13   | B     | Native renderer and windows               | Required       | D9, D10                  | 10,000 moved           |
| R14   | B     | Connection state out of the profile       | Required       | D19                      | 1,500 changed          |
| R14b  |       | Sessions. One tab per connection          | Required       | Boards                   | feature work           |
| R15   | C     | Page command and event layer              | Required       |                          | 3,400 moved            |
| R16   | C     | Page folder moves                         | Required       |                          | 60,000 moved           |
| R17   | C     | Big components split, helpers merged      | Required       | D28                      | 5,000 moved            |
| R18   | C     | Styles reorganized                        | Required       |                          | 2,500 moved            |
| R19   | C     | Settings saves one field at a time        | Deferrable     | D21                      | 900 removed            |
| R20   | C     | One store pattern                         | Deferrable     |                          | 600 removed            |
| R21   | C     | Design merges                             | Deferrable     | D22, D23                 | 800 removed            |
| R22   |       | Phase 10. Logs, scrollback and search     | Required       | D29, D34, D35, D36, D40  | 1,200 added            |
| R23   | D     | Docs, help and guards                     | Required       | D11, D30, D32            | docs and CI            |
| R24   |       | Phase 11. Release groundwork              | Required       | D10                      | small                  |
| R25   |       | Phase 11. Accessibility and high contrast | Required       |                          | feature work           |
| R26   |       | Phase 11. Signed packages                 | Required       | D38                      | release workflow       |

Required means 1.0 needs it, since you asked for the refactor before 1.0. Deferrable means 1.0 can ship without it and no later phase depends on it.

Safe stopping points. You can pause at the end of R6, R13, R18 or R23 for as long as you like, cut a release there, or let feature work resume. At each one every check is green, no folder is half moved, and docs/architecture.md matches the tree. Between them a stage is half done, for example between R10 and R12, when profile code lives partly in `profile/` and partly in the old files. The deferrable phases sit right after a safe stopping point, so skipping one leaves nothing half done.

## The 13 bugs

The audits found these on the way. They are not refactor work. Each one gets its own commit with a test that fails first, and each needs your yes. They all land in R3, before any code moves, so the moves carry fixed code.

| #   | What you notice as a player                                                                                                                                              | The cause                                                                                          | Where                                                                     | After the fix                                                                                                                                                                                     |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | An alias with a Lua body swallows what you type. Nothing is sent and no Lua runs                                                                                         | The input path expands aliases with a call that drops script bodies. Needs D4                      | `input.rs` `process_line`                                                 | An alias set to run Lua runs its body where you typed it, with the words after its name in captures, and the game hears what it sends                                                             |
| 2   | In the command palette, alias rows show no command, and an alias that takes arguments runs at once instead of filling the command line                                   | The palette reads `template`, but Rust sends `expansion`                                           | `palette.ts` `buildAliasEntries`                                          | Each alias row in the palette shows the command or script it runs, a search finds it by that command, and an alias that takes arguments fills the command line                                    |
| 3   | After `#group` or a loadout switch turns a macro group off, its keys keep firing until you edit any macro                                                                | Nothing tells the command line that macro groups changed                                           | `script_state.rs` `toggle_group`, the macro group listener in `Input.tsx` | A macro group that `#group`, Lua or a loadout switch turns off stops its keys at once                                                                                                             |
| 4   | `#profile save`, `#profile load` and `#script load` fail on Windows and write to the wrong place on Linux. Saving a profile with no file yet writes a stray profile.toml | They use a hard coded macOS folder                                                                 | `input.rs` `profile_path`, `script_path_for`                              | `#profile save`, `#profile load` and `#script load` use the Vosh app data folder on every platform, and a save never writes a stray profile.toml                                                  |
| 5   | If profiles.toml fails to load at launch and you then delete it to recover, an old snapshot replaces your real default profile                                           | A save fallback writes a root profile.toml                                                         | `commands.rs` `persist_state_with`                                        | If Vosh could not read your profile list at launch and you deleted it to start fresh, your Default profile now stays as you left it instead of being replaced by settings from the broken session |
| 6   | "héllo" imports as "hÃ©llo" from TinTin++ and CMUD files                                                                                                                 | The importers cast bytes to characters                                                             | `tintin_import.rs` `read_braced`, `import.rs` `translate_cmud_wildcards`  | "héllo" imports as "héllo" from TinTin++ and CMUD files, and a CMUD pattern that quotes a letter with a tilde matches that letter                                                                 |
| 7   | From `#lua`, `#script load`, a Settings timer, a script alias or a plugin load, `mud.timer`, `mud.cancel_timer`, `mud.input` and prompt values do nothing                | Only the game output path applies every Lua effect                                                 | `session.rs` `apply_script_result` and the paths beside it                | Lua from `#lua`, `#script load`, a Settings timer, the tick, a script alias or a plugin starts and cancels timers, runs `mud.input` lines and sets prompt values, just as Lua from a trigger does |
| 8   | Lines that timers, the tick or `mud.input` send never update the target display and never repaint the prompt after a `#prompt` change                                    | Fired lines skip two steps typed lines take                                                        | `session.rs` `run_fired_locked`                                           | Lines that your timers, the tick command or a Lua script send now update the target display and redraw your prompt after a `#prompt` change, the same as lines you type                           |
| 9   | After the game closes the connection, typing says "session task gone" instead of "[not connected]"                                                                       | The dead session stays in app state                                                                | `commands.rs` `session_send_input`                                        | After the game closes the connection, a line you type says "[not connected]"                                                                                                                      |
| 10  | Memory grows slowly, and GMCP handlers run twice or more after each `#script reload`                                                                                     | A cancelled Lua timer never frees its callback, and a reload subscribes to GMCP again              | `crates/script`                                                           | A cancelled Lua timer frees its memory, and a GMCP handler runs once however often you type `#script reload`                                                                                      |
| 11  | In loadout mode, a profile behaves differently after a switch than after a restart                                                                                       | A switch drops items the profile file holds, while launch keeps them. Needs D16                    | `commands.rs` `lay_catalog_over` against `launch.rs` `load_loadout_mode`  | In loadout mode, a switch to a profile keeps the aliases, triggers and macros its file holds, the same as a restart                                                                               |
| 12  | After you resize the window on macOS, the game keeps wrapping at the old width until the size changes again                                                              | A busy session lock drops the size report                                                          | `native_surface/mod.rs` `report_sizes`                                    | After you resize the window on macOS, the game wraps at the new width, even when the resize lands while you type                                                                                  |
| 13  | On Windows with the native surface forced on and a split open, the resize cursor shows over the whole terminal                                                           | Only behind a hidden flag today. With D9 option B this code leaves in R13 and the bug goes with it | `native_surface/windows.rs`, the cursor code around `track_leave`         | Not fixed in R3. With D9 option B its code leaves in R13                                                                                                                                          |

The Phase 10 check found one more. If you quit while connected, or the app crashes, the next launch restores the scrollback saved at your last disconnect, and the session's end time is never logged. Reading the exit path confirms it writes neither. R22 fixes it, since that phase owns scrollback.

## About this plan

Where the plan lives. Once you approve it, this plan goes into the repo as `docs/refactor-plan.md` at R0, with a status table at the top. CLAUDE.md Phase Status then names that file and the current R phase, so a later session reads CLAUDE.md, opens the plan, and knows where the work stands. The last commit of each phase updates both (D39).

How to read it. The answer sheet, the phase list and the 13 bugs fill the first screens. The three promises every phase keeps sit in 4.1. Part 1 says where the code stands. Part 2 draws the target. Part 3 lists the dead code. Part 4 details each phase. Part 5 explains each decision in the order of the answer sheet. Part 6 lists the risks.

Places in code. This plan names places by function, selector or section name, because the latency work and every deletion shift line numbers. The few line numbers left refer to commit 9656157.

| Word           | What it means here                                                                                  |
| -------------- | --------------------------------------------------------------------------------------------------- |
| The page       | The web view side, in TypeScript and React. Everything under `src/`                                 |
| The app, Rust  | `src-tauri/src` and the crates in `crates/`                                                         |
| IPC            | A command the page calls in Rust, or an event Rust sends the page. Both are matched by name strings |
| Native surface | The GPU renderer that draws the terminal on macOS. Under the underlay it sits beneath the page      |
| xterm          | The terminal library the page uses. It draws the terminal on Windows and Linux                      |
| Twin           | One job written once in Rust and once in TypeScript, because both renderers need it (2.10)          |
| Digest         | A test that hashes what the session sends to the screen and the log, so any change fails it         |
| Golden file    | A stored copy of a config file's exact bytes that a test compares against                           |
| Fake MUD       | A test server in the prompt test kit that plays Aabahran output over a local port                   |
| Board          | A mockup you approve before a visual change                                                         |

## Part 1. Where the code stands

### 1.1 Size by area, at 9656157

| Area          | Where                                                                               | Lines       | The big files                                                                                                          |
| ------------- | ----------------------------------------------------------------------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------- |
| Prompt engine | `crates/prompt`                                                                     | 35,254      | About 17,300 code and the rest tests. stage.rs 4,572, vars.rs 2,925, engine.rs 2,419                                   |
| Small crates  | `crates/telnet`, `ansi`, `gmcp`, `alias`, `vars`, `trigger`, `script`, `map`, `log` | 9,378       | log 3,416, trigger 1,645, telnet 1,107. Each crate has one user, the app                                               |
| App backend   | `src-tauri/src`                                                                     | 58,122      | commands.rs 9,088, session.rs 5,742, profile_config.rs 4,476, cell_render.rs 4,258, input.rs 3,225, term_grid.rs 2,817 |
| Page code     | `src` without tests                                                                 | 61,659      | session.ts 3,409, App.tsx 1,990, ServerMapView.tsx 1,643, Terminal.tsx 1,546, PromptCard.tsx 1,243, Input.tsx 1,112    |
| Page tests    | `src/**/*.test.*`                                                                   | 25,512      | 140 files with 1,689 tests                                                                                             |
| Styles        | `src/styles.css`, `src/styles/`                                                     | 14,825      | styles.css alone is 7,164                                                                                              |
| Help          | `HELP.md`, `src/lib/helpContent.ts`                                                 | 762 and 453 | The same 43 topics written out twice                                                                                   |

Rust has 1,460 test functions. The app offers 133 commands to the page and sends 51 named events back. The latency run adds about 1,100 lines, most of them tests.

### 1.2 What makes the code hard to follow today

1. A handful of files do everything. commands.rs holds the commands the page calls, and also the app state, the save engine, the profile switch, the event sender, the Settings payload schema, the Settings and Help windows, and the catalog wizard. session.rs runs the socket loop and also GMCP, Lua effects, three timer systems, logging, prompt drawing and the output path. session.ts, App.tsx and styles.css do the same on the page.
2. Old code still sits in the tree. About 7,800 lines of page files load nowhere, about 5,800 lines of styles.css style nothing that renders, and 18 backend commands have no caller.
3. Names come from older eras. Ember names the August look. Path B names what the app now calls loadout mode. A folder called legacy holds the live hook every Settings page saves through. 342 comments cite phases or spec sections that are not in the repo.
4. Layers call each other in circles. The session loop calls into commands.rs and back. A helper in src/lib imports a component. App state sits in process wide switches, so eight functions carry a `_with` copy only so tests can swap the switch.
5. The same job is written several times. Four floating menus, three icon sets, three hex color parsers, five ways to skip escape codes, three shapes for the tick settings, and two copies of every help topic. When one copy changes and the others do not, you get a bug.
6. Docs describe a project that no longer exists. CLAUDE.md says Phase 9 is complete and describes a drawer that is gone. README claims MSDP, MCCP and MXP, which Vosh refuses. fixtures/README lists three empty folders.
7. Tests live in four layouts, so the tree doesn't tell you where a module's tests are.

### 1.3 What already protects the refactor

- The session digests in session_show_tests.rs fail on any change to output bytes, log rows or kept lines.
- Three stored fixtures are read by both the Rust tests and the page tests (pinned splits, preview splits and pointer cases).
- The fake MUD drives 21 tests that run the real session loop over a local port.
- The wizard round trip runs 300 random profile sets through the catalog wizard.
- Shared fixtures hold the twins together for word wrap, pane layout cleanup and the GMCP views (2.10).
- The latency pins from the latency run hold what a typed line and its answer leave behind, and which renderer draws the live terminal.

## Part 2. The target structure

### 2.1 Rules that decide where things live

1. One folder per part of the app. If you know what the app does, you can guess the folder.
2. A file holds one job, and its name says that job in today's words.
3. Code that crosses between Rust and the page lives in one place on each side. Rust commands live in `src-tauri/src/ipc`. Page calls into Rust live in `src/ipc`.
4. Lower layers never call upward. Crates know nothing about the app. The session never calls a command. A helper never imports a component.
5. Every command name and every event name is written once on each side, and a test checks the two sides agree.
6. Tests sit beside what they test. A module big enough to need a folder keeps its tests in a tests folder inside it.
7. When a file passes about 800 lines of code, it almost always has more than one job, and the plan splits it. Test files and the prompt test kit are the exception.
8. A job is written once. The twins in 2.10 are the exception, because two renderers draw the same terminal.

### 2.2 Rust crates

This is the layout if you approve D2. With D2 option B all nine crates stay and only get tidied.

| Crate                                 | What lives there                                                                                                                                                                       | Made from            |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| `crates/protocol` (vosh-protocol)     | The wire, with no app state. `telnet/` (parser, negotiation, one subnegotiation framer, codes), `gmcp.rs` with the package names, and `ansi.rs`, which strips ANSI codes to plain text | telnet, gmcp, ansi   |
| `crates/automation` (vosh-automation) | Aliases, variables, triggers, group switches, the command splitter and the revision counter. Plain data with no Lua                                                                    | alias, vars, trigger |
| `crates/script` (vosh-script)         | The sandboxed Lua engine and the `mud.*` API, kept apart so the Lua build stays isolated                                                                                               | script               |
| `crates/log` (vosh-log)               | logs.sqlite. `sessions.rs`, `search.rs`, `lookup.rs`, `sqlite.rs` for the shared pragmas, and `forget/` split into the SQL wipe, the pure login replay and the Aabahran game strings   | log                  |
| `crates/prompt` (vosh-prompt)         | The prompt engine and the prompt editor logic (2.3)                                                                                                                                    | prompt               |
| removed                               | The local map store, if D3 retires it                                                                                                                                                  | map                  |

### 2.3 Inside the prompt crate

| Folder or file | What lives there                                                                                                                                                                                               | Made from                                                                                  |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `engine/`      | PromptEngine, the front door the session holds for each profile. Following Char.Prompt, reading the game's prompt replies, miss counting and status                                                            | engine.rs                                                                                  |
| `config.rs`    | The `[prompt]` table a profile file saves. PromptConfig, CaptureConfig and PromptShow                                                                                                                          | config.rs, unchanged                                                                       |
| `design/`      | The template language. Tokens and pieces, one field name grammar, the writer, and the shared look algebra                                                                                                      | template.rs, parts of edit.rs                                                              |
| `values/`      | The value catalog, value formats (`format.rs` keeps its name), the GMCP snapshot, what is hidden and why, the resolver, sample values and preview overrides                                                    | vars.rs, format.rs, gmcp.rs, overrides.rs                                                  |
| `render/`      | Drawing a design to bytes and spans, plus SGR state                                                                                                                                                            | render.rs                                                                                  |
| `capture/`     | Recognizing a prompt line, regex captures and generic captures                                                                                                                                                 | capture.rs, generic.rs                                                                     |
| `aabahran/`    | Game knowledge only. Codes, colors, lexing, shapes, the observer, and `who.rs` for the compile errors                                                                                                          | aabahran.rs and its folder                                                                 |
| `stage/`       | The terminal byte protocol around the prompt. Output, marks, blocks, reading, drawing, pinning, and the candidates ring it fills on each send and each GA or EOR                                               | stage.rs                                                                                   |
| `card/`        | What the prompt card on the page receives. State, describe, edit, the compile report, presets, the sentences you read, and the candidates view, which groups the ring by shape and checks a capture against it | state.rs, describe.rs, edit.rs, report.rs, presets.rs, candidates.rs, parts of aabahran.rs |
| `testkit/`     | The fake Aabahran plus shared test designs and clocks                                                                                                                                                          | testkit, copies in tests                                                                   |
| `wrap.rs`      | Terminal word wrap, held to the shared fixture                                                                                                                                                                 | unchanged                                                                                  |

The crate root keeps public the modules the app and `crates/prompt/tests` use. Inside each module, items nothing outside uses become `pub(crate)`, so the compiler flags unused code that hides behind `pub` today. Items only tests use sit behind the `testkit` feature (R9).

### 2.4 The app (`src-tauri/src`)

| Folder or file | What lives there                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Made from                                                                                                                                                                                                 |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib.rs`       | The module list and `run()`, about 80 lines                                                                                                                                                                                                                                                                                                                                                                                                                                                              | lib.rs (690)                                                                                                                                                                                              |
| `app/`         | `state.rs` (AppState, including today's process switches), `launch.rs` (every startup step in order), `exit.rs`, `events.rs` (every event name as a constant with its payload and listener noted, plus `broadcast()`), `windows.rs` (window names, the Settings and Help windows, the backdrop, macOS spellcheck), `menu/`, `system_fonts.rs`, `plugins.rs`, `updater.rs`                                                                                                                                | AppState in commands.rs, the setup closure and exit handler in lib.rs, launch.rs, exit_flush.rs, list_events.rs, window_backdrop.rs, `open_aux_window` and its helpers, app_menu.rs, fonts.rs, plugins.rs |
| `ipc/`         | Every command the page calls, as thin wrappers by topic (session, terminal, native surface, automation, loadouts, profiles, characters, ui config, panes, tick, prompt, logs, windows, updater, wizard, affects). `ipc/mod.rs` holds the one registration list                                                                                                                                                                                                                                           | commands.rs, the wrappers in prompt_commands.rs and characters.rs, the single command files                                                                                                               |
| `session/`     | One task per connection. `conn` (the loop), `read` (the socket read path), `batch` (the frame and log rows a burst of reads owes, from the latency run), `steps` (pure per line steps), `prompt_view`, `gmcp`, `gmcp_vars`, `effects` (the one Lua effects applier), `lua_timers`, `log_sink`, `perf`, `lines`, `echo`, `connection`, `room_block`, `highlight_ground`, `tests/`                                                                                                                         | session.rs, gmcp_bind.rs, line_accumulator.rs, hidden_input.rs, connection.rs, room_block.rs, highlight_ground.rs, the five session test files                                                            |
| `output.rs`    | The one path that writes text to the terminal                                                                                                                                                                                                                                                                                                                                                                                                                                                            | session.rs `emit_output`, `emit_session_output`, `emit_repaint`, `emit_counted` and their helpers, commands.rs `echo_lines`                                                                               |
| `input/`       | What happens to a line you type. `mod` (run_line), `slash`, `target`, `prompt`, `automation`, `profile`, `script`, `vars`, `tick`                                                                                                                                                                                                                                                                                                                                                                        | input.rs                                                                                                                                                                                                  |
| `script.rs`    | The bridge between the Lua engine and the live profile. One name, `script`, for the crate, this bridge and `input/script`                                                                                                                                                                                                                                                                                                                                                                                | script_state.rs                                                                                                                                                                                           |
| `tick.rs`      | The tick timer and its one settings shape                                                                                                                                                                                                                                                                                                                                                                                                                                                                | tick.rs plus two copies of its settings                                                                                                                                                                   |
| `prompt/`      | The app side of the prompt. Setting, showing and repainting it, and finding your last game prompt in the logs                                                                                                                                                                                                                                                                                                                                                                                            | the logic in prompt_commands.rs, prompt_lookup.rs                                                                                                                                                         |
| `profile/`     | Everything about a profile. `live.rs` (the profile in memory, runtime state only), `file.rs` (the profile file format), `ui.rs` (its `[ui]` table), `panes.rs` (pane layout, its cleanup and the old dock conversion), `shared.rs` (global.toml and the sharing scope), `set.rs` (profiles.toml and the set), `login_match.rs` (which profile a login picks), `worlds.rs`, `switch.rs` (switching the active profile and the auto switch at login), `inactive.rs` (reading a profile that is not active) | profile.rs, profile_config.rs, profile_set.rs, the switch in commands.rs, characters.rs                                                                                                                   |
| `loadouts/`    | Loadout mode. `catalog.rs` (catalog.toml), `set.rs` (loadouts.toml), `gating.rs`, `presets.rs`, and `wizard/` with `plan.rs` (items and conflicts), `groups.rs` (group naming), `apply.rs` and `journal.rs` (the crash journal)                                                                                                                                                                                                                                                                          | loadout.rs, loadout_store.rs, migration.rs, the wizard in commands.rs                                                                                                                                     |
| `disk/`        | How Vosh writes files. `paths.rs` (one AppDataDir), `atomic.rs` (safe writes and backups), `save.rs` (the save engine and its lock order), `custom_themes.rs`, and `upgrades/`, one ordered list of one time upgrades with every id unchanged                                                                                                                                                                                                                                                            | parts of profile_config.rs, the save code in commands.rs, prompt_migration.rs, preset_rollout.rs, the mudclient copy in lib.rs until D15 retires it                                                       |
| `import/`      | Importers for MUSHclient, Mudlet, GMUD, CMUD and TinTin++, sharing one report type                                                                                                                                                                                                                                                                                                                                                                                                                       | import.rs, tintin_import.rs                                                                                                                                                                               |
| `logs/`        | Opening logs.sqlite and scrollback.txt, and the forget passwords runner                                                                                                                                                                                                                                                                                                                                                                                                                                  | log_state.rs, forget_passwords.rs                                                                                                                                                                         |
| `affects/`     | The affects snapshot and the per character peak cache                                                                                                                                                                                                                                                                                                                                                                                                                                                    | affects_snapshot.rs, affect_full.rs                                                                                                                                                                       |
| `native/`      | The native renderer. `grid/` (the text grid it draws, with regions, find and links), `gpu/` (style, atlas, decor, bands, frame, and `shaders/*.wgsl`), `surface/` (mod, device, pointer, report, the echo frame wait from the latency run, macos, plus windows and linux if D9 keeps them)                                                                                                                                                                                                               | term_grid.rs, cell_render.rs, native_surface/                                                                                                                                                             |
| `color.rs`     | The one place that parses colors                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | three parsers today                                                                                                                                                                                       |
| `tests/`       | Tests that span the crate. Fake MUD, broadcast, echo, wizard round trip, the IPC contract, the config goldens, the upgrade order, `throughput.rs` (P2) and `latency.rs`                                                                                                                                                                                                                                                                                                                                  | the matching files today, and latency_tests.rs from the latency run                                                                                                                                       |

`profile/` is the concept a newcomer looks for. `disk/` only says how files get written safely. The Rust side has no `store` folder, so it never clashes with the page's `stores/`, which hold live data in memory.

### 2.5 The page (`src`)

| Folder                    | What lives there                                                                                                                                                                                                                                                                                      | Made from                                                                                                         |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `main.tsx`, `prepaint.ts` | The entry point and the first themed paint                                                                                                                                                                                                                                                            | unchanged                                                                                                         |
| `shell/`                  | The main window. `MainWindow.tsx` (today's App.tsx cut to about 400 lines) with its hooks useScrollbackSplit, useFind, useAppCommands, useNativeSurfaceBridge and useUiConfigFollow. The title band, session menu, status line, clock, and `overlays/` for the palette, toasts and update notice      | App.tsx, shell/, the top level overlays                                                                           |
| `settings/`               | The Settings window. `SettingsWindow.tsx`, the sidebar, useSettingsAutoSave, and one folder per page (general, appearance, layout, input, automation, characters)                                                                                                                                     | SettingsApp.tsx, settings/groups, settings/pages, settings/rows, the hook in settings/legacy                      |
| `help/`                   | The Help window. `HelpWindow.tsx`, its parts and the HELP.md reader                                                                                                                                                                                                                                   | HelpApp.tsx, help/, helpContent.ts, helpNav.ts                                                                    |
| `ipc/`                    | Every call into Rust and every event, one file per topic, plus one useTauriEvent hook                                                                                                                                                                                                                 | session.ts, raw invokes in App, Terminal, Input and others                                                        |
| `stores/`                 | Live data from the game and from settings, in memory. `gmcp/`, `config/`, `session/`                                                                                                                                                                                                                  | lib/stores, chatStore, groupStore, immStore, toasts                                                               |
| `terminal/`               | Terminal.tsx with terminalRenderer.test.ts. `xterm/` (lift bands, and `xtermMirror.ts` from the latency run), `native/`, the output pipeline (region writer, output shaper, word wrap, rows), terminal colors and cells, the find bar, the scroll depth chip, the right click menu, the history split | Terminal.tsx, `terminal*.ts`, outputShaper, wordWrap, sgrCells, bandCells, promptBands, Resizable, xtermMirror.ts |
| `input/`                  | The command line and its hooks (caret, preferences, macro keys, tab completion, history)                                                                                                                                                                                                              | Input.tsx, recentNames, maskedInput                                                                               |
| `panel/`                  | The right panel and each pane. `affects/`, `map/`, `group/`, `chat/`, `imm/`, the vitals footer                                                                                                                                                                                                       | panel/, ServerMapView, MapPaneControls                                                                            |
| `prompt/`                 | The pinned prompt dock and the prompt card, with the prompt logic beside them                                                                                                                                                                                                                         | components/prompt, `lib/prompt*.ts`                                                                               |
| `theme/`                  | Colors, built in themes, the theme runtime, the paint cache, theme import                                                                                                                                                                                                                             | color, chrome, themes, theme, themePaint, themeImport, themeThumb, appearanceSettings                             |
| `automation/`             | Logic for triggers, aliases, macros, timers, presets and loadouts, shared by Settings and the palette                                                                                                                                                                                                 | `automation*.ts`, presets, colorTokens, macroKeys, wizardPresets, tickDraft                                       |
| `ui/`                     | The kit every window shares. Buttons, rows, fields, toggles, menus, keycaps, one icon set, the confirm dialog, the code editor, window controls                                                                                                                                                       | settings/ui, panel/MenuSurface, three icon sets, shared top level components                                      |
| `lib/`                    | Small helpers used in several places. Platform, shortcuts, text, hex colors, cell widths, the escape stack, focus, plus useWindowBoot and the one deep link helper all three windows use                                                                                                              | lib/, settingsLink.ts, helpLink.ts                                                                                |
| `styles/`                 | See 2.6                                                                                                                                                                                                                                                                                               |                                                                                                                   |
| `test/`                   | Shared test fixtures and one Tauri mock                                                                                                                                                                                                                                                               | src/test, copied mocks                                                                                            |

Each window's root sits in the folder that holds its contents, so you find Settings in `settings/` and nowhere else.

### 2.6 Styles (`src/styles`)

main.tsx imports one file, `styles/index.css`. Every custom property lives in tokens.css. A class prefix tells you which file holds its rule.

| File            | What lives there                                                                                                                               | Made from                                                                                                                 |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `index.css`     | The one entry, in cascade order, with a header naming which prefix lives where                                                                 | index.css plus the separate frame.css import                                                                              |
| `fonts.css`     | The bundled mono fonts                                                                                                                         | the `@font-face` blocks at the top of styles.css                                                                          |
| `base.css`      | Reset, html and body, global scrollbars                                                                                                        | the base section of styles.css                                                                                            |
| `tokens.css`    | Every custom property. Colors, radii, metrics, fonts, the floating surface recipe, one highlight color and one key ring. No `--c-*` names left | tokens.css and the `:root` blocks in four files                                                                           |
| `controls.css`  | The `st-` controls every window uses                                                                                                           | the Root control fills and the Controls section of settings.css                                                           |
| `overlays.css`  | `ov-` floating surfaces and the one menu recipe                                                                                                | overlays.css                                                                                                              |
| `frame.css`     | `shell-` names, the main window frame                                                                                                          | frame.css                                                                                                                 |
| `terminal.css`  | `.terminal-area`, the history split, the macOS underlay list, the split handle                                                                 | the terminal and split scrollback sections of styles.css, the live `.resizable*` rules, the Terminal section of frame.css |
| `input.css`     | `.input-row` (including its font rule), caret shapes, the caret mirror, the paste flash                                                        | the Input row section of styles.css, the Input band section of frame.css                                                  |
| `panel.css`     | `pane-` and `panel-` names, with affects optionally in its own file                                                                            | panel.css                                                                                                                 |
| `map.css`       | The map pane and the map view internals                                                                                                        | the Map section of panel.css, the ServerMapView internals section of styles.css                                           |
| `prompt.css`    | `prompt-` and `pc-` names only                                                                                                                 | prompt.css without its Settings section                                                                                   |
| `settings.css`  | The `st-` window frame and every Settings page, now with the Input prompt section                                                              | settings.css, the Settings, Input, Prompt section of prompt.css                                                           |
| `help.css`      | `hp-` names                                                                                                                                    | help.css                                                                                                                  |
| `migration.css` | The wizard's interim styles, until D23 retires them                                                                                            | the Migration wizard section and the live `.settings-app` rules of styles.css                                             |

styles.css goes away. CSS layers stay out, because xterm's and CodeMirror's own styles would then beat every layered rule.

### 2.7 Docs

| File                      | What lives there                                                                                                                                                                                                                                    | Made from                                           |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| `README.md`               | What Vosh is, how to install and build, and the protocols it really supports (telnet with RFC 1143, TTYPE and MTTS, NAWS, NEW-ENVIRON, CHARSET, GMCP)                                                                                               | README.md without the MSDP, MCCP and MXP claims     |
| `CONTRIBUTING.md`         | Setup, the CI gates in CI order, commit rules, where tests go, the switches that regenerate fixtures, the help and CHANGES rules                                                                                                                    | CONTRIBUTING.md                                     |
| `CLAUDE.md`               | Agent rules, the stack, a true repo layout, and a short current state that names the current R phase                                                                                                                                                | CLAUDE.md without the phase diary                   |
| `HELP.md`                 | The one copy of the help text, with D11 option A                                                                                                                                                                                                    | HELP.md and helpContent.ts                          |
| `CHANGES.md`              | Release notes, as today                                                                                                                                                                                                                             | CHANGES.md                                          |
| `docs/refactor-plan.md`   | This plan with its status table, until the refactor ends                                                                                                                                                                                            | this file, at R0                                    |
| `docs/requirements.md`    | The 1.0 requirements, the anti goals and the writing style, with what D3, D29 and D40 change                                                                                                                                                        | prompt.md                                           |
| `docs/architecture.md`    | The crate and module map, the command and event catalog, the lock order, which work runs on which thread, the twins, and two traces for newcomers (a typed line from the command line to the socket, and a game line from the socket to the screen) | new in R7, and each later phase updates it          |
| `docs/data-files.md`      | Every file Vosh writes, its format, which upgrades touch it, what must stay readable, and the rollback rule (D14)                                                                                                                                   | new                                                 |
| `docs/renderer.md`        | The native surface as it works now                                                                                                                                                                                                                  | docs/native-renderer.md without its milestone diary |
| `docs/history/`           | prompt.md and the renderer milestone log, if you want them kept                                                                                                                                                                                     | prompt.md, the diary in docs/native-renderer.md     |
| `fixtures/README.md`      | Only folders that exist, each with the test that reads it                                                                                                                                                                                           | fixtures/README.md                                  |
| `examples/lua/combat.lua` | The sample script, with its folder path fixed                                                                                                                                                                                                       | scripts/combat.lua                                  |

### 2.8 Names that change

| Today                                                   | Becomes                                                                      | Why                                                                                                     |
| ------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `commands.rs`                                           | `ipc/<topic>.rs` plus `app/`, `profile/`, `loadouts/` and `disk/`            | It held ten jobs                                                                                        |
| `session.rs`, `input.rs`                                | `session/`, `input/`                                                         | Same reason                                                                                             |
| `term_grid.rs`, `cell_render.rs`, `native_surface/`     | `native/grid/`, `native/gpu/`, `native/surface/`                             | One home for the native renderer. Its `gpu` no longer clashes with the prompt crate's `render`          |
| `profile.rs`, `profile_config.rs`, `profile_set.rs`     | `profile/live.rs`, `profile/file.rs` and their neighbors                     | One folder for the profile concept                                                                      |
| `loadout.rs`, `loadout_store.rs`                        | `loadouts/`                                                                  | They are loadout mode's files                                                                           |
| `migration.rs`                                          | `loadouts/wizard/plan.rs` and `groups.rs`                                    | It is the wizard planner, not a migration                                                               |
| `prompt_migration.rs`, `preset_rollout.rs`              | `disk/upgrades/`                                                             | One list of one time upgrades                                                                           |
| `prompt_commands.rs`                                    | `ipc/prompt.rs` and `prompt/`                                                | Wrappers and logic apart                                                                                |
| `script_state.rs`                                       | `script.rs`                                                                  | One name for the Lua bridge, matching vosh-script                                                       |
| `fonts.rs`, `app_menu.rs`                               | `app/system_fonts.rs`, `app/menu/`                                           | fonts.rs is the font picker list, not the renderer's fonts                                              |
| `latency_tests.rs`                                      | `tests/latency.rs`                                                           | Crate wide tests live together                                                                          |
| `install_probe`                                         | `install`                                                                    | It is the real install                                                                                  |
| `PATH_B_ACTIVE`, `persist_path_b`, `path_b_mode_active` | loadout mode names                                                           | The UI and docs call it loadout mode. The wire field `path_b_active` stays through a serde rename (D17) |
| vosh-prompt `vars` module                               | `values`                                                                     | It clashes with the vosh-vars crate and the Vars struct (D25)                                           |
| vosh-prompt `Vosh` struct                               | `ClientValues`                                                               | It holds what Vosh itself supplies (D25)                                                                |
| `App.tsx`, `SettingsApp.tsx`, `HelpApp.tsx`             | `shell/MainWindow.tsx`, `settings/SettingsWindow.tsx`, `help/HelpWindow.tsx` | Each window's root sits with its contents                                                               |
| `ServerMapView`                                         | `panel/map/MapView`                                                          | It only ever renders inside the map pane                                                                |
| `settings/ui`                                           | `ui`                                                                         | The prompt card, Help and Settings all use it                                                           |
| `settings/legacy/useSettingsAutoSave`                   | `settings/useSettingsAutoSave`                                               | It is the live save path for every page                                                                 |
| `settings/groups/*Group`                                | `settings/<page>/<Page>Page`                                                 | Groups and pages are the same kind of thing                                                             |
| `promptBand.ts`, `promptBands.ts`                       | `prompt/pinnedDock.ts`, `terminal/xterm/liftBands.ts`                        | Two names one letter apart for different jobs                                                           |
| `xtermMirror.ts`                                        | `terminal/xterm/xtermMirror.ts`                                              | It belongs to the xterm side of the terminal                                                            |
| session.ts `GroupState`                                 | `GroupToggle`                                                                | It clashed with the group roster type                                                                   |
| `TerminalHandle.debug()`                                | `contentSize()`                                                              | The split reveal depends on it, so it is not debug                                                      |

### 2.9 What never changes

- Every live Tauri command name. The page calls commands by the Rust function name, so functions may move between files but keep their names and parameter names.
- Every live event string, including `session://gmcp/<Package>`.
- Every serde field name in files on disk and in payloads. That includes `pattern` in trigger files, `path_b_active`, the `[ui]` prompt copy, the `migrations` ids in profiles.toml, the `.bak.<ms>` and `.before-prompt-editor` suffixes and the wizard marker table name in logs.sqlite.
- Every browser storage key, such as `vosh.cache.themePaint`, `vosh.palette.recent` and `vosh.nativesurface`.
- The Lua API. All 18 `mud.*` names.
- The OSC 7717 marks both renderers read.
- The telnet bytes Vosh sends, including the TTYPE string that carries the version.
- Every Settings anchor and help topic id, because deep links, the palette and `#help` use them.

### 2.10 Kept on purpose. The Rust and TypeScript twins

Two renderers draw the same terminal, xterm on the page and the native grid in Rust. These jobs therefore exist twice on purpose. Rule 8 does not merge them. Any change to one side lands on the other side in the same commit, and the named fixture proves they agree.

| Job                                           | Rust side                                                        | Page side                                                     | What holds them together                                                                                                                                                                                      |
| --------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Word wrap                                     | `crates/prompt` `wrap.rs`, which the native grid calls           | `terminal/wordWrap.ts` WordWrapper                            | `fixtures/wrap/cases.json`, read by both test suites                                                                                                                                                          |
| Prompt regions, holds and pins                | `native/grid/regions.rs`                                         | `terminal/terminalRegion.ts` RegionWriter                     | `fixtures/prompt/aabahran/pinned/splits.b64`, `preview/splits.b64` and `pointer/cases.json`. The session tests write the native grid's screens, and the page tests replay the same payloads into a real xterm |
| Lift bands under a prompt                     | `native/gpu/bands.rs` (BAND_X, BAND_Y, MAX_LIFT_ROWS)            | `terminal/xterm/liftBands.ts` layoutBands                     | Matching constants with comments that point at each other. No shared fixture today, so R2 adds one                                                                                                            |
| Rows the grid keeps and rows the game is told | `native/surface/report.rs` `grid_and_game_rows`, `short_of_band` | `terminal/terminalRows.ts` keptRows, gameSize                 | Unit tests on each side only. R2 adds a shared case file                                                                                                                                                      |
| Pane layout cleanup                           | `profile/panes.rs` sanitize                                      | `panel/paneLayout.ts` sanitizeLayout                          | `fixtures/pane-layout/sanitize.json`                                                                                                                                                                          |
| GMCP views                                    | `crates/prompt` `values/` (tests/views.rs)                       | the GMCP stores (aabahranViews.test.ts, hiddenStore.test.tsx) | `fixtures/gmcp/aabahran/views.json` and `lament.json`                                                                                                                                                         |

Four smaller pairs are kept in step by hand today. R2 adds a test for each that reads both sides, the way a Rust test already reads presets.ts.

- The known worlds, `profile_set.rs` KNOWN_WORLDS and useConnection.ts.
- The presets off marker, `loadout_store.rs` PRESETS_OFF and automationRecords.ts PRESETS_OFF_MARKER.
- The preset library, which lives only in presets.ts and which three Rust tests read with `include_str!`.
- The UI config defaults, Rust's serde defaults and session.ts `normalizeUiConfig`. R19 retires most of the page copy.

## Part 3. Dead code to remove

Every item here was found by one auditor and confirmed by a second. Each group lands as its own commit, and you see the diff before any file is deleted, as CLAUDE.md requires. Items that need a decision first say which one.

### 3.1 Page files nothing loads (R4)

An import graph from `src/main.tsx`, the only page entry, never reaches these 26 files. A scratch copy with all of them deleted passed typecheck and all 140 test files.

| File                                                            | Lines | Replaced by                 |
| --------------------------------------------------------------- | ----- | --------------------------- |
| `components/VitalsSettings.tsx`                                 | 1,629 | Layout, Vitals rows         |
| `components/TriggerForm.tsx`                                    | 916   | TriggersEditor              |
| `components/settings/legacy/LegacyEditors.tsx`                  | 874   | The new Settings pages      |
| `components/ThemesTab.tsx`                                      | 689   | Appearance page             |
| `components/ProfilesTab.tsx`                                    | 572   | Characters page             |
| `components/AliasForm.tsx`                                      | 357   | AliasesEditor               |
| `components/MacrosTab.tsx`                                      | 330   | MacrosEditor                |
| `components/TimersTab.tsx`                                      | 259   | TimersEditor                |
| `components/LogsTab.tsx`                                        | 258   | General, Session logs       |
| `lib/ansi.ts`                                                   | 250   | sgrCells and logView        |
| `components/ImportTab.tsx`                                      | 242   | ImportPanel                 |
| `components/TopBar.tsx`                                         | 188   | The title band              |
| `components/LoadoutsTab.tsx`                                    | 182   | LoadoutsEditor              |
| `components/settings/groups/AutomationGroup.tsx`                | 168   | AutomationPage              |
| `components/Connect.tsx`                                        | 153   | Session menu and General    |
| `components/TrackedAffectsEditor.tsx`                           | 143   | Characters, Tracked affects |
| `lib/vitalsTemplate.ts`                                         | 122   | nothing                     |
| `components/TopBarLoadouts.tsx`                                 | 115   | nothing                     |
| `components/settings/groups/CharactersGroup.tsx`                | 90    | CharactersPage              |
| `components/Icons.tsx`                                          | 44    | ui icons                    |
| `components/settings/pages/appearance/SentCommandColorRow.tsx`  | 40    | the row inlined in Input    |
| `components/settings/pages/appearance/SplitDividerColorRow.tsx` | 40    | the row inlined in Layout   |
| `lib/usePersistedSet.ts`                                        | 37    | nothing                     |
| `components/settings/groups/AppearanceGroup.tsx`                | 26    | AppearancePage              |
| `lib/unsaved.ts`                                                | 21    | nothing                     |
| `components/UnsavedDot.tsx`                                     | 11    | nothing                     |
| Total                                                           | 7,756 |                             |

### 3.2 Page code that dies with them (R4)

| Item                                                                    | Lines      | Evidence                                                                                         |
| ----------------------------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------ |
| `lib/vitalsColor.ts`                                                    | 146        | Its only users are VitalsSettings and a dead function in vitalsLayouts                           |
| `lib/vitalsLayouts.ts`                                                  | 84         | Only VitalsSettings uses it, except two thresholds that move into vitalsStore.ts                 |
| `lib/panels.ts`, and `layoutFromDock` with its helpers in paneLayout.ts | 355 and 65 | Only paneLayout.test.ts calls them. Rust `PaneLayoutPersist::from_dock` does the real conversion |
| ServerMapView's non embedded mode                                       | about 240  | The one caller, MapPane, always passes `embedded`                                                |
| `lib/terrainDecor.ts`                                                   | 333        | Called only inside that dead branch                                                              |
| Resizable's horizontal, left and right variants                         | about 70   | The one caller passes `anchor="top"`                                                             |

### 3.3 Page exports and code nobody calls (R4)

- `formatModifier` (affects.ts), `baseAnsiList` (baseAnsi.ts), `useHidden` (hiddenStore.ts), `usePanelOpen` and `useShownPanes` (panelLayoutStore.ts), `addPaneToPanel` (paneActions.ts). Each has zero references. App.tsx redoes the last three inline, so R17 either uses them or drops them.
- The `vosh:connect-request` listener in useConnection.ts. Nothing sends that event.
- `TerminalMenu`'s `connected` prop and `Card`'s `padded` prop with `.st-card-padded`. Nothing reads or passes them.
- The orphan comment in App.tsx about the custom prompt rendering in the backend, which sits on no code, and a duplicate import of `./lib/session`.
- The Char.State and weather stores, 56 and 71 lines. They run at launch and nothing shows them. Needs D27.
- The temporary split debug overlay and the live terminal counter, about 60 lines. Needs D20.

### 3.4 Events nobody hears (R4 and R6)

| Event                                                          | Sent from                                                                                                  | Note                                                                                                        |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `vosh://vitals-config-changed`                                 | session.ts `broadcastUiConfigChanges`                                                                      | No listener anywhere                                                                                        |
| `vosh://moons-position-changed`                                | session.ts `broadcastUiConfigChanges`                                                                      | No listener. One test asserts it fires and changes with it                                                  |
| `vosh://side-panels-fill-height-changed`                       | session.ts `broadcastUiConfigChanges`                                                                      | Its only listener has no caller                                                                             |
| `vosh://dock-layout-changed`                                   | commands.rs `dock_layout_set`                                                                              | Dies with that command                                                                                      |
| `vosh://alias-groups-changed`, `vosh://trigger-groups-changed` | commands.rs `aliases_set_group_enabled`, `triggers_set_group_enabled`, `aliases_import`, `triggers_import` | Only dead files listen. The sends inside the two live import commands go as single lines                    |
| `vosh://macro-groups-changed`                                  | commands.rs `macros_set_group_enabled`                                                                     | Stays. Its listener in Input.tsx is live, and bug 3's fix in R3 sends it from `#group` and loadout switches |

### 3.5 Backend commands nothing calls (R6)

Each command goes with its registration in lib.rs, its page wrapper in session.ts and any event only it sent.

| Command                                                                                                  | Lines                             | Page wrapper                                             | Needs                                                                                           |
| -------------------------------------------------------------------------------------------------------- | --------------------------------- | -------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `session_send`                                                                                           | 14                                | sendBytes, sendLine                                      | nothing                                                                                         |
| `profile_export`, `profile_import`                                                                       | 26                                | exportProfile, importProfile                             | nothing. CLAUDE.md's Phase Status text goes with them                                           |
| `dock_layout_get`, `dock_layout_set`                                                                     | 31                                | dockLayoutGet, dockLayoutSet, subscribeDockLayoutChanged | nothing. The `dock_layout` field on disk stays (D13)                                            |
| `logs_search`                                                                                            | 19                                | searchLogs                                               | nothing. `LogStore::search` becomes test only                                                   |
| The `profile` argument of `pane_layout_get`, and `characters::inactive_pane_layout`                      | 25                                | paneLayoutGet in session.ts                              | nothing. The command itself stays, since paneLayout.ts `getPaneLayout` calls it on every launch |
| `app_version`                                                                                            | 13                                | none, only a test calls it                               | nothing                                                                                         |
| `aliases_groups_list`, `aliases_set_group_enabled`, `triggers_groups_list`, `triggers_set_group_enabled` | 54                                | four wrappers and two listeners                          | D6                                                                                              |
| `macros_set_group_enabled`                                                                               | 24                                | setMacroGroupEnabled                                     | D6. Its event stays                                                                             |
| `profile_set_metadata`                                                                                   | 22                                | profileSetMetadata                                       | D7                                                                                              |
| `plugins_list`, `plugins_set_enabled`, `plugins_reload`, PluginInfo                                      | 93                                | three wrappers                                           | D5                                                                                              |
| `map_walk_to`, `map_set_note`, `map_set_avoid`                                                           | 65                                | walkToRoom, setRoomNote, setRoomAvoid                    | D3                                                                                              |
| Total                                                                                                    | about 390 Rust and 120 page lines |                                                          |                                                                                                 |

### 3.6 App code that never runs (R6 and R13)

- `sync_target_var` and its four calls, 21 lines. `refresh_target_idx` already does the same thing right before each call.
- `accumulator.reset()` on the disconnect path of `io_loop`, with `LineAccumulator::reset`, 5 lines. The accumulator is already empty and is dropped right after.
- `MapState.previous_room_id`, 4 lines. Written, never read.
- The mirror in `Profile::set_prompt_config`, 7 lines. Nothing reads the live copy, and the saved copy is rebuilt from the engine anyway.
- `Loadout.profile_vars`, `Loadout.tick`, `Loadout.connection` and `ConnectionConfig`, 35 lines. Never read, but every loadouts.toml save writes empty tables for them. Removing them changes the written bytes, so they go with D12.
- `#![allow(dead_code)]` in loadout.rs, loadout_store.rs and migration.rs. With the allows removed the compiler reports nothing, so they only hide future dead code.
- The two `#[allow(dead_code)]` in term_grid.rs that hide `TermGrid::char_at` and `TermGrid::cell`. Only tests call those two, so they move under `cfg(test)`.
- The `tauri-plugin-shell` plugin and dependency. No permission grants it and nothing uses it.
- The 20 stand in functions for a build without the native surface, about 90 lines plus 30 attributes. They go only if D9 keeps the surface on every desktop target (R6). With D9 option B, Windows and Linux need them, so they stay.
- The macOS on top surface code. Under the macOS underlay the view never receives mouse events, so the mouse handlers, cursor rects, `set_frame`, two install branches and `after_redraw` on all three platforms never run, about 235 lines. R13, because `set_frame` needs the underlay switch turned into a platform check first.
- `Uniforms.cell_size`, which neither shader reads. It becomes a named pad so the uniform keeps its size. R13.
- The stray doc comment about macro bindings that sits above `import_detect` and documents the wrong function.
- The `"scrollback.bin"` name in the mudclient folder copy, which can never match because the file has always been scrollback.txt. Goes with the copy (D15).
- The mudclient folder copy itself, 86 lines. The first tagged release already used the vosh folder. D15, and it must leave no later than the map store.
- The `VOSH_WRITE_PLAYS` exporter, 31 test lines, whose screenshot harness is not in the repo. Needs D20, which removes it.
- Stale comments in profile.rs and session.rs that say quick keys persist. They reset on restart, as HELP.md says.
- An empty untracked folder `src-tauri/src/session/`. A local folder removal only, nothing to commit.

### 3.7 Small crates (R7 and R8)

| Crate   | Item                                                                                                                                                                                          | Lines            |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- |
| ansi    | The whole SGR attribute model (sgr.rs, color.rs, span attributes, `current_attributes`, `reset`). The app only uses `plain_text`, whose output never depends on attributes                    | 195              |
| telnet  | `thiserror` dependency and an empty dev table. Unused protocol constants may stay as a reference table                                                                                        | 2 to 18          |
| alias   | `Alias::with_script`, and the separator setting that nothing ever changes from `;`                                                                                                            | 14               |
| trigger | `TriggerError::NotFound`, `TriggerStore::preset_ids`                                                                                                                                          | 12               |
| script  | `Action::DropCallback`, `ScriptError::InvalidRegex`, the `serde` dependency, and the always true `enabled` and always zero `priority` on Lua triggers. `#scripts` output stays byte identical | 22               |
| script  | `drop_callback` duplicates a private twin. Merge the two, do not delete                                                                                                                       | 6                |
| log     | `LogError::Io`, serde derives that never deserialize, and items marked public that only the crate uses                                                                                        | 8                |
| map     | `set_position`, `exits_in_area`, `room_count`, unused serde derives. With D3 option A the whole crate goes, about 545 lines plus 154 in map_state.rs and about 120 lines of plumbing          | 36, or about 820 |

### 3.8 Prompt crate (R9)

Dead everywhere.

- `PromptConfig::take_switch_and_template` and `take_show`, 37 lines plus about 70 test lines. Their own comments call them orphans.
- `MapValues::now`, 4 lines.
- Seven root exports nothing uses (Clock, SeenKind, StatusReport, Position, Format, Hidden, Resolver).
- `Packet::seq` and the snapshot's sequence counter, 8 lines, read only by one test.
- Comments that point to `src-tauri/src/prompt_template.rs`, which no longer exists.

Fields the page receives and never reads. Needs D24.

- `Entry::gmcp` with its 61 catalog strings and `FieldState::gmcp`, 66 lines.
- `FieldState::formats`, `CompileReport::vars` and `codes`, `ReportShape::label`, `kind` and `which`, about 16 lines.

### 3.9 Styles (R5)

| What                                                      | Selectors                                                                                                                                                                                                                                                  | Lines       |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| Ember title bar chrome                                    | `.brand-*`, `.topbar*`, `.topbar-loadouts*`, `.settings-search`, `.session-chip*`, `.session-menu*`, `.main-row`, `.terminal-column`                                                                                                                       | 490         |
| Small orphan blocks and panel zones                       | `.kbd`, the `.vitals-bar` and `.vitals-row-template` font rule, `.find-toolbar input`, `.panel-zone*` with its card tints, `.settings-caret-picks`, `.caret-sample*`, `.right-column`, `.group-pinned-resizable`                                           | about 190   |
| The old vitals bar                                        | `.vitals-*` with the keyframes vitals-ledger-pulse, vitals-drain-danger-pulse and vitals-badge-danger-pulse                                                                                                                                                | 957         |
| The Settings window before milestone 2                    | `.settings-sect`, `.settings-frow`, `.settings-flabel`, `.settings-shell`, `.settings-tabs*`, the old trigger and alias rows, font picker, tracked affects and updates                                                                                     | 1,495       |
| Panels tab, vitals style rows and legacy rows             | `.settings-color-row`, `.settings-paste-row`, `.panels-*`, `.chip-style-*`, the sticky save bar, keyframes panels-cursor-blink                                                                                                                             | 741         |
| Old tabs and the old update notice                        | `.macros-*`, `.timers-*`, the trigger and alias group inputs, `.update-notice*`, `.theme-card*`, `.profiles-*`, `.settings-btn-mute`, `.settings-btn-danger`                                                                                               | 743         |
| Import tab and the unsaved tag                            | `.import-*`, `.settings-saved`, `.settings-unsaved*`, keyframes vosh-unsaved-pulse, `.settings-autosave-hint`, `.settings-error`, `.settings-loading`                                                                                                      | 133         |
| Old overlays and the loadouts tab                         | `.loadouts-*`, `.loadout-row*`, `.find-toolbar*`, `.imm-chip-*`, `.palette*`, `.well-splits*`, `.terminal-menu*`, `.toast*`, `.confirm-*`, `.base-ansi*`, `.settings-size-*`. The live versions use `ov-` names in overlays.css                            | 836         |
| Rules that can never match once R4 removes their branches | `.caps`, `.map-subhead*`, `.map-controls-*`, `.tileset-bar`, `.tileset-status`, `.map-mode-toggle`, `.map-zoom*`, `.resizable-horizontal`, `.resizable-anchor-left`, `.resizable-handle-horizontal`, and the `.settings-app` text field and checkbox rules | about 270   |
| Values a later sheet always overrides                     | the `--c-*` hex values in styles.css `:root`, the 44px `.terminal-area-find-inset`, `.input-row .prompt`, the `.input-caret` glow, the `.terminal-area` padding, the `[data-platform='windows'] .app` radius                                               | about 40    |
| settings.css leftovers                                    | `.st-note` and the interim `.st-legacy` block                                                                                                                                                                                                              | 23          |
| Token aliases nothing reads                               | `--c-surface-lift`, `--c-warn`, `--c-danger`, `--c-info`, `--c-success` in tokens.css. Unread only after the styles.css cuts, so they go last                                                                                                              | 5           |
| Total                                                     |                                                                                                                                                                                                                                                            | about 5,900 |

Keep the `.input-row` font rule. It sits between two dead blocks in the shared section near the top of styles.css, and it is the only rule that puts the command line in your terminal font and size. R18 moves it into input.css.

The Roboto Slab and Rajdhani font imports in main.tsx and their two dependencies go too. Every rule that uses them is in the list above. Inter stays, because html, body and the migration wizard still use it.

### 3.10 Files and docs (R4 and R23)

- `NEXT_PHASES.md`, 79 lines describing May plans and UI that is gone.
- `loop-prompt.txt`, an agent prompt for a renderer branch that shipped long ago.
- `.docking_baseline/`, three Phase 0 notes about components that no longer exist.
- `mockups/`, 20 Ember era HTML files, about 4,000 lines. Git history keeps them.
- `scripts/combat.lua`. Nothing loads it, but HELP uses "combat" as an example, so it moves to `examples/lua/` with its folder path fixed.
- The iOS, Android and Windows Store icons. Desktop builds never read them. Needs D8.
- Stale lines in fixtures/README.md about telnet, ansi and mccp folders, fixed in R23.

D31 covers the first five. The icons need D8, and R23 fixes the README lines.

### 3.11 Test only code (fence it, do not just delete it)

These items are only used by tests. Items only a crate's own unit tests use move under `#[cfg(test)]`. Items that tests in another crate, or in a crate's `tests/` folder, use go behind a `testkit` feature with `#[cfg(any(test, feature = "testkit"))]`, the pattern vosh-prompt already uses. R7 adds that feature to vosh-log and any other crate that needs it.

- Page. `moonLabel`, `parseCharacterNames`, `formatCharacterNames`, `parsePort`, the appearanceSettings `colorInputValue`, `oklchToRgb` with the four CIEDE2000 functions, `pieceRange`. Keep `escapeDepth` and `resetAppMenuState` as labeled test hooks, and keep the views exported for their tests.
- Small crates. `Parser::reset`, `reset_ttype_cycle`, `gmcp::build_raw`, `Alias::with_group`, `with_max_depth`, `TriggerStore::len`, `vosh_trigger::process` and `process_scoped`, `MapStore::list_area`, `direction::reverse`, `forget_passwords`, `is_local_host`, `LogStore::search`. `LogStore::in_memory`, `append_raw`, `get_session` and `session_character` serve the app's tests, so they sit behind vosh-log's new `testkit` feature.
- Prompt crate. `MapValues` (to testkit), `Stage::stale`, `pin_drawn`, `repaint`, `set_capture`, `card_open`, `shows`, `pinned`, `swallows`, `Recognizer::codes`, `settles`, `compile`, `hidden`, `disagreements`, `Hidden::none`, `Snapshot::new`, `colors::Color::sgr`, `Template::is_empty`, `Span::look`, and the root exports MapValues and SpanColor. `Vars::new` (61 uses in `crates/prompt/tests`) and the root export Vars sit behind `testkit`.
- App. `input::process` stays as a test helper. The Forsaken Lands test port seam and the render counter go in R11 once `spawn` takes the known host flag from its caller.

Once `LogStore::append_raw` is test only, vosh-log no longer needs vosh-ansi outside tests. vosh-trigger still does, for `pieces`.

### 3.12 Looks dead, is live. Do not remove

The second reader and the critics caught these. Each one would break something if deleted.

- `Compiled::prompt` and `fprompt` in the prompt crate. `#prompt game` saves them.
- `CharPrompt::at_login`. The prompt last seen command sends it to Settings.
- `Vars::remove_script` and `CaptureSource::Log`. Lua and saved files use them.
- `ScriptEngine::drop_callback`. Every Lua timer fire calls it.
- The `band_bind_group` texture entries. The GPU requires them while the band pipeline shares the layout.
- `useSettingsAutoSave`, even though it sits in a folder called legacy.
- `MigrationWizard`, `.settings-app`, `.settings-btn`, `migration-*` and the `--c-*` tokens. ImportPanel opens the wizard inside `.settings-app`.
- The `.input-row` font rule in styles.css (3.9).
- `pane_layout_get`. Every launch calls it. Only its `profile` argument is dead.
- `vosh://macro-groups-changed` and its listener in Input.tsx. Bug 3's fix uses them.
- `LoadoutsState` in session.ts. `loadoutsGetState` returns it.
- `connectTo`, `useCharacterName` and `loadSystemFont`. Only their exports become unneeded.
- `ProfileEntry.description`. Profile duplication copies it and you have text there. Only the setter command dies.
- `Loadout.auto_match`. The Loadouts editor shows it, though nothing writes it or matches on it (D33).
- `ScopeConfig.dock_layout` and every `dock_layout` field. The lazy dock to panes conversion reads them, and 0.7.2 reads them on a rollback.
- The trigger serde shim that reads the old `pattern` and `action` fields.
- `input.rs` `profile_path` and `script_path_for`. They are live and buggy (bug 4), not dead.
- The `persist_state_with` fallback. It is reachable and harmful (bug 5), not dead.
- The `/__vosh-dbg` sink in vite.config.ts. It is a dev only tool that driver scripts use.
- `LEDGER_LOW_ENTER` and `LEDGER_LOW_EXIT`, which vitalsStore uses.
- `shell-moon`, which no CSS styles but StatusClock.test.tsx uses to find the moons.

## Part 4. The phases

### 4.1 Rules every phase follows

Gates, all green before a phase lands.

1. `npm run format:check`
2. `npm run lint`
3. `npm run typecheck`
4. `npm test`
5. `cargo fmt --all -- --check`
6. `cargo clippy --all-targets --all-features -- -D warnings`
7. `cargo test --workspace --all-features`
8. The IPC contract test from R2.
9. CI on macOS, Windows and Linux for any phase that touches platform code. Code behind a platform check only compiles on that platform's runner, so a Mac alone can't prove it.

The perf set, recorded in R1 from the latency run's head and checked again after every phase that touches the session, the renderer or the page's output path. A phase may not make any number worse beyond normal noise. If it does, the phase stops.

| #   | Measure                                                 | How                                                                                                 |
| --- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| P1  | Your echo on screen, and the reply on screen, in frames | Release build against the fake MUD, measured the way the latency run measured it                    |
| P2  | Output throughput                                       | A captured session pushed through the session loop to the grid, a benchmark test skipped by default |
| P3  | Launch to the first themed paint                        | Release build                                                                                       |
| P4  | Opening Settings and the Appearance page                | Release build                                                                                       |
| P5  | Searching a week of logs                                | A benchmark over a heavy week written through the real LogStore, skipped by default                 |

Running P2 and P5. Both are tests that `cargo test` skips. Run each in a release build, and compare its best run with the numbers recorded before the phase.

```
cargo test -p vosh-app --release --lib p2_ -- --ignored --nocapture
cargo test -p vosh-log --release --test p5_search -- --ignored --nocapture
```

- P2 lives in `src-tauri/src/tests/throughput.rs`. It plays the fake Aabahran's greeting, `login-new` and 3,000 rounds of the `quiet`, `fight-tank` and `lament-new` wire reads, each round closed by a numbered pulse, about 9.5 MB in all. The game writes it at once, and the real session loop takes it to the native grid with the default design drawn and every row logged. Five runs print the time to the grid and MB per second, then the best and the median.
- P5 lives in `crates/log/tests/p5_search.rs`. It writes the Phase 10 check's heavy week through the real LogStore, 702,987 rows in an 83.5 MB file, then times the session list, nine searches the Settings log view runs and the page before the first. Each search runs once on a fresh connection and five more times warm. A run that writes the week leaves the whole file in the system cache, so every number it prints is a cached read. For cold numbers, set `VOSH_P5_DB` to a file path and run once to write the week there. A later run reuses the file without reading its lines first. After `sudo purge`, a whole run times only the session list and the first search cold, since that search reads every line into the cache. To time one step cold, run `sudo purge` and then run with `VOSH_P5_ONLY` set to the step's name as the run prints it, such as `VOSH_P5_ONLY="rare name"`, once for each step.
- Both also check what they time. P2 fails when the log or the grid loses, doubles or changes a round, when the log loses the rows before the first pulse or after the last, and when a prompt reaches the grid undrawn. P5 fails when a page or the session list differs from a plain scan of the week. A failure there is a broken contract, not a slow run.
- The first release build of the app tests takes about four minutes. Both run in a dev build too, far slower. Numbers from a busy machine swing widely, so record them with nothing else building.

The app check. A dev build launched with a scratch HOME that holds a copy of your profile folder, never your real one. Look at the main window, every Settings page and Help, in one dark and one light theme, with the macOS native surface on and off. Compare against the R1 screenshots.

The golden rule. A golden file changes only in a commit tied to a numbered bug or a lettered decision, and that commit shows you the byte diff. The old inputs that must still load never change. Fixture switches such as `VOSH_WRITE_WIRE` stay unset during refactor commits. If a pure move needs a digest or golden written again, something broke and the phase stops.

Commit rules. Conventional Commits, one concern each. Pure moves land apart from edits, so `git diff -M` shows them as renames and blame survives. No apostrophes in heredoc commit bodies. Nothing is pushed or tagged until you ask.

Closing a phase. Its last commit updates the status table in docs/refactor-plan.md, the matching sections of docs/architecture.md once that file exists, and the one line in CLAUDE.md Phase Status that names the current phase. Your approval of the phase covers that CLAUDE.md line.

Three promises hold for every phase.

1. Behavior stays the same, except in a commit tied to a numbered bug or a lettered decision you approved. The game sees the same bytes, your screen shows the same pixels, and the app does the same thing when you type, click or switch profiles.
2. Every file Vosh already wrote keeps loading. That covers profile files, profiles.toml, global.toml, catalog.toml, loadouts.toml, the wizard journal, affect_full.toml, logs.sqlite, maps.sqlite, scrollback.txt and the browser storage keys. Through 1.0, a downgrade to 0.7.2 keeps your settings too (D14).
3. No live name changes between Rust and the page. Dead commands, events and fields leave together with their callers. New names arrive only through a lettered decision, such as D21's setters, and the contract test lists them.

### 4.2 Order and why

1. Start from the latency run's head. It touched session.rs, Terminal.tsx and the native surface, exactly the files the refactor splits, so it lands first and nobody rebases it over a split.
2. Pin behavior before touching it. R2 adds the tests that catch a broken contract.
3. Fix the known bugs next. Players get the fixes early, and every later commit is a pure delete, move or merge with nothing hidden inside.
4. Delete before moving. Dead code removed in R4 to R6 is code nobody has to move, rename or review later.
5. Rust bottom up, then the page. Crates first, then the app frame, the session, the data layer and the renderer. The page stage starts once the Rust command and event names stop moving. R19 is the one exception, and it adds its setter commands on both sides in the same commits.
6. Deferrable phases sit right after a safe stopping point, so skipping one leaves nothing half done.
7. Sessions comes after R14, which gives each connection its own state, and before the page stage, so the command and event names tabs add settle before R15 gathers them.
8. Phase 10 comes after the page command layer and the save model, so its new Settings rows are built once on the final pattern. If you defer R19, R22 builds them on today's save path.
9. Docs last among the refactor phases, when the structure they describe is real. Phase 11 packaging comes after that, so packaging work isn't redone by a later move.

### 4.3 Stage A. Clear the decks

#### R0. Freeze and set up (required)

Goal. One known starting point, the plan in the repo, and the earliest answers in hand.

Work.

1. Ask D10 first. The Berkeley Mono files have sat in the public repo since May 17 and ship inside released builds. If the license does not allow that, the first refactor commit removes them (see D10).
2. Settle the branch flow (D1). With option A, finish milestone 2 and merge `one-window` into main locally. `one-window` holds 768 commits that main lacks, and main last moved on August 28. Pushing waits for your word.
3. With D39, add this plan as `docs/refactor-plan.md` with a status table, and change CLAUDE.md Phase Status to name it and the current phase.
4. List the stale branches and worktrees for you. Once the latency run lands, 13 older `m2/*` branches still show commits, which look squashed into `one-window` already. Nothing is removed until you approve the list.
5. Pause feature work on the files each phase moves until that phase lands. Feature work may resume at a safe stopping point.

Checks. Every gate green at the start. A red gate gets fixed first, in its own commit.

Size. No code.

#### R1. Latency work, already landed (required, done)

Goal. Start the refactor from the head of the latency run, so its faster numbers become the baseline every later phase must hold.

What landed. The seven commits a55a817 to d2239d7 from `perf/latency`. Your line goes to the game before its log row. A burst of reads draws one frame and writes its log rows after it, and a busy log never holds the loop. Your echo draws in the frame the answer asks for. The hidden xterm under the macOS underlay takes no writes. Pins hold what a typed line and its answer leave behind, and which renderer draws the live terminal. That covers all three items the first draft listed as remaining. In the target, latency_tests.rs becomes `src-tauri/src/tests/latency.rs`, xtermMirror.ts becomes `src/terminal/xterm/xtermMirror.ts`, and the frame and log rows a burst owes become `session/batch.rs`.

Work. Confirm the seven commits are on `one-window`. Then record the baseline from that head. Test counts per crate and for vitest, clippy clean on all three platforms, the perf set, and screenshots for the app check.

Checks. latency_tests, terminalRenderer tests, the session digests and the fake MUD tests.

Size. No new code.

#### R2. Safety nets (required)

Goal. Pin every contract the refactor could break, before anything changes. Tests only.

Work.

1. The IPC contract test. It collects every invoke name and listen name in `src` and checks them against the registered commands and the event constants. A Rust test that reads presets.ts already shows the pattern.
2. Config golden files in `fixtures/config`. A default and a full ProfileConfig, GlobalConfig, LoadoutSet, GlobalCatalog and profiles.toml serialize to the same bytes. Old inputs that must still load, such as bare tracked affects, `character =`, `one_with_erelei`, a `[connection]` table, a file with no `[prompt]`, a dock layout with no panes, and a catalog without `enabled_presets`.
3. The upgrade run order. A test pins today's order at launch. The mudclient folder copy, the wizard finish, the prompt upgrade, the preset rollout, then the profiles load.
4. Shared case files for the two twins that have none, the band geometry and the row split, read by both test suites. A test for each hand kept pair in 2.10.
5. A test that the tokens.css defaults equal the colors derived for the default theme.
6. Snapshots of the help topic ids and the Settings anchors.
7. The P2 and P5 benchmarks, both skipped by default. P5 reuses the Phase 10 check's generator, which writes a heavy week through the real LogStore.
8. Real telnet and ANSI captures in `fixtures/telnet` and `fixtures/ansi`, with tests that read them, once D37's files exist. If they come later, they land in R7.
9. Scratch tools for finding dead code, a knip config and the CSS usage script. They join CI in R23 with D32.

Checks. The new tests pass against today's code.

Size. About 1,100 test lines.

#### R3. The 13 bug fixes (required)

Goal. Fix the bugs listed near the top before code moves, so every later commit is a pure delete, move or merge.

Work. One commit per bug, each with a test that fails before the fix, each after your yes.

- Bug 1 waits for D4. With option A, its fix adds `run_body` to vosh-script, which hands captures to Lua as values instead of building Lua source with escaped strings. That path serves trigger Script actions too, so Lua error lines in those scripts change (today each one is one higher than the line you wrote), and the commit pins with tests what a `return` in a body does. Doing this here means capture handling is written and checked once.
- Bug 3's fix sends `vosh://macro-groups-changed` from `#group` and loadout switches. R6 keeps that event.
- Bug 11 waits for D16.
- Bug 12 touches the native surface, so CI on all three platforms runs for it.
- Bug 13 lands only if D9 keeps the Windows surface. With option B its code leaves in R13.

Checks. All gates plus the new tests. Each fix earns a CHANGES.md line at the next release.

Size. About 700 changed lines.

#### R4. Dead page code and old files (required)

Goal. Remove everything in 3.1 to 3.4 that lives on the page, plus the stale root files.

Work. Show the diff, then delete, one cluster per commit.

1. The old Settings tree. The three groups, LegacyEditors, the nine old tabs and forms, the two orphan appearance rows, unsaved.ts, usePersistedSet.ts, Icons.tsx and UnsavedDot.tsx.
2. The Ember title chrome. TopBar, TopBarLoadouts and Connect.
3. VitalsSettings with vitalsTemplate.ts, vitalsColor.ts and vitalsLayouts.ts, after the two thresholds move into vitalsStore.ts.
4. LogsTab with ansi.ts.
5. panels.ts with layoutFromDock and its tests.
6. ServerMapView's non embedded mode, then terrainDecor.ts, then Resizable's unused variants.
7. The small exports and branches in 3.3, with D20 and D27.
8. The stale root files in 3.10, with D31, and the mobile entry attribute and icons, with D8.

Also update `src/components/settings/README.md`, which still describes the dead groups.

Checks. All gates, the build, and the app check.

Size. About 9,400 lines removed, plus about 4,000 in mockups.

#### R5. Dead styles (required)

Goal. Remove the styles in 3.9.

Work. The styles.css families in four commits (title chrome, small blocks and panel zones, the vitals bar, the old Settings window and overlays). The `.input-row` font rule stays. Then the two settings.css leftovers, the overridden values and the two unused font families. The five token aliases go last, since they become unread only after the styles.css cuts.

Checks. The app check against the R1 screenshots in every window, dark and light, underlay on and off. The CSS text tests don't read styles.css, so the screenshots are the real guard. Type in the command line and check it still uses your terminal font.

Size. About 5,900 lines removed.

#### R6. Dead backend commands and app code (required)

Goal. Remove 3.5 and the parts of 3.6 that don't wait for R13.

Work.

1. Each dead command with its registration, its page wrapper and any event only it sent, one commit per family. The ones that need a decision wait for it. `vosh://macro-groups-changed` stays, and `pane_layout_get` stays with only its `profile` argument removed.
2. The never run app code in 3.6, with the `VOSH_WRITE_PLAYS` exporter, which D20 removes. The macOS surface code and the uniform wait for R13, and the mudclient copy for R8.
3. The 20 stand in functions, only if D9 is option A or C. With option B they stay, and R13 narrows build.rs to macOS when the Windows and Linux surface code leaves.
4. D12 with D14. The three old `[ui]` fields leave the Settings payload and the page in the same commit, after the payload gets serde defaults, so an older page or a missing field never fails to save. Rust keeps them on disk as stored values it writes back without reading. The three empty Loadout tables stop being written, in their own commit with the golden diff.
5. The unused shell plugin.

Checks. All gates, the contract test, the fake MUD tests, and clippy on Windows and Linux in CI.

Size. About 1,500 lines removed.

### 4.4 Stage B. Rust structure

#### R7. Small crates, tidied in place (required)

Goal. Make each small crate say only what it does, before any crate moves.

Work.

1. The deletions in 3.7.
2. The test only items in 3.11. vosh-log gets a `testkit` feature and a dev dependency on itself with that feature, as vosh-prompt has, and the app's dev dependency turns it on.
3. vosh-ansi shrinks to `plain_text`. Its `vte` line keeps `default-features = false`, because the renderer's terminal library breaks without it.
4. No real captures, since D37 keeps the synthetic fixtures.
5. Merge copies inside each crate. One subnegotiation framer for TTYPE, CHARSET, NEW-ENVIRON, NAWS and GMCP. One callback cleanup in the Lua engine. Shared row mappers in the log crate. A `Trigger::new` constructor that replaces about 65 eight field literals in tests.
6. Start `docs/architecture.md` with the crate map and the twins table from 2.10. Each later phase updates its section.

Checks. The negotiator tests pin exact bytes and must pass unchanged. All crate tests, the app tests and clippy pedantic.

Size. About 1,000 lines removed and 300 added.

#### R8. Crates merged and the log crate split (required)

Goal. Four small crates by layer instead of nine single use crates. With D2 option B, this phase shrinks to steps 4 and 5.

Work.

1. `git mv` telnet, gmcp and ansi into `crates/protocol`. `git mv` alias, vars and trigger into `crates/automation`.
2. Add the shared pieces there. One group switch used by aliases, triggers and later macros, one revision counter, one command splitter, one script call record. vosh-script then uses the automation crate's variable scope, and the bridge function in the app goes.
3. Update the app's imports mechanically. No new external crates.
4. Split the log crate by pure moves. The password wipe moves in its own commit with all of its tests. vosh-log then owns the format of the rows that record what you sent, so the hidden text exists once instead of twice.
5. With D3 option A, delete `crates/map`, map_state.rs, the map writer and its plumbing through the session. The step that creates the scripts folder becomes its own launch step, since today it hides inside the map store opener. maps.sqlite stays on disk untouched. The mudclient folder copy leaves in the same commit or earlier (D15). Today the map store creates maps.sqlite before the copy runs, and that file makes the copy skip. Without it, a fresh install on a machine that still has a `com.aabahran.mudclient` folder would copy the old data in. If you keep the copy, it gets its own guard here.

Checks. All tests, the fake MUD tests, the upgrade order test, and the TTYPE string still carries the workspace version.

Size. About 6,000 lines moved and up to 1,000 removed.

#### R9. Prompt crate (required)

Goal. The prompt engine laid out as in 2.3, with one source for each table.

Work.

1. Remove what is dead everywhere (3.8).
2. Fence the test only items (3.11). `crates/prompt/tests` holds about 9,200 lines in 15 files that import by module path, so what they use stays reachable behind `testkit`.
3. Shared test designs and clocks in the testkit replace six copies of your design string, five of your PROMPT codes and six of DETAILED, including one in the app's session tests.
4. One source per table. One list joiner, one moon table, one position table, one Layer enum, one way to convert color and style choices. PieceKind derives Serialize, which deletes its mirror type.
5. Move each test module into its own file, then split stage.rs, engine.rs, vars.rs, template.rs, edit.rs and render.rs into the folders in 2.3. Old paths keep a `pub use` for one commit so the app compiles untouched, then the app's imports update in a commit of their own. The app uses `vosh_prompt::config::` 56 times and `stage::` 49 times, and those paths keep working.
6. Break the four import loops between modules.
7. Narrow visibility as 2.3 describes.
8. The renames in D25, and the payload trims in D24 together with the page types that mirror them.

Two pairs look alike but behave differently, and they stay separate unless you decide otherwise (D26).

Checks. `cargo test -p vosh-prompt`, the session digests, the wire fixtures unchanged with `VOSH_WRITE_WIRE` unset, the retired default designs still 884 and 728 bytes, and the prompt tests on the page.

Size. About 21,000 lines moved and about 1,200 removed.

#### R10. App frame and the command layer (required)

Goal. Break up commands.rs so every command is a thin wrapper in `ipc/`, and nothing below `ipc/` calls a command.

Work.

1. `app/state.rs` and `app/events.rs`, with every event name as a constant. Inline event strings become the constants, and payloads and send order stay the same.
2. Make `disk/save.rs` (the save engine), `profile/switch.rs` (switching profiles and the auto switch at login) and `loadouts/wizard/` (the wizard's apply step and crash journal) by pure moves out of commands.rs. R12 fills these folders with the rest.
3. `ipc/` one topic per commit, each with its tests carried along.
4. Split prompt_commands.rs. Its logic, about 600 lines before its tests, moves to the app's `prompt/` module, and `ipc/prompt.rs` keeps only thin wrappers.
5. One `ipc::handler()` replaces both command lists in lib.rs.
6. lib.rs shrinks to the module list and `run()`. Setup goes to `app/launch.rs` and the exit sequence to `app/exit.rs`.
7. Merge the copies. One prompt repaint helper. One save then broadcast helper that takes each command's save policy explicitly, so every command keeps exactly the policy it has today. One "profiles not loaded" lookup. Serde names for the import formats instead of two string tables. One place for Settings value cleanup.
8. The session calls `profile::switch` instead of a command, which breaks the circle between the two.
9. docs/architecture.md gains the lock order (the save lock, then the profile set, then the profile) and the two traces. The top of `disk/save.rs` states the lock order too.

Checks. The contract test, broadcast_tests, the wizard round trip, and the commands tests moved with no edits so `git diff -M` shows renames.

Size. About 11,000 lines moved.

#### R11. Session and input (required)

Goal. Turn session.rs and input.rs into folders whose files each do one step.

Work.

1. Merge the session test helpers into one harness and move the tests under `session/tests` with their names unchanged. latency_tests.rs moves to `tests/latency.rs`.
2. Pull the output path into `output.rs`.
3. Turn session.rs into `session/` by cut and paste, keeping every signature. The burst bookkeeping from the latency run (ReadBatch, the frame budget, the owed log rows) becomes `session/batch.rs`.
4. Add `Conn` and `LogSink`. The two copies of the socket read path become one, and the seven `too_many_arguments` allows go.
5. Move the World.Time tick reading into tick.rs and the Room.Chars parsing into `input/target.rs`. One tick settings shape replaces three, with the same keys on disk and in the payload.
6. Split input.rs into `input/` and add InputResult constructors in place of 25 hand built literals.
7. `spawn` takes the known host flag from its caller, which deletes the Forsaken Lands test port seam.
8. Rename for clarity. `timers` becomes `lua_timers`, `TICK_EMIT_INTERVAL` becomes `POLL_INTERVAL`, and the spec numbers in comments become plain reasons. That covers the prompt build spec's D numbers, decision numbers and section numbers wherever the app still cites them, such as D20 in profile_config.rs and D22 in term_grid.rs, since they read as this plan's, in one docs commit. R10 already took them out of the files it made. The app takes up D25. `prompt_supplies` becomes `client_values`, and the `vosh` locals that hold what it returns, in the session and in the prompt logic R10 moved into `prompt.rs`, become `client`, as they did in the prompt crate in R9.
9. Place the files Part 2.4 gives a home that no phase named. log_state.rs and forget_passwords.rs move to `logs/`, script_state.rs becomes `script.rs`, and room_block.rs and highlight_ground.rs, which came after this plan, move into `session/` beside the steps that read them.

After R10. The crate wide `tests/` folder already holds every other crate wide test, so item 1 puts latency.rs beside them. `session_send_input` in `ipc/session.rs` still runs the line pipeline R10 moved there untouched, with `echo_lines` and `deliver_script_result` beside it. Item 2 takes `echo_lines` to `output.rs`, and item 6 takes the pipeline and `deliver_script_result` to `input/`, which leaves the command a thin wrapper. The Settings tick payload, with `tick_config_payload` and `apply_tick_config`, already lives in tick.rs, so item 5 merges the three shapes there. `session_connect` and `session_disconnect` still hold about 100 lines of connection bookkeeping, which clear the session variables, keep the live connection and character, reset the affects and send the session identity. Item 3 takes it to `session/`, so both commands end thin. `handle_char_known_for_auto_switch` in `profile/switch.rs` does more than switch. It skips a name it already saw, records the affect fulls and sends the session identity too, so item 3 moves it to `session/gmcp.rs`, and switch.rs keeps `auto_switch_for_character` for it to call.

Checks. The digests and the three stored fixtures pass without being written again. The fake MUD tests, latency tests and the perf set.

Size. About 11,000 lines moved.

#### R12. Profiles, loadouts and files on disk (required)

Goal. Everything about profiles in `profile/`, loadout mode in `loadouts/`, and safe file writing in `disk/`, with one rule for each job. This fills the folders R10 started.

Work.

1. Split profile_config.rs into `profile/file.rs`, `profile/ui.rs`, `profile/panes.rs`, `profile/shared.rs`, `disk/atomic.rs` and `disk/custom_themes.rs`. Split profile_set.rs (about 1,000 lines of code) into `profile/set.rs`, `profile/login_match.rs` and `profile/worlds.rs`, with its sharing scope joining `profile/shared.rs`. profile.rs becomes `profile/live.rs`, and the inactive profile reading in characters.rs becomes `profile/inactive.rs`.
2. Split loadout.rs and loadout_store.rs into `loadouts/`. Split migration.rs (about 900 lines of code) into `loadouts/wizard/plan.rs` and `groups.rs`.
3. One AppDataDir in `disk/paths.rs` and one `ProfileSet::read_all` replace six separate loops over every profile file. Launch reads profiles.toml once instead of three times.
4. One ordered list of upgrades in `disk/upgrades/`, with every id byte identical and the run order the R2 test pins.
5. One table of shared categories drives the seven places that spell them out by hand today.
6. One catalog overlay for launch and switch, now that bug 11's fix made them agree.
7. The importers move into `import/`, with TinTin++ sharing the common report.
8. The loadout mode renames (D17).
9. The process switches move into AppState (D18), and the eight `_with` copies go.
10. affects_snapshot.rs and affect_full.rs move to `affects/`.

After R10. R10 made `disk/save.rs`, `profile/switch.rs` and `loadouts/wizard/` with `apply.rs`, `journal.rs` and the wizard tests. profile.rs declares the switch until item 1 makes it `profile/live.rs`, and loadouts.rs declares the wizard. Some bodies stayed beside their commands for this phase to place. `create_profile`, `rename_profile`, `duplicate_profile`, `flush_before_copy`, `change_scope_locked`, `scope_refusal_for_unread` and the refusals they give sit in `ipc/profiles.rs`, and `profile/` takes them so those commands end thin. `reset_inactive_panes`, `reset_live_panes` and the detail and export bodies stay in characters.rs, which item 1 makes `profile/inactive.rs`. Every Settings value rule is now a coerce or normalize function in profile*config.rs, so item 1 carries one family to `profile/ui.rs`. Serde names the import formats, and a test pins the names and errors the page reads, so item 7 moves import.rs as it is. `set_active_loadouts` in `ipc/loadouts.rs`, which takes the loadout and profile locks, goes to `loadouts/loadouts.rs`, and `install_preset_triggers` in `ipc/automation.rs` to `loadouts/presets.rs`. The closure in `migration_apply` that sets the relaunch flag and the loadout mode flag joins the apply step in `loadouts/wizard/apply.rs` once item 9 moves both flags into AppState. Most tests in `ipc/ui_config.rs` test the profile file, not the commands. Item 1 takes the ones that read or write a profile through ProfileConfig, the `a_profile_without*_`and`_\_stays_with_each_character`tests and the`through_toml`half of each round trip, with UiConfig to`profile/ui.rs`, and leaves only the payload, generation and theme pick tests beside the commands. For D18, the relaunch and save suppression flags and the panes and UI config generations sit beside AppState in `app/state.rs`. The loadout mode flag stays in input.rs, the dirty counter in `disk/save.rs` and the unread file list in profile_config.rs.

After R11. AppState still calls the Lua timers `script_timers`, which the session now calls `lua_timers`, so item 9 renames the field as it moves the switches in. input.rs holds `APP_DATA_DIR` beside the loadout mode flag, and item 3 makes it the one AppDataDir. The profile file's tick table is `TickConfig` in tick.rs, so item 1 leaves it there. `connect` in session.rs and the login in `session/gmcp.rs` call profile_set.rs, characters.rs and affect_full.rs, and their paths follow items 1 and 10.

D13 changes nothing here. Vosh keeps writing `dock_layout`, so no golden changes in this phase.

Checks. The config goldens byte identical, the old inputs still load, the upgrade order test, the wizard round trip, the sharing scope tests, the unread file guard tests, and a `#profile reset` and `#profile load` run in the app with a scratch HOME.

Size. About 12,000 lines moved.

#### R13. Native renderer and windows (required)

Goal. The native renderer under one `native/` folder, split by job, and free of reaching into its callers' globals.

Work.

1. `native/grid/`, `native/gpu/` with the shaders in their own `.wgsl` files, `native/surface/`, and `color.rs`.
2. A pure `build_frame` that takes hover, find, the copy notice and the cell size as inputs. Frame tests that need no GPU, because the current GPU tests skip silently on a machine without one.
3. About 28 statics in the surface become a few structs. The underlay switch becomes a platform check, so each platform owns its own placement.
4. Remove the macOS on top code in 3.6 and the unused uniform.
5. `app/windows.rs`, `app/menu/` and `app/system_fonts.rs`. `install_probe` becomes `install`.
6. D9. With option B, remove the Windows and Linux surface code, narrow build.rs to macOS, and keep the platform seam and the stand in functions so option C can come later. That removes about 800 lines and three dependencies.
7. D10. Read the bundled JetBrains Mono from the app bundle at launch instead of baking about 4.9 MB into the binary.

After R10. R10 took the windows part of item 5 early. `app/windows.rs` holds the Settings and Help windows, the backdrop from window_backdrop.rs, the main window's close and blur, and macOS spellcheck, and the menu opens Settings and Help through it instead of a command. Item 5 still makes `app/menu/` from app_menu.rs and `app/system_fonts.rs` from fonts.rs, and renames `install_probe`. The two menu commands already sit in `ipc/windows.rs`, and `fonts_list` in `ipc/ui_config.rs`. The 19 native surface commands sit in `ipc/native_surface.rs` with a copy of `parse_hex`, and item 1 folds it and the one in `app/windows.rs` into `color.rs`. The native surface event names in `app/events.rs` sit behind `cfg(native_surface)`, so narrowing build.rs for D9 leaves none of them dead.

After R11. Two session paths reach the grid behind `cfg(native_surface)`. `output.rs` feeds it through `term_grid::feed_session_output` and asks for a frame through `native_surface::request_redraw`, and `session/conn.rs` asks `term_grid::reader_busy` whether you are reading back, with a stand in that says no in a build without the surface. Item 1 points them at `native/`, and D9 keeps the stand in. The native surface resize tests and `tests/latency.rs` start their session through `session::spawn` with the shared state.

Checks. Frame tests, pointer tests, the session tests that drive the grid, the latency tests, the app check on macOS (cursor shapes over the divider and links, wheel, selection, right click menu, find, copy, fullscreen corners), and Windows and Linux compile and clippy in CI.

Size. About 10,000 lines moved and about 1,200 removed with D9 option B.

Safe stopping point.

#### R14. Connection state out of the profile (required)

Goal. Move what belongs to one connection out of the profile and into a connection object the session task owns. Today every output decision takes the profile lock, and a profile switch has to carry connection state across. This is the largest change in meaning in the plan. It is required, because it is the groundwork for tabs, one per connection, in R14b (D19).

Work.

1. Add `session::Connection`, owned by the session task. It holds the prompt stage, what the session feeds the prompt engine (the GMCP packets and the prompt values Lua sets), your target and its room index, the Room.Chars list, and the tick's session fields.
2. Commands and slash commands that read these reach the connection through the session's handle instead of the profile lock. docs/architecture.md gives the connection its place in the lock order.
3. Keep today's rules exactly. A profile switch keeps the GMCP packets and drops the prompt values. A disconnect clears the target, the room list and both prompt feeds.
4. The profile switch stops carrying connection state across, and `profile/live.rs` keeps only what lasts with the profile file.
5. Output steps that only need connection state stop taking the profile lock.

After R11. `Conn` in `session/conn.rs` already holds what the loop owns for one connection, the socket, the negotiator, the telnet parser, the line accumulator, the server echo, the perf counters and the burst's frame and log rows, and `LogSink` in `session/log_sink.rs` holds its log row and scrollback ring. The prompt stage, the target, the Room.Chars list and the tick's session fields still sit on the profile, and the end of `io_loop` clears them. The Settings timer deadlines are a local of the loop, and its poll arm runs the tick and the Settings timers. A typed line and the lines the session runs compare your target and how your prompt looks in one place, `Shown` in `session/effects.rs`, and every target payload comes from `TargetPayload::of`, so item 1 changes each once.

One naming choice waits for you before item 1. The session folder holds conn.rs, with the loop and `Conn`, beside connection.rs, which opens the plain or TLS socket and defines `Stream`, so a new `Connection` type would make three names a newcomer cannot tell apart. Either connection.rs becomes socket.rs, or item 1 grows `Conn` in place of a new type.

Checks. The digests, the fake MUD tests, the latency tests, the perf set, and a profile switch while connected in the app, where your target, prompt and tick carry on as today.

Size. About 1,500 changed lines.

### 4.5 Sessions

#### R14b. Sessions (required)

Goal. Tabs for more than one connection, one session each, in the style of otty. The phase takes the number R14b so every later phase keeps the number this plan and its commits use.

Work.

1. Boards first. Mockups that follow otty's vertical tab sidebar, measured from otty itself, for you to approve before any code. The tab sidebar shows only while two or more sessions are open, so a single connection looks as it does today.
2. Build what the boards show on the connection state R14 moves out of the profile.

Checks. All gates, the digests, the fake MUD tests, the latency tests and the perf set, plus the app check with one session open and with two.

Size. Feature work, sized once you approve the boards.

### 4.6 Stage C. The page

#### R15. Page command and event layer (required)

Goal. Every call into Rust and every event on the page in `src/ipc`.

Work.

1. Split session.ts into `ipc/<topic>.ts` by moving code, with session.ts left as a forwarding file so no import breaks. Then update the about 90 importers and delete the forwarding file, all inside this phase.
2. One `useTauriEvent` hook replaces about 70 copies of the subscribe and cancel pattern.
3. The raw native surface calls in App, Terminal, Input and TerminalMenu move into `ipc/nativeSurface.ts`.
4. Event names become constants that the contract test checks against Rust.
5. The theme echo logic moves into `theme/`, which ends the import loop between session.ts and theme.ts.

Checks. All gates, the contract test and the app check.

Size. About 3,400 lines moved and about 800 removed.

#### R16. Page folder moves (required)

Goal. The folders in 2.5.

Work. `git mv` one folder per commit, with every import path that points at a moved file edited in the same commit, including `?raw` imports in tests. Settings anchors and data attributes stay exactly as they are. The helper that imports a component moves so the arrow points the right way.

Rust reads some page files by path. Each one changes in the same commit as the file it reads, and that commit builds on macOS, since app_menu.rs only compiles there.

- app_menu.rs includes `src/lib/appShortcuts.json`. This is production code.
- cell_render.rs includes the four fonts in `src/assets/fonts`, unless R13 already reads them from the bundle.
- Tests in loadout_store.rs, preset_rollout.rs and commands.rs include `src/lib/presets.ts`.
- A template.rs test reads HELP.md at the repo root, so HELP.md stays where it is.

Checks. All gates and a macOS build. Nothing else changes, so the app check is short.

Size. About 60,000 lines moved, with only import paths edited.

#### R17. Big components split, helpers merged (required)

Goal. No page component carries more than one job.

Work.

1. App.tsx into five hooks around a short MainWindow.
2. Terminal.tsx into hooks that keep today's effect order exactly. The region writer registers before the lift tracker, WebGL starts after the first fit, scrollback restores before live output, and the xterm mirror from the latency run keeps its place.
3. Input.tsx into hooks.
4. PromptCard.tsx into hooks for where the card sits, editing with undo, and the capture steps, around a short card.
5. InputPrompt.tsx into its prompt section files.
6. ServerMapView into `panel/map/`, after adding tests for its packet parsing and painters, since it has none today.
7. AffectsPane into a style picker plus a TimersView.
8. One hex color API, one cell width helper, one list joiner and one possessive (D28), one error sentence helper, one JSON list parser and one known world helper.

Checks. The app check on macOS with the underlay on and off, the split scrollback reveal, the prompt card, and Windows and Linux in CI.

Size. About 5,000 lines moved and about 700 removed.

#### R18. Styles reorganized (required)

Goal. The files in 2.6.

Work.

1. Point every reader of `--c-*` names at tokens, in CSS and in the four TypeScript files that read them. Then delete the alias block. `--c-accent-soft` and `--c-split-divider` get token names, with defaults in tokens.css so the first paint after an update still has them.
2. Move the surviving styles.css rules into terminal.css, input.css (with the `.input-row` font rule), map.css, migration.css, fonts.css and base.css, keeping each rule's place in the cascade.
3. Fold `.app` into `.shell` and the three layers of input row rules into one.
4. One entry file. Update the test CSS include list and the raw CSS imports in the tests in the same commit.

Checks. The app check against the screenshots, with special care for the macOS underlay, which depends on a list of wrapper class names.

Size. About 2,500 lines moved.

Safe stopping point.

#### R19. Settings saves one field at a time (deferrable)

Goal. Replace the full snapshot save with one setter per field (D21).

Work. Today every Settings change sends the whole configuration. That needs diffing against the last send, generation checks, three prime functions, two echo windows and four echo listeners to stop windows from undoing each other. Setters for one field remove all of it. Some already exist (theme, affects display, chat colors). Each new setter lands in Rust and on the page in the same commit, and the contract test lists it.

Checks. A test per setting that two windows changing different settings keep both changes. The app check with Settings and the main window open together.

Size. About 900 lines removed and some Rust added.

#### R20. One store pattern (deferrable)

Goal. One way to build a store on the page.

Work. A `createConfigStore` factory replaces seven near identical config stores. A `createGmcpStore` factory serves the GMCP stores. The chat, group and staff stores and the toasts move onto the shared base. Every exported hook keeps its name. It comes after R19 so the factory is built on the final save path.

Checks. All gates, the store tests unchanged, the shared GMCP view tests, and the app check of the panes that read GMCP (affects, group, chat, map, vitals).

Size. About 600 lines removed.

#### R21. Design merges (deferrable)

Goal. One recipe for each kind of control (D22, D23). Each merge changes pixels a little, so each one starts with a board for your approval.

Work. One menu recipe for the five menus, one keycap, one button recipe, one highlight color, one visually hidden class, one dot, one paged scroll, a z-index scale, one window edge, one caret geometry, and the migration wizard on the shared kit, which retires the last legacy styles and the Inter font.

Checks. Your approval of each board, then the app check of every window that uses the merged control, dark and light.

Size. About 800 lines removed.

### 4.7 Phase 10, docs and guards

#### R22. Phase 10. Logs, scrollback and search (required)

Goal. Close Phase 10 as prompt.md writes it, or as D29 and D40 change it.

Where it stands. Vosh already logs every line you see with its colors to logs.sqlite, and the lines you send as `> cmd`. Search has its page in General, Session logs, with regex, a case toggle, paging and copy. Scrollback survives restarts through scrollback.txt. The demo, searching a week of logs in under a second, passes today in a release build, at about 0.15 s warm and 0.6 s cold for a heavy week of 700,000 lines. It passes only because the whole log is about one week long. Each search reads every row and counts every match, so time grows with the log, to about 1.2 s warm and 4.7 s cold at eight weeks. A cold search in the dev build already misses the bar at one week.

Work.

1. Search engine (D35). Read oldest first, or in ascending chunks, which the OS reads ahead well. Filter local sessions by session id instead of looking up each line's session. Count matches only within the scope. Run each search on the blocking pool, not the shared async workers, and cancel a search the next keystroke replaces.
2. Search scope in the log view (D35). Last 7 days by default, plus This session, Last 30 days and All time, scoped to the current profile's host and port.
3. Save as file (D29). A session or a date range as plain `.txt` or ANSI `.log`, built on the export the backend already has.
4. Retention and switches (D34). A "Keep logs for" row in General, Session logs, a per profile "Log sessions" switch, and logging off by default for 127.0.0.1 and localhost.
5. Scrollback after a quit or crash. When you quit while connected, the exit step writes scrollback.txt and ends the open log session. While connected, scrollback also saves every few minutes off the session loop, so a crash loses little.
6. Scrollback size and times (D40). With option A, a scrollback size in lines in General that both renderers and scrollback.txt follow.
7. Dev build speed (D36). Optimize `libsqlite3-sys`, `rusqlite`, `regex` and `regex-automata` in `[profile.dev.package]`.
8. The Phase 10 demo as a test. P5 over a heavy week stays under one second, and over eight weeks a Last 7 days search stays under one second too.

Checks. All gates, P5 before and after, the latency tests (searches must not touch the session loop), the forget passwords tests, and the app check on the Session logs page with a scratch HOME.

Size. About 1,200 lines, most of them tests.

#### R23. Docs, help and guards (required)

Goal. Docs that match the code, and guards that keep dead code from growing back.

Work.

1. Help from one source (D11). The help tests shrink to tests of parsing and behavior.
2. README, CONTRIBUTING, CLAUDE.md (D30), the docs in 2.7, and fixtures/README.md. The CLAUDE.md layout section reflects the real tree, and docs/requirements.md records what D3, D29 and D40 changed.
3. The remaining spec numbers and era names in comments become plain reasons.
4. CI as D1 and D32 decide. A check that package.json, Cargo.toml and tauri.conf.json agree on the version. One page build shared by the Rust jobs instead of four. The release workflow runs the same gates before it builds, installs libssl-dev like CI, and its comments stop saying builds are unsigned while it passes signing secrets. The pre commit hook passes only script files to eslint.
5. Mark the refactor done in docs/refactor-plan.md, and move the plan to `docs/history/`.

Checks. All gates, the help tests, the new CI steps green on all three runners, and a fresh clone builds by following README alone.

Size. Docs and CI only.

Safe stopping point. The refactor is done.

### 4.8 Phase 11. Packaging

These come after the structure settles, so packaging work isn't redone by a later move.

#### R24. Release groundwork (required)

Work. D10 settled, with any license text beside the fonts. The bundle's long description stops promising a "connected map window". The updater stays opt in, and a test checks it. No version bump or tag without your ask.

Checks. All gates, the updater opt in test, and a draft release build on each platform in CI.

Size. About 100 lines.

#### R25. Accessibility and a high contrast theme (required)

Work. Accessibility hints across the three windows and a high contrast theme, as Phase 11 asks. Each starts with a board. Under the macOS underlay the terminal text lives only in the native grid, and the hidden xterm is hidden from screen readers by the same rule that hides it from view. This phase decides how a screen reader reads the terminal there, for example an accessibility mode that turns the underlay off so xterm's screen reader support takes over.

Checks. A VoiceOver pass over each window, a contrast check of every text color in the new theme, and the app check.

Size. Feature work, sized with its boards.

#### R26. Signed packages on every platform (required)

Work. macOS signing and notarization, for which the release workflow already passes the secrets. Windows signing with the certificate from D38. The dmg, msi, nsis, deb, rpm and AppImage targets are already set. The release workflow builds a draft, and you publish.

Checks. The Phase 11 demo. Each package installs and launches on its platform, Gatekeeper accepts the macOS build, and Windows reports a valid signature.

Size. Release workflow changes only.

## Part 5. Decisions for you

In the order of the answer sheet. Each says which phase waits on it.

### Needed at R0

D10. Bundled fonts (R0, R13, R24). Blocks 1.0.

- Berkeley Mono is a commercial typeface, and it is the default terminal font. The two files have sat in the public repo since May 17, and released builds carry them. The repo holds no license that allows that.
- A. You hold a license that allows shipping it in a public GPL app. Its text goes beside the fonts.
- B. Remove it at R0. JetBrains Mono becomes the default, and profiles that name Berkeley Mono fall back to it. Git history still holds the files, and only a history rewrite with a force push removes them, which is your call.
- Separately, the renderer bakes about 9.5 MB of fonts into the binary while the same files ship for the page. Reading them from the app bundle at launch removes the copy (R13).
- Recommendation. Answer this first. B unless A is certain. Then read the fonts from the bundle in R13.
- Answered October 1, 2026. Option B. With Berkeley Mono gone, the renderer bakes the two JetBrains Mono files, about 4.9 MB, and R13 moves them out of the binary.

D1. Branch flow for the refactor (R0). Blocks 1.0.

- A. Finish milestone 2, merge `one-window` into main, then land each phase as a pull request into main so CI runs on all three platforms.
- B. Keep working on `one-window` and turn on CI for pushes to every branch.
- C. Keep today's flow with local gates only.
- Recommendation. A, and also turn on CI for branch pushes. CI costs nothing for a public repo, and you still decide every push. Today CI runs only for pushes to main and pull requests into it, so without this, Windows and Linux code is never compiled during the refactor.
- Answered October 1, 2026. Option C. Each phase lands on `one-window` on your machine with no pushes.

D39. Where this plan lives (R0). Add it as `docs/refactor-plan.md` with a status table at the top, and change CLAUDE.md Phase Status to name it and the current R phase. The last commit of each phase updates both, and your approval of the phase covers that one CLAUDE.md line. Recommendation. Yes. Kept outside the repo, a later session can't find it from CLAUDE.md alone.

### Needed at R2

D37. Real telnet and ANSI bytes (R2, or R7).

- Every wire fixture today is synthetic and says so. The CLAUDE.md quality bar asks for captured bytes, and this audit may not read your logs.
- A. You record one short session with socat, as fixtures/README shows (connect, log in, look, one fight round, quit). I trim it to the negotiation and a few colored lines, strip names, chat and credentials, and show you each file before it lands.
- B. Build the Aabahran server from its local source and capture a throwaway character on your machine. No player text at all, but the server has to build.
- C. Keep the synthetic files and change the quality bar to say so.
- Recommendation. A. It takes a few minutes of your time and gives the parsers the bytes the real server sends.
- Answered October 1, 2026. Option C. The synthetic fixtures stay. The quality bar line in CLAUDE.md waits for your word on its new wording.

### Needed at R3

D4. Lua alias bodies (R3). Blocks 1.0.

- A. Make them run. Fix the expansion call, pass captures as values through `run_body`, settle how captures are numbered, and add tests.
- B. Retire the feature. Keep reading the `script` field so saved files round trip, and remove the editor option and the help text.
- Recommendation. A, with a CHANGES line. Settings and HELP both promise it, and aliases saved with a Lua body today swallow input.

D9. The native surface on Windows and Linux (R3, R6, R13). Blocks 1.0. It is off by default, reachable only through a hidden flag, and has never run on real hardware. Bug 13 and the 20 stand in functions depend on this answer, so it comes early.

- A. Keep it behind the flag, but stop starting it at launch unless the flag is on. The stand in functions go in R6.
- B. Drop it for 1.0. Windows and Linux keep using xterm, as they do by default today. R13 removes the code and narrows build.rs to macOS, and the stand in functions stay because those platforms need them.
- C. Port the macOS underlay design to both (new work).
- Recommendation. B, keeping the platform seam so C can come later. That removes about 800 lines and three dependencies of untested code.

D16. The catalog on a profile switch (R3). Make the switch keep a profile file's own items, the way launch does (bug 11). Recommendation. Yes, with a test.

### Needed at R4

D8. Desktop only (R4). Drop the mobile entry attribute and the iOS, Android and Windows Store icons. The stand in functions follow D9, not this. Recommendation. Yes. Nothing builds or ships for mobile.

D20. Debug tools (R4).

- Remove the temporary split debug overlay, the live terminal counter, the two WebGL window flags and the prompt pointer probe.
- Keep the `/__vosh-dbg` sink, which is dev only.
- Keep the `VOSH_WRITE_PLAYS` exporter only if its screenshot harness still lives somewhere outside the repo.
- Recommendation. Yes to all three. Tell me whether the harness exists.
- Answered October 1, 2026. Remove the four tools, keep the `/__vosh-dbg` sink, and remove the exporter too. R4 removed the tools, and R6 removed the exporter.

D27. The Char.State and weather stores (R4). Recommendation. Drop them now and keep their fixtures. They come back with the pane that shows them.

D31. Old files (R4). Delete NEXT_PHASES.md, loop-prompt.txt, .docking_baseline/ and mockups/. Move scripts/combat.lua to examples/lua with its path fixed. Recommendation. Yes. Git history keeps all of it.

### Needed at R6

D3. The local map store (R6, R8). Blocks 1.0.

- A. Retire it. Delete vosh-map, map_state.rs and the three map commands, and stop writing maps.sqlite. The file stays on disk. D15's mudclient copy leaves with it.
- B. Keep it and build speedwalk, click to walk and room notes for 1.0.
- Recommendation. A. Today Vosh writes every room to maps.sqlite and nothing reads it back, while the pane draws from what the game sends. prompt.md lists speedwalk, click to walk and stored maps for 1.0, so A changes what 1.0 promises, and docs/requirements.md records it. Plan those as features after the refactor, built on what the game sends.

D14. Rolling back to 0.7.2 (R6, R12). A downgrade to 0.7.2 reads the `[ui]` prompt copy, `dock_layout`, and the three old fields in D12.

- A. Through 1.0, Vosh keeps writing all of them, and they retire together in the first release after 1.0.
- B. Accept that a downgrade resets them.
- Recommendation. A. It costs a few stored fields and keeps a way back while 1.0 settles.

D12. Old fields nothing reads (R6).

- `ui.vitals`, `moons_position` and `side_panels_fill_height` leave the page and the Settings payload, which gets serde defaults first. With D14 option A, Rust keeps them on disk as stored values it writes back without reading.
- The empty `tick`, `connection` and `profile_vars` tables in each loadout stop being written. 0.7.2 reads defaults for them, so a downgrade loses nothing.
- Recommendation. Yes. The loadout change alters the bytes Vosh writes, so its golden diff comes to you in its own commit.

D5. Plugins (R6).

- A. Drop the three plugin commands. Plugins keep loading at launch, and HELP says how to turn one on in the profile file.
- B. Add a Plugins row to Settings for 1.0.
- Recommendation. A. No screen calls these commands, and a plugin system is a Phase 12 stretch item.

D6. Group toggle commands (R6).

- A. Drop them. `#group` and loadouts already switch groups.
- B. Bring group checkboxes back to Settings.
- Recommendation. A. The macro groups event stays either way.

D7. Profile descriptions (R6). Drop the command that sets them, and keep reading and writing the field so your text survives. Recommendation. Yes.

### Needed at R8

D2. Crate layout (R8).

- A. Four crates by layer (protocol, automation, script, log) plus prompt.
- B. Keep nine crates and only tidy them. R8 then only splits the log crate and retires the map store.
- Recommendation. A. Each crate has one user today, so the walls between them protect little, and four crates are much easier to find your way around.

D15. Old one time migrations (R8, R12).

- The copy from the old `com.aabahran.mudclient` folder. No released build needs it. It must leave no later than the map store, or get its own guard (R8).
- The old mudclient browser storage keys and the old map style key.
- The old Settings tab ids the palette still writes to Recent.
- The move from a root profile.toml to `profiles/`, which only v0.0.2 and v0.0.3 need.
- Recommendation. Once you confirm your own machine has moved, retire the mudclient folder copy and its keys in the same commit as the map store. Remap the palette's Recent ids once, then drop the old tab ids. Keep the root profile.toml move, since it costs almost nothing.

### Needed at R9

D24. Prompt payload fields the page never reads (R9). Drop them, and keep each value's GMCP source as a comment beside its catalog entry. Recommendation. Yes.

D25. Prompt crate renames (R9). The `vars` module becomes `values`. The `Vosh` struct becomes `ClientValues`, since "supplies" would read as game items in a MUD. `Stage::show` becomes `show_as_sent`, since `show` elsewhere means where your prompt is placed. `format.rs` keeps its name inside `values/`. Recommendation. Yes.

D26. Two look alike rules in the prompt crate (R9). What lamented tears hides is computed two ways that differ on Char.Combat, and two max value spellings run in different orders. Recommendation. Keep both as they are and document why. Merge only if you want the behavior to change.

### Needed at R12

D13. Dock layout (R12). Keep the lazy conversion from the old dock to panes, and keep writing `dock_layout` through 1.0, as D14 asks. No recorded upgrade before 1.0. A profile file that arrives later, through `#profile load`, a hand copy or a session in 0.7, still needs the lazy conversion, so the upgrade would add code and remove none. Recommendation. Yes. Retire both after 1.0 with D14.

D17. Loadout mode names (R12). Rename the Path B identifiers in code and keep `path_b_active` on the wire through a serde rename. Recommendation. Yes.

D18. App state instead of process switches (R12). Move the loadout mode flag, the relaunch flag, the save suppression flag, the generation counters and the unread file list into AppState, and delete the eight `_with` copies that exist for tests. Recommendation. Yes. It also lets tests stop sharing counters.

D33. Loadout matching (R12). Each loadout can carry a world and character list, and the Loadouts editor shows them, but nothing sets them and nothing switches loadouts by them.

- A. Drop the World and Characters rows from the editor. Keep reading the field so hand edited files still load.
- B. Build the matching, so connecting as a character picks its loadout.
- Recommendation. A for 1.0. Profiles already match characters at login, and loadout matching can come later as a feature.

### Needed at R14 and later in the page stage

D19. Connection state out of the profile (R14).

- A. Do it before 1.0 as R14.
- B. Do it after 1.0.
- Recommendation. A, but only if R11 lands clean. Otherwise B. It simplifies profile switching and takes the profile lock off the output path, but it is the largest change in meaning in the plan.

D28. Wording helpers (R17). One possessive ("Rhys's", not "Rhys'") and the serial comma everywhere. Recommendation. Yes.

D21. Settings saves one field at a time (R19). Recommendation. Yes, after the page command layer lands. It removes a whole class of bugs where two windows undo each other. Its new setter commands are the one place the plan adds names between Rust and the page.

D22. Visual merges (R21). One menu, one keycap, one button style, one highlight color, one window edge (the main window uses 0.10 and Settings uses 0.18), one caret geometry (the input uses 7 by 15 and the picker 7.8 by 17), one scroll depth chip and a z-index scale. Recommendation. Yes, as a design pass that starts with a board for you.

D23. The migration wizard's look (R21). Rebuild it on the shared kit, which retires the last legacy styles and the Inter font. Recommendation. Yes, board first.

### Needed at R22, Phase 10

D29. Log files (R22). Blocks 1.0.

- prompt.md asks for per session log files with rotation and per session toggles. Vosh already logs every line with its colors to logs.sqlite, which search, the prompt lookup and `#logs forget-passwords` all read.
- A. No running text files. SQLite stays the one store, and you get Save as file, plain `.txt` or ANSI `.log`, for a session or a date range. An optional per profile "also write a text log" switch can come after 1.0, off by default.
- B. Per session text files with rotation, as prompt.md wrote it.
- Recommendation. A. A second write on every line would land on the session loop the latency work just trimmed, and text files would keep the password lines that forget passwords only cleans in the database. A changes prompt.md's requirement, and docs/requirements.md records it.

D40. Scrollback size and timestamps (R22). Blocks 1.0.

- prompt.md asks for scrollback sized in lines or memory, with timestamps. Today both renderers keep a fixed 10,000 lines, and only the log view shows times.
- A. A scrollback size in lines in General, and times stay in the log view.
- B. A, plus optional times in the terminal, which both renderers then draw.
- C. Neither for 1.0, and docs/requirements.md says so.
- Recommendation. A. Every logged line already carries its time, and terminal timestamps would need new work in both renderers.

D34. Log retention and switches (R22).

- No file rotation. A "Keep logs for" setting (forever by default, or 1 year, 90 days, 30 days) deletes whole sessions past the limit and gets the space back by compacting the file a little at a time. Turning it on for an existing file needs one full compaction.
- A per profile "Log sessions" switch, on by default.
- Logging off by default for 127.0.0.1 and localhost.
- At a heavy rate of play the log grows about 85 MB a week, about 4.4 GB a year.
- Recommendation. Yes to all three.

D35. Log search scope (R22).

- A. A default scope of Last 7 days, plus This session, Last 30 days and All time, scoped to the current profile's host and port. Line ids grow with time, so a week needs no new index. Then fix the engine as R22 describes. That should hold about 100 ms warm and 175 ms cold for a week in release however big the log grows, an estimate not yet measured end to end.
- B. Also add SQLite's built in FTS5 trigram index. Searches drop to milliseconds, but the file doubles and every written line costs more.
- Recommendation. A. Add B only if All time must be instant.

D36. Search speed in dev builds (R22). Optimize `libsqlite3-sys`, `rusqlite`, `regex` and `regex-automata` in `[profile.dev.package]`. It is a build setting only, and it makes dev searches about twice as fast warm and 30 percent faster cold. Recommendation. Yes.

### Needed at R23

D11. One source for help (R23).

- A. HELP.md is the source. The page reads it and finds the topics by their headings, with each topic id kept beside its heading.
- B. helpContent.ts is the source and a script writes HELP.md.
- Recommendation. A. You edit prose in one Markdown file, and about 400 lines of tests that police the copy go away.

D30. Project docs (R23). Replace the phase model in CLAUDE.md and prompt.md with docs/requirements.md plus the milestone plan. Only you approve CLAUDE.md changes. Recommendation. Yes.

D32. Dead code guards in CI (R23). Add knip as a dev dependency and the CSS usage script as an npm script and CI step. Neither sends data anywhere. Recommendation. Yes. Without them dead code piles up again, because the Rust compiler can't see through command registration and the page compiler can't see unreachable files.

### Needed at R26

D38. Windows signing (R26). Blocks 1.0.

- prompt.md asks for signed Windows builds. Signing needs a code signing certificate from an outside service, and CLAUDE.md asks for your approval before any new outside service.
- A. Choose a certificate service and add its secret to the release workflow.
- B. Ship 1.0 unsigned on Windows and say so in README.
- Recommendation. A, chosen early, since issuing a certificate can take days.

## Part 6. Risks and how the phases guard against them

| Risk                                                                                                   | How the plan guards against it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Your files on disk. A rename or removal breaks loading, or a downgrade loses data                      | R2 pins the bytes of every config file and a set of old files that must still load. The golden rule allows a golden change only in a commit tied to a bug or decision, with the diff shown. No struct refuses unknown keys, so a removed field still loads. Upgrade ids, backup suffixes, the wizard marker table and `pattern` stay byte identical. D14 keeps 0.7.2 rollback working through 1.0. Retiring the map store or the mudclient copy never deletes a file. The password wipe splits as a pure move with all 1,299 test lines. Scripted runs use a scratch HOME |
| Settings between windows and versions. Rust requires some payload fields with no default               | R6 adds defaults before any field leaves, and changes Rust and the page in one commit. R19 replaces full saves only after R15, with a two window test. Browser storage keys never change                                                                                                                                                                                                                                                                                                                                                                                  |
| Names shared by Rust and the page. A rename fails only at runtime                                      | The R2 contract test lands before any move. Functions keep their names when they move. Event names become constants in R10 and R15. New names come only with D21                                                                                                                                                                                                                                                                                                                                                                                                          |
| Both renderers. The twins, the underlay's class list and Terminal.tsx's effect order all carry meaning | The digests and stored splits are read by both sides and never written again to make a phase pass. 2.10 names every twin and its fixture. The app check runs with the underlay on and off. R17 keeps the effect order. R18 updates the underlay class list with any class rename. R13 adds GPU free frame tests                                                                                                                                                                                                                                                           |
| Three platforms. Platform code compiles only on its own runner                                         | D1 turns on CI for every push. R3, R6, R13 and R17 need green CI on Windows and Linux, and R16 needs a macOS build for app_menu.rs. Platform CSS moves intact                                                                                                                                                                                                                                                                                                                                                                                                             |
| Locks and threads. A reordered lock can deadlock the app or the exit save                              | Moves keep every lock scope exactly. R10 writes the lock order into docs/architecture.md and `disk/save.rs`. R13 documents the renderer's order (surface, then grid, then style). R14 gives the connection its place                                                                                                                                                                                                                                                                                                                                                      |
| Tests that pin bytes or read files by path                                                             | Every move that changes a folder depth fixes its `include_str!` paths in the same commit. R16 lists the Rust reads of page files. The CSS text tests change with the files they read. Fixture switches stay unset                                                                                                                                                                                                                                                                                                                                                         |
| Branches in flight                                                                                     | The latency run lands first. R0 lists the rest. Each phase names the files it moves, and feature work on them waits for a safe stopping point                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Deletions that turn out to matter                                                                      | Only items a second reader confirmed are listed, and 3.12 names what looks dead and is not. You see every deletion's diff. Product choices wait for their decision. D20 asks about the screenshot harness, the one script outside the repo                                                                                                                                                                                                                                                                                                                                |
| Behavior changes hiding in refactors                                                                   | Every behavior change is a numbered bug or a lettered decision in its own commit after your yes. Visual merges wait for a board. Digests, goldens, latency pins and screenshots catch anything that slips                                                                                                                                                                                                                                                                                                                                                                 |

## Appendix. Phase numbers in the first draft

R0 to R2, R7 to R13 and R23 to R26 keep their numbers. The rest moved.

| First draft                  | This draft |
| ---------------------------- | ---------- |
| R3 Dead page code            | R4         |
| R4 Dead styles               | R5         |
| R5 Dead backend              | R6         |
| R6 Bug fixes                 | R3         |
| R14 Phase 10                 | R22        |
| R15 Connection state         | R14        |
| R16 Page commands and events | R15        |
| R17 Stores                   | R20        |
| R18 Folder moves             | R16        |
| R19 Component splits         | R17        |
| R20 One field saves          | R19        |
| R21 Styles reorganized       | R18        |
| R22 Design merges            | R21        |
