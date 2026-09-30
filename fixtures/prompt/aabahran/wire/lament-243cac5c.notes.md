# lament-243cac5c.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `lament-243cac5c` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

Build 243cac5c answers `lament` while you tank. Char.Affects goes out at once as `{"affects":[]}`, with no flag.

Char.Vitals sends zeros, Char.Combat sends the guard's name alone, and Group.Info sends `{}`, all with no flag. This build sends no Char.State, no Room.Weather and no `tank` object, and it drops the battle line. The prompt prints `Tester: ` then `[0/0hp 0/0mn 0/0mv]`.

Vosh hides your vitals, the opponent, your affects and your group by the derived terms. The tank's health reads Hidden from the empty `%P` after `%n`.
