# fight-tank.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `fight-tank` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The new build answers `fight`, and a Blackwatch guard hits you, so you tank. Your PROMPT is `%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c`.

Char.Combat names the guard at 54 percent and carries the `tank` object with your name at 75 percent. The text is the attack, the battle line, a blank line, the tank line `Tester: [===|===|===|---]` that `%n%P%C` prints, then the vitals line and IAC GA.

Vosh reads the two lines as the Tank shape. A design that reads nothing on the tank line leaves it as the game sent it and draws over the last line.
