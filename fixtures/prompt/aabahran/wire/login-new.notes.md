# login-new.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `login-new` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The new build logs you in once your client answers IAC WILL GMCP. Your PROMPT is `%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c`.

`connect_char` sends Char.Status, then Char.Prompt with your PROMPT, then Char.Affects, Char.Worth, World.Time and World.Moons. `do_look` sends Room.Info. The prompt time packages follow. All of them come before the first room text and the first prompt.

Vosh takes your PROMPT from Char.Prompt with no typing, and a capture that follows the game reads the first prompt with it.
