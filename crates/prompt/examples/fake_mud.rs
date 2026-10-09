//! A fake Aabahran on a local port, for scripted runs of the app with a
//! scratch HOME.
//!
//! It serves the test kit's fake game, one [`Mud`] per connection, so the
//! app meets the same pulses the tests read. Run it with
//!
//! ```text
//! cargo run -p vosh-prompt --example fake_mud -- --port 4556
//! cargo run -p vosh-prompt --example fake_mud -- --port 4558 --build older
//! ```
//!
//! Options.
//!
//! - `--port N` listens on port N, 4556 when left out.
//! - `--build new`, `--build 243cac5c` or `--build older` picks the
//!   server build, the new build when left out.
//! - `--name NAME` logs in as NAME, Tester when left out.
//! - `--prompt TEXT` and `--fprompt TEXT` set the PROMPT and fight prompt
//!   the game holds at login, stored as `do_prompt` stores what you type.
//!   `--prompt all` starts on `prompt all`.
//! - `--reconnect` plays a link dead reconnect, with no Char.Status and
//!   no Char.Prompt.
//! - `--no-ga` turns telnet GA off, and `--compact` turns compact on.
//! - `--eor` answers each IAC DO EOR with IAC WILL EOR, as a game that
//!   keeps no state for it would, and then ends each prompt with IAC EOR
//!   in place of GA.
//! - `--wizi N` and `--incog N` log you in as an immortal at those
//!   levels, so `(Wizi N) ` and `(Incog N) ` come before each prompt.
//!
//! In the game, `prompt`, `fprompt`, `look`, `afk`, `compact`,
//! `telnetga` and `quit` work as the game's own commands. `fight`,
//! `lament` and `blind` change what you face, `split` cuts the next
//! prompt in two writes, `spam N` sends N lines, and `pulses N` sends N
//! pulses of output with no command before them.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use vosh_prompt::testkit::mud::{self, stored};
use vosh_prompt::testkit::{Build, Mud, Options};

const USAGE: &str = "usage: fake_mud [--port N] [--build new|243cac5c|older] [--name NAME] [--prompt TEXT] [--fprompt TEXT] [--reconnect] [--no-ga] [--eor] [--compact] [--wizi N] [--incog N]";

/// What the command line asked for.
struct Args {
    port: u16,
    options: Options,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut port = 4556;
    let mut options = Options::new(Build::New);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--port" => {
                port = value("--port")?
                    .parse()
                    .map_err(|_| "--port needs a number".to_string())?;
            }
            "--build" => {
                let name = value("--build")?;
                options.build = Build::parse(&name).ok_or(format!("no build is called {name}"))?;
            }
            "--name" => options.name = value("--name")?,
            "--prompt" => {
                let text = value("--prompt")?;
                options.prompt = match text.as_str() {
                    "all" => mud::PROMPT_ALL.to_string(),
                    "" => String::new(),
                    _ => stored(&text),
                };
            }
            "--fprompt" => {
                let text = value("--fprompt")?;
                options.fprompt = if text.is_empty() {
                    String::new()
                } else {
                    stored(&text)
                };
            }
            "--reconnect" => options.reconnect = true,
            "--no-ga" => options.ga = false,
            "--eor" => options.eor = true,
            "--compact" => options.compact = true,
            "--wizi" => {
                options.wizi = value("--wizi")?
                    .parse()
                    .map_err(|_| "--wizi needs a number".to_string())?;
            }
            "--incog" => {
                options.incog = value("--incog")?
                    .parse()
                    .map_err(|_| "--incog needs a number".to_string())?;
            }
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("{other} is no option\n{USAGE}")),
        }
    }
    Ok(Args { port, options })
}

/// Play one connection until the client leaves or you quit.
async fn serve(mut socket: TcpStream, options: Options) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    let mut mud = Mud::new(options);
    socket.write_all(&mud.greeting()).await?;
    let mut buf = [0u8; 4096];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        for write in mud.receive(&buf[..n]) {
            if write.after_ms > 0 {
                tokio::time::sleep(Duration::from_millis(write.after_ms)).await;
            }
            socket.write_all(&write.bytes).await?;
            if write.close {
                socket.shutdown().await?;
                return Ok(());
            }
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
    println!(
        "The fake Aabahran, {} build, listens on 127.0.0.1:{} for {}.",
        args.options.build.name(),
        args.port,
        args.options.name
    );
    loop {
        let (socket, peer) = listener.accept().await?;
        println!("{peer} connected.");
        let options = args.options.clone();
        tokio::spawn(async move {
            if let Err(e) = serve(socket, options).await {
                eprintln!("{peer} left with an error. {e}");
            } else {
                println!("{peer} left.");
            }
        });
    }
}
