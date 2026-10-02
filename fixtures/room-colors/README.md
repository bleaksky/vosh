# Room colors fixtures

Hand written and synthetic. Nothing here was captured from a live session. Every line is game text the Aabahran server prints, built from its own format strings and area files, which live in the forsaken_lands source and the `area` folder beside it. Tolliver and Maren are invented names for players.

## looks.json

Room looks, each as a list of events in wire order. A `gmcp` event is a packet, a `line` event is one line of text with the ANSI codes the server sends, and a `prompt` event is a prompt the server ends with a GA. `room` marks the lines a Room trigger matches, the things and the people a look lists after its exits line. `src-tauri/src/session_room_tests.rs` plays each look through the session's own steps, and `crates/trigger/src/engine.rs` highlights each word of each line in place.

Where the lines come from.

- The look itself, in order, is `do_look` in act_info.c. The room's name, two spaces and its description, a blank line, the exits line, then the things and the people.
- The exits line is `do_exits` with `auto` in act_info.c. Room 5279 in area/fortblac.are, The Bank of Aabahran, has one exit, `D2`, so its line reads `[Exits: south]`.
- The room name carries the `` `8 `` code area/fortblac.are gives it, which the server sends as `ESC[0;1;30m`, and the reset code after it, two backticks, as `ESC[0;0m`. The first look also has the 256 color tint `do_look` puts before the name of an inside room.
- The things are `list_to_char` in act_info.c, five spaces or a count like `( 2) ` before an object's long text. Objects 5201 and 5202 in area/fortblac.are give `A black-steel helm is here, gleaming darkly.` and `A pair of black-steel gauntlets rests on the ground.`. The inventory lines use their short texts, `a black-steel helm` and `a pair of black-steel gauntlets`, as `do_inventory` lists them under `You are carrying:`.
- The people are `char_to_char` in act_info.c. Mobs 5287 and 5283 reset in room 5279 and print their long texts, `A Blackwatch villager scurries about, taking care of business.` and `A representative of the Bank of Aabahran is here, ready to take your gold.`. Their Room.Chars names are their short texts, `a villager` and `a bank representative`. A resting player prints as `Tolliver is resting here.`, here with the `[AFK] ` tag whose AFK the server colors with `` `1 ``.
- `Maren walks in.` is `$n walks in.` in act_move.c, an arrival that lands after a look in the same pulse.
- `The day has begun.` is `weather_update` in update.c at hour 6.
- `You spot some fresh spur.` is `show_tracks` in skills4.c, which follows the exits line when the game draws its minimap.
- `It is pitch black ... ` is the dark room line in `do_look`, which prints no exits line.
- The prompt is the one `prompt all` sets, `%n%P%C<%hhp %mm %vmv> `, with sample numbers.

## lines.json

Single lines for the Room and time colors preset. `trigger` names the trigger that colors a line and `match` the text its color covers. A line with no `trigger` is a near miss that no trigger of the preset may touch. `src/lib/presets.test.ts` runs the preset's patterns on each one, `src-tauri/src/session_room_tests.rs` runs each through the session's own steps, and `crates/trigger/src/engine.rs` highlights each word of each line in place.

- The exits lines are `do_exits` with `auto`. Room 5233, The Eastern Square, has exits `D0` to `D3`. Room 5279 has `D2`. Room 5200, Rock Bottom, has `D4` and a door at `D5` that resets closed, which the line shows in parentheses. The same room shows `(+down)` when you see a trap on that door, the `+` in `` `! `` bold red. A room with no exit you can see reads `[Exits: none]`, the same text the builder tutorial mob in area/higher.are echoes.
- The eleven time of day lines are `weather_update` in update.c, the five usual ones and the six it sends in eternal darkness.
- The WiZNET lines are `wiznet` in act_wiz.c, `` `&W`8i`&ZNET`8 ``, the time cut from ctime, and the message. The messages are `TICK!` from update.c, `Newbie alert!  $N sighted.` from comm.c and `$N has posted a note.` from recycle.c. Three more carry colors of their own, which the preset leaves on them. `` `!Corrupted Pfile detected: %s`` `` from comm.c is bold red, `` `&%s attacked %s at %d`` `` from `m_yell` in magic.c is bold white, here in room 5279, and `` `@%s has been forced wizinvis for idling > 13 ticks.`` `` from update.c is bold green.
- The says, tells and yells are the formats in languages.c and act_comm.c, `$n says`, `%s tells you` and `$n yells`, each with its color code around the text. The game has no gossip channel, so the newbie channel from `do_newbiechat` stands in for one. Each quotes a line the preset colors.
- `Welcome to Wiznet!` is `do_wiznet` in act_wiz.c.
- The prompts are the one `prompt all` sets, and `%e` alone from `do_promptexit` in act_info.c, which prints single letters, `---` while you are blind and `???` in forest mist.
- `Obvious exits:` and its row are `do_exits` typed by hand, and `You can't tell where the exits are.` is what it prints in forest mist.
- The room names are rooms 5279, 5233 and 5200 in area/fortblac.are, with their own color codes.
- `The sky is getting cloudy.` is `sky_event_text` in update.c, which can follow a time of day message in the same send.

## preset.json

The triggers of the Room and time colors preset, exactly as `presetTriggers` in `src/lib/presets.ts` makes them. `src/lib/presets.test.ts` holds the two equal, so a change to the preset changes this file in the same commit, and the Rust tests install these triggers.
