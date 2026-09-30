# prompts-off-new.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `prompts-off-new` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The new build answers `prompt off`, then sends three pulses of output with no command before them.

Char.Prompt with `"enabled":false` comes first. The game says `You will no longer see prompts.` and, unlike the older builds, prints no `Prompt set to` after it. Each pulse that follows brings every prompt time package and no prompt text.

Vosh raises the prompts off status and counts no missed prompt while the packages keep coming.
