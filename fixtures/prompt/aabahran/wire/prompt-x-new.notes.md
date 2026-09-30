# prompt-x-new.bin

Synthetic. The fake Aabahran in the test kit wrote it as case `prompt-x-new` of `crates/prompt/src/testkit/wire.rs`. It is no socat capture, and it stays marked synthetic until James approves a capture to take its place. Run `VOSH_WRITE_WIRE=1 cargo test -p vosh-prompt --test wire` to write it again after a change to the fake.

The new build answers `prompt <%h/%Hhp %m/%Mmn>`. Your PROMPT was `%n%P%C[%h/%Hhp %m/%Mmn %v/%Vmv]%c`.

Char.Prompt with the new setting `<%h/%Hhp %m/%Mmn> ` comes first, then the prompt time packages, then `Prompt set to <%h/%Hhp %m/%Mmn> `, a blank line, and the new prompt `<1020/1020hp 800/800mn> ` with IAC GA.

A capture that follows the game takes the new codes from Char.Prompt, so the prompt right after the reply already reads with them.
