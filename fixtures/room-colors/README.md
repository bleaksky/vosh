# Room colors fixtures

Hand written and synthetic. Nothing here was captured from a live session. Every line is game text the Aabahran server prints, built from its own format strings and area files, which live in the forsaken_lands source and the `area` folder beside it. Tolliver and Maren are invented names for players.

## looks.json

Room looks, each as a list of events in wire order. A `gmcp` event is a packet, a `line` event is one line of text with the ANSI codes the server sends, and a `prompt` event is a prompt the server ends with a GA. `room` marks the lines a Room trigger matches, the things and the people a look lists after its exits line. `src-tauri/src/session_room_tests.rs` plays each look through the session's own steps.

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
