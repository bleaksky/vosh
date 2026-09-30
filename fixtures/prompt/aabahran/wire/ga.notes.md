# ga.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `ga` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The new build answers `look` with `prompt all` and GA on. Your PROMPT is `%n%P%C<%hhp %mm %vmv> `.

The read ends in the partial prompt `<1020hp 800m 930mv> ` and IAC GA in the same read.

Vosh draws the prompt at the GA, with nothing painted raw first.
