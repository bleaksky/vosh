# Aabahran GMCP packets

Each `.gmcp` file holds one GMCP payload the way the Aabahran server writes it between `IAC SB 201` and `IAC SE`. That is the package name, a space, and the JSON. The Rust tests hand the bytes to `vosh_gmcp::parse`, and the TypeScript tests split them the same way and feed the data to the stores.

These packets are written by hand, not captured. They follow the server's `docs/gmcp-spec.md` and the emitters in `gmcp.c` as of 2026-09-29, with the lamented tears `hidden` flag, the Char.Combat `tank` field, and the Char.Prompt, Char.State and Room.Weather packages. Character names are placeholders.

| File | What the server sends it for |
| --- | --- |
| `char-vitals.gmcp` | Your vitals at a prompt. |
| `char-vitals-hidden.gmcp` | Your vitals under lamented tears. Every field is 0 and `hidden` is true. |
| `char-affects.gmcp` | Your affects at login or on a change. |
| `char-affects-hidden.gmcp` | Your affects under lamented tears. The list is empty and `hidden` is true. |
| `char-combat.gmcp` | A fight where you see your opponent's health. |
| `char-combat-hidden.gmcp` | A fight where the game withholds that health. Lamented tears, blindness, mirror image, or a target in another room all do it. |
| `char-combat-tank.gmcp` | A fight where your opponent hits someone in your group. |
| `char-combat-tank-hidden.gmcp` | The same fight under lamented tears. The tank keeps its name and loses its health. |
| `char-combat-end.gmcp` | The fight ends. |
| `char-prompt.gmcp` | Your prompt settings at login, with the stock prompt. |
| `char-prompt-off.gmcp` | Your prompt settings after `prompt off`. |
| `char-prompt-fight.gmcp` | Your prompt settings with a fight prompt and a colour code, kept raw. |
| `char-state.gmcp` | Your position and spoken language at a prompt. |
| `room-weather.gmcp` | The weather outdoors at a prompt. |
| `room-weather-indoors.gmcp` | The weather indoors, in Celsius. |
| `group-info.gmcp` | Your group at a prompt. |
| `group-info-solo.gmcp` | No group. |
| `group-info-hidden.gmcp` | Your group under lamented tears. |
