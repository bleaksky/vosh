# Aabahran GMCP packets

Each `.gmcp` file holds one GMCP payload the way the Aabahran server writes it between `IAC SB 201` and `IAC SE`. That is the package name, a space, and the JSON. The Rust tests hand the bytes to `vosh_protocol::gmcp::parse`, and the TypeScript tests split them the same way and feed the data to the stores.

These packets are written by hand, not captured. They follow the server's `docs/gmcp-spec.md` and the emitters in `gmcp.c` as of 2026-09-29, with the lamented tears `hidden` flag, the Char.Combat `tank` field, and the Char.Prompt, Char.State and Room.Weather packages. Character names are placeholders.

Three server builds send different packets under lamented tears. The new build (54f14ef6 and later) marks each hidden packet with `"hidden":true`. The build before it (243cac5c) sends the same empty and zero packets with no flag. The older build sends the true values, and only Char.Affects names the song. The prompt engine in `crates/prompt` reads all three.

| File | What the server sends it for |
| --- | --- |
| `char-vitals.gmcp` | Your vitals at a prompt. |
| `char-vitals-hidden.gmcp` | Your vitals under lamented tears. Every field is 0 and `hidden` is true. |
| `char-vitals-zero.gmcp` | Your vitals under lamented tears on 243cac5c. Every field is 0 with no flag. |
| `char-affects.gmcp` | Your affects at login or on a change. |
| `char-affects-hidden.gmcp` | Your affects under lamented tears. The list is empty and `hidden` is true. |
| `char-affects-empty.gmcp` | Your affects under lamented tears on 243cac5c. The list is empty with no flag. |
| `char-affects-lament.gmcp` | Your affects under lamented tears on the older build. The list names the song. |
| `char-combat.gmcp` | A fight where you see your opponent's health. |
| `char-combat-hidden.gmcp` | A fight where the game withholds that health. Lamented tears, blindness, mirror image, or a target in another room all do it. |
| `char-combat-withheld.gmcp` | The same fight on 243cac5c. The target comes alone with no flag. |
| `char-combat-lament-older.gmcp` | A fight under lamented tears on the older build. The condition and health still come. |
| `char-combat-tank.gmcp` | A fight where your opponent hits someone in your group. |
| `char-combat-tank-hidden.gmcp` | The same fight under lamented tears. The tank keeps its name and loses its health. |
| `char-combat-end.gmcp` | The fight ends. |
| `char-prompt.gmcp` | Your prompt settings at login, with the stock prompt. |
| `char-prompt-off.gmcp` | Your prompt settings after `prompt off`. |
| `char-prompt-fight.gmcp` | Your prompt settings with a fight prompt and a colour code, kept raw. |
| `char-state.gmcp` | Your position and spoken language at a prompt. |
| `room-info.gmcp` | The room you look at, the Bank of Aabahran with one exit south. |
| `room-info-rhapsody.gmcp` | The made up room the game sends while rhapsody of delusion is on you, with all six exits. |
| `room-weather.gmcp` | The weather outdoors at a prompt. |
| `room-weather-indoors.gmcp` | The weather indoors, in Celsius. |
| `group-info.gmcp` | Your group at a prompt. |
| `group-info-solo.gmcp` | No group. |
| `group-info-hidden.gmcp` | Your group under lamented tears. |
| `group-info-empty.gmcp` | Your group under lamented tears on 243cac5c. It is the same `{}` a solo player gets. |
| `group-info-own-row.gmcp` | Your group under lamented tears on the older build. The roster comes whole, your own row included. |
| `snoop-start.gmcp` | You start snooping Tolliver. Only a client that names Snoop in Core.Supports gets it. |
| `snoop-output.gmcp` | What Tolliver's screen got, eastroad.are room 6900 and the prompt after it, ANSI kept. Each control byte goes out as `\u00XX`, the way `gmcp_send_snoop` writes it. |
| `snoop-stop.gmcp` | The snoop of Tolliver ends, by your stop or because Tolliver left the game. |

`lament.json` lists the three lamented tears cases, one per server build. Each names the packets in the order that build sends them and the hidden state the backend works out from them, in the `{vitals, tank, opponent, affects, group}` shape of `session://hidden`. The Rust session tests feed the packets through the session and check that it works out that state and draws `?` for your vitals. The TypeScript store tests feed the same packets with that state and check that the Vitals, Affects and Group panes read hidden.

`views.json` records what each packet means with no hidden model: the vitals, the affects one row per name, the fight with a flagged opponent's health and condition left out, the group with a flagged roster left out, position and language, the weather, the prompt settings, and the room. The prompt engine keeps its own copy of the packages to draw your prompt, and the panes read the webview stores, so `crates/prompt/tests/views.rs` and `src/test/aabahranViews.test.ts` both check their reading of each packet against this one record. No webview store reads Char.State or Room.Weather, so only the engine checks those records until a pane brings their stores back.

A room view reads Room.Info as the prompt engine reads it on the new build, with the exits as direction words in the game's door order and the region by its `climate` name. Its `map` holds what the room strip under the Map pane reads where that differs on purpose, and only the webview test reads it. The made up rhapsody room names all six exits with room 0 behind each. The Exits piece shows all six, as `%e` prints them under the song, while the strip lists only exits with a room behind them. The engine prints the game's sector of -1, while the strip colors a sector only when the map has one. The rhapsody room sends no region, so both read none.

## Chat packets

The `chat` folder holds Comm.Channel packets, written by hand from `gmcp_send_channel_ext` in `gmcp.c` and its callers in `act_comm.c` and `languages.c`. Only the chat store in the webview reads them, in `src/stores/gmcp/chatStore.test.ts`, so they sit apart from the packets the prompt engine keeps a view of. The messages are made up.

| File | What the server sends it for |
| --- | --- |
| `chat/say.gmcp` | Someone in the room says something in common. |
| `chat/tell.gmcp` | A tell you receive in common, marked `received`. |
| `chat/tell-foreign.gmcp` | A tell in a language you do not know. The text is the garble you saw, the language reads `foreign` and `understood` is false. |
| `chat/yell.gmcp` | A yell from a mob, whose name runs to several words. |
| `chat/gtell-disguised.gmcp` | A group tell from a disguised player, as an immortal sees the name. |
| `chat/cabal.gmcp` | Cabal talk. The channel carries no language. |
| `chat/immortal.gmcp` | Immortal talk. |

## Map packets

The `map` folder holds Map.Tiles packets. Nobody captured them or wrote them by hand. A line for line port of `generate_map` in `minimap.c` and `gmcp_send_map` in `gmcp.c`, as of 2026-10-02, built them from the game's own area files, read in the order `area.lst` boots them. Every room, exit, sector and area vnum is the game's, each door stands as a reset leaves it, and the keys come in the order the server writes them. The viewer is a mortal of level 30 in daylight, except in the Dark Castle look, where an immortal stands. Secret exits stay hidden from a mortal. An immortal sees them all, uppercase in `e` when they lead past the next cell, with their doors `hidden` in `d`. `l` is the daylight light level, and `f` carries only the safe and bank letters, since the shop, trainer and healer letters depend on who stands in a room at that moment. Only the map view reads them, in its tests under `src/lib`.

| File | Where you stand |
| --- | --- |
| `map/caranduin-west-of-the-fountain.gmcp` | West of the City Fountain in Caranduin, room 4406, at radius 7. The Common Road west of you leads north to the Potion Shop and south to the Trading Post, while the cells beside it hold the Pill Shop and The Meat Store, so the game sends those two letters in uppercase. The `zr` search climbs to the wall at Inside the West Gate and stops short of the two Under a Battlement Ladder rooms seven steps away, so only `a` carries Atop a Battlement Ladder over each and On the Battlements beyond each. |
| `map/caranduin-the-common-road.gmcp` | The Common Road in Caranduin, room 4405, at radius 7, one step west of the room above. You walked, so most rooms sit one cell further east than in that packet. |
| `map/val-miran-central-square.gmcp` | The Central Square of Val Miran, room 20605, at radius 10. The Forest's Edge sits ten steps north in row 0, and A trail through the light forest sits ten steps west in column 0. Every room in `a` and `b` comes again in `zr` at the same spot, Before the Temple of Neutrality right above you among them. |
| `map/mahn-tor-an-enormous-gate.gmcp` | An Enormous Gate in Mahn-Tor's Dungeon, room 18362, at radius 7. Its locked door south opens on A Long Tunnel. The next Long Tunnel leads south with no door to A Four-Way Intersection, whose closed door north leads to another intersection, so the game sends that letter as `N`. |
| `map/dark-castle-maw-immortal.gmcp` | The Maw of Malfeascances in The Dark Castle, room 27057, at radius 10, seen by an immortal. Barnok Boulevard and A Secret Hideout lead to each other through hidden doors. The hidden door south of the Mausoleum of Innocence leads past Lord Corim Street in the cell below, and the one south of A cobbled road into the forest faces an empty cell. |
