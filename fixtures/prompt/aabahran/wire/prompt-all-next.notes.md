# prompt-all-next.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `prompt-all-next` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The older build with `prompt all` and no GA. Your PROMPT is `%n%P%C<%hhp %mm %vmv> `, which ends with no line end.

The read holds the reply to `look`, which ends in the partial prompt `<1020hp 800m 930mv> `. Output with no command before it follows in the same read. Its packets come first, then the line end `write_to_buffer` starts it with, which completes the partial, then someone arriving and the next partial prompt.

Vosh draws the first prompt once its line end completes it and settles on the second at the end of the read.
