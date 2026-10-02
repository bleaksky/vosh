use super::*;
use crate::forget::aabahran::is_password_prompt;
use crate::forget::replay::PasswordFinder;

/// Feed `rows` as session `sid` from id `first`, the way the log
/// stores them. Each row gets its own time except an [`Also`] row,
/// which shares the time of the line before it.
fn feed(finder: &mut PasswordFinder, sid: i64, first: i64, rows: &[Row<'_>]) {
    let mut ts = 0;
    for (i, row) in rows.iter().enumerate() {
        let id = first + i as i64;
        match row {
            Out(text) => finder.row(id, sid, id, text, false),
            Sent(line) => {
                ts = id;
                finder.row(id, sid, ts, &format!("> {line}"), true);
            }
            Also(line) => finder.row(id, sid, ts, &format!("> {line}"), true),
        }
    }
}

/// Indexes of the rows the finder blanks when `rows` are one session.
fn blanked(rows: &[Row<'_>]) -> Vec<usize> {
    let mut finder = PasswordFinder::new();
    feed(&mut finder, 7, 1, rows);
    finder
        .finish()
        .lines
        .iter()
        .map(|(id, sid)| {
            assert_eq!(*sid, 7, "a found line names another session");
            (*id - 1) as usize
        })
        .collect()
}

/// Index of the `n`th sent row (from zero) in `rows`.
fn sent_at(rows: &[Row<'_>], n: usize) -> usize {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Sent(_) | Also(_)))
        .nth(n)
        .map(|(i, _)| i)
        .expect("no such sent row")
}

// ---- the prompt test ----

#[test]
fn every_password_prompt_the_game_shows_reads_as_one() {
    // tables.c chargen_table, the prompts comm.c char_gen writes.
    for prompt in [
        "Password?> ",
        "Imm-Password?> ",
        "Enter password for Quill>",
        "Retype Password>",
        "Password> ",
        "New password> ",
        "Confirm password> ",
        "Character password> ",
        "Set immortal password> ",
        "Confirm immortal password> ",
        "Current password> ",
        "Confirm new password> ",
        "Immortal password> ",
        // comm.c nanny, the older login.
        "Password: ",
        "Please enter your Imm password: ",
        "Please enter the old password in order to use the name Quill: ",
        "Please choose a password for Quill: ",
        "Please retype password: ",
        "Retype password: ",
    ] {
        assert!(
            is_password_prompt(prompt),
            "{prompt:?} should read as a password prompt"
        );
    }
}

#[test]
fn chatter_about_passwords_is_no_prompt() {
    for line in [
        "Bob gossips 'what is the password?'",
        "You say 'my password is safe'",
        "Wrong password.",
        "Wrong password.  Wait 10 seconds.",
        "Passwords don't match.",
        "New password set.",
        "Syntax: password <old> <new>.",
        "Password must be at least five characters.",
        "Invalid account name or password.",
        "Account password changed successfully.",
        " [#] Play   [N]ew character   [L]ink character   [P]assword   [D]isconnect",
        "Bob tells you 'type password> to see it'",
        "Your choice> ",
        "Account name> ",
        "",
    ] {
        assert!(
            !is_password_prompt(line),
            "{line:?} should not read as a password prompt"
        );
    }
}

// ---- the login replay ----

#[test]
fn the_account_password_is_blanked() {
    let rows = session(&[&greeting(), &login(SECRET_ACCOUNT), &account_menu()]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
}

#[test]
fn a_wrong_account_password_and_the_retry_are_both_blanked() {
    let rows = session(&[
        &greeting(),
        &login("Wr0ngGuess"),
        &[
            Out(""),
            Out("Invalid account name or password."),
            Out("            open for play. welcome."),
            Out(""),
            Out(TAGLINE),
        ],
        &login(SECRET_ACCOUNT),
        &account_menu(),
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 5)]);
}

#[test]
fn a_short_account_name_goes_back_to_the_menu_and_blanks_nothing() {
    let rows = session(&[
        &greeting(),
        &[
            Sent("e"),
            Out(""),
            Sent("ab"),
            Out("Account name must be 3-20 characters."),
            Out("            open for play. welcome."),
            Out(""),
            Out(TAGLINE),
            Sent("h"),
            Out(""),
            Sent("rules"),
        ],
    ]);
    assert_eq!(blanked(&rows), Vec::<usize>::new());
}

#[test]
fn a_mortal_pick_goes_to_the_game_and_blanks_nothing_more() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("1"),
            Out("Welcome to the Forsaken Lands."),
            Out(""),
            Out(MOTD),
            Out(""),
            Out("The Temple Square"),
            Out(""),
            Sent("look"),
            Out("The Temple Square"),
            Sent("north"),
        ],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
}

#[test]
fn the_immortal_password_after_a_pick_is_blanked() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Sent(SECRET_IMM),
            Out("Immortals, read the board before you build."),
            Out(""),
            Out(MOTD),
            Sent("look"),
        ],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
}

#[test]
fn a_failed_immortal_password_is_blanked() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[Sent("2"), Out(""), Sent("Wr0ngImm"), Out("Login failed.")],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
}

#[test]
fn a_quiet_reconnect_after_a_pick_keeps_the_first_command() {
    // A character left link dead in the game reconnects straight from
    // the pick with nothing but the prompt's line break, and the next
    // line is an ordinary command.
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("1"),
            Out(""),
            Sent("look"),
            Out("The Temple Square"),
            Out(""),
            Sent("north"),
        ],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2)]);
}

#[test]
fn an_immortal_without_an_immortal_password_sets_one_twice() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Out("This immortal character requires an immortal password."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Out("Immortal password set and character linked successfully."),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    assert_eq!(
        blanked(&rows),
        vec![sent_at(&rows, 2), sent_at(&rows, 4), sent_at(&rows, 5)]
    );
}

#[test]
fn a_new_account_blanks_the_password_and_its_confirmation() {
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
        ],
        &account_menu(),
        &[Sent("d"), Out("Goodbye.")],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 3)]);
}

#[test]
fn a_taken_account_name_asks_for_another_name_first() {
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("taken"),
            Out("That account name is already taken."),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
        ],
        &account_menu(),
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 3), sent_at(&rows, 4)]);
}

#[test]
fn a_short_or_mismatched_new_password_asks_again_and_every_try_is_blanked() {
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent("abc"),
            Out("Password must be at least five characters."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent("Typo9cinderLark"),
            Out("Passwords don't match."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("score")],
    ]);
    assert_eq!(
        blanked(&rows),
        (2..=6).map(|n| sent_at(&rows, n)).collect::<Vec<_>>()
    );
}

#[test]
fn a_password_change_from_the_account_menu_blanks_all_three() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("p"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Out("Account password changed successfully."),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    assert_eq!(
        blanked(&rows),
        vec![
            sent_at(&rows, 2),
            sent_at(&rows, 4),
            sent_at(&rows, 5),
            sent_at(&rows, 6)
        ]
    );
}

#[test]
fn a_wrong_old_password_goes_back_to_the_menu() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("p"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Out("Wrong password."),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
}

#[test]
fn linking_a_character_blanks_its_password() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("l"),
            Out(""),
            Sent("Quill"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Out("Character linked successfully."),
        ],
        &account_menu(),
        // An immortal asks for the immortal password too.
        &[
            Sent("l"),
            Out(""),
            Sent("Vellin"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Out("Immortal character detected. Verify immortal password."),
            Out(""),
            Sent(SECRET_IMM),
            Out(""),
            Out("Character linked successfully."),
        ],
        &account_menu(),
        // One with no immortal password sets one, twice.
        &[
            Sent("l"),
            Out(""),
            Sent("Maudrey"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Out("This immortal character requires an immortal password before linking."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Out("Immortal password set and character linked successfully."),
        ],
        &account_menu(),
        // A name that does not exist goes straight back to the menu.
        &[
            Sent("l"),
            Out(""),
            Sent("Nobody"),
            Out("That character doesn't exist."),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    let expected: Vec<usize> = [2, 5, 8, 9, 12, 13, 14]
        .iter()
        .map(|&n| sent_at(&rows, n))
        .collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn a_new_character_blanks_its_password_and_the_retype() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("n"),
            Out(""),
            Sent("Qu1ll"),
            Out("Illegal name, try another."),
            Out(""),
            Sent("Quilla"),
            Sent("y"),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent("Typo9cinderLark"),
            Out("Passwords do not match."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent("n"),
            Out(""),
            Sent("m"),
        ],
    ]);
    let expected: Vec<usize> = [2, 7, 8, 9, 10]
        .iter()
        .map(|&n| sent_at(&rows, n))
        .collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn connecting_past_a_character_already_playing_asks_for_its_password() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("1"),
            Out("That character is already playing."),
            Out(""),
            Sent("n"),
            Out(""),
            Sent("Quill"),
            Out(""),
            Sent(SECRET_OLD),
            Out("Welcome."),
            Out(MOTD),
            Sent("look"),
        ],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 6)]);
}

#[test]
fn the_direct_login_asks_an_immortal_twice() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out("That character is already playing."),
            Out(""),
            Sent("y"),
            Out("Reconnect attempt failed."),
            Out(""),
            Sent("Vellin"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Sent(SECRET_IMM),
            Out("Immortals, read the board."),
            Out(MOTD),
            Sent("look"),
        ],
    ]);
    assert_eq!(
        blanked(&rows),
        vec![sent_at(&rows, 2), sent_at(&rows, 6), sent_at(&rows, 7)]
    );
}

#[test]
fn a_password_command_in_the_game_is_blanked() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("1"),
            Out("Welcome."),
            Out(MOTD),
            Sent("password Mn2quartzWren Pw9cinderLark"),
            Out("New password set."),
            Sent("pa Mn2quartzWren Pw9cinderLark"),
            Sent("PASSW Mn2quartzWren"),
            Sent("acctpassword Mn2quartzWren Pw9cinderLark"),
            Sent("acct Mn2quartzWren Pw9cinderLark"),
            Sent("delpassword Mn2quartzWren Pw9cinderLark"),
            Sent("immpass Pw9cinderLark Pw9cinderLark"),
            Sent("delete Pw9cinderLark"),
            // No argument, or not a password command.
            Sent("password"),
            Sent("pat dog"),
            Sent("p Mn2quartzWren"),
            Sent("acc sword"),
            Sent("delet"),
            Sent("delete"),
            Sent("dele Mn2quartzWren"),
            Sent("say my password is safe"),
            Sent("practice"),
        ],
    ]);
    // The account password, then every password command from
    // `password` through `delete`. The pick in between keeps its text.
    let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11]
        .iter()
        .map(|&n| sent_at(&rows, n))
        .collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn an_implementor_command_that_sets_a_password_is_blanked() {
    // act_info.c do_account and act_wiz.c do_mset, reached through
    // do_set, take a new password as their last argument. The game
    // reads the command, the subcommand, and the field by prefix.
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Sent(SECRET_IMM),
            Out(MOTD),
            Sent("account password Tester Pw9cinderLark"),
            Out("Password for account 'Tester' has been changed."),
            Sent("acco p Tester Pw9cinderLark"),
            Sent("ACCOUNT Pass Tester Pw9cinderLark"),
            Sent("set char Quill pwdreset Pw9cinderLark"),
            Out("New password set."),
            Sent("set c Quill pw Pw9cinderLark"),
            Sent("set mob Vellin immpwdreset Pw9cinderLark"),
            Sent("SET M Vellin i Pw9cinderLark"),
            Sent("set cha Vellin immpw two words"),
            // Another subcommand or field, another command, or no
            // password given.
            Sent("account list"),
            Sent("account password Tester"),
            Sent("account link Quill Tester"),
            Sent("acc password Tester Pw9cinderLark"),
            Sent("set char Quill pwdreset"),
            Sent("set char Quill p 5"),
            Sent("set char Quill int 18"),
            Sent("set skill Quill pw 75"),
            Sent("set obj sword i 5"),
            Sent("se char Quill pwdreset Pw9cinderLark"),
            Sent("say set char Quill pwdreset is the syntax"),
        ],
    ]);
    // The account password, the immortal password, then the eight
    // commands from `account password` through `set cha`.
    let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        .iter()
        .map(|&n| sent_at(&rows, n))
        .collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn chatter_before_a_line_blanks_nothing() {
    let rows = vec![
        Out("Bob gossips 'what is the password?'"),
        Sent("laugh"),
        Out("Wrong password."),
        Sent("look"),
        Out("Password must be at least five characters."),
        Sent("north"),
        Out("   Abandon hope, all ye who enter here..."),
        Sent("east"),
        Out(""),
        Sent("look"),
        Out(""),
        Sent("score"),
    ];
    assert_eq!(blanked(&rows), Vec::<usize>::new());
}

#[test]
fn a_line_right_after_a_logged_prompt_is_blanked() {
    // The prompt reaches the log when the game ends its line before
    // you answer. Any world, any prompt the game uses.
    let rows = vec![
        Out("By what name do you wish to be known?"),
        Sent("Quill"),
        Out("Password: "),
        Sent(SECRET_OLD),
        Out(""),
        Out("Welcome."),
        Sent("look"),
        Out("Enter password for Quill>"),
        Out("Something else first."),
        Sent("north"),
    ];
    assert_eq!(blanked(&rows), vec![3]);
}

#[test]
fn a_line_already_hidden_is_not_counted_again() {
    let rows = session(&[
        &greeting(),
        &login("(hidden)"),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Sent("(hidden)"),
            Out(MOTD),
            Sent("look"),
        ],
    ]);
    assert_eq!(blanked(&rows), Vec::<usize>::new());
}

#[test]
fn every_piece_of_a_password_split_at_a_semicolon_is_blanked() {
    // Before masked input skipped the pipeline, a `;` in a password
    // split it into lines the session logged together, one write,
    // one time. The game took the first piece as the answer.
    let rows = session(&[
        &greeting(),
        &[
            Sent("e"),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent("Zq7vellum"),
            Also("Sparrow"),
            Also("Branwick"),
            Out(""),
            Out("Invalid account name or password."),
            Out(TAGLINE),
            Sent("e"),
            // Its own write, so its own time.
            Sent("look"),
        ],
    ]);
    assert_eq!(
        blanked(&rows),
        vec![sent_at(&rows, 2), sent_at(&rows, 3), sent_at(&rows, 4)]
    );
}

#[test]
fn a_short_new_password_split_in_two_asks_twice_and_every_try_is_blanked() {
    // The game reads each piece as its own answer and turns down both,
    // so it asks for the new password and its confirmation after.
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent("ab"),
            Also("cd"),
            Out("Password must be at least five characters."),
            Out(""),
            Out("Password must be at least five characters."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    let expected: Vec<usize> = (2..=5).map(|n| sent_at(&rows, n)).collect();
    assert_eq!(blanked(&rows), expected);
}

// The game reads one piece of a split line each pulse. The login sends
// no GA, so a prompt it prints between two pieces stays a partial
// line until the next pulse's output ends it. It then lands in the
// log as a row of its own, or joined to the front of the next line.

#[test]
fn a_split_password_refused_at_the_confirmation_keeps_every_later_try_blanked() {
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("tester"),
            Out(""),
            // `ab` is too short, and `cdefgh` becomes the new password.
            Sent("ab"),
            Also("cdefgh"),
            Out("Password must be at least five characters."),
            Out(""),
            Out("New password> "),
            // `ab` fails the confirmation, and `cdefgh` becomes the
            // new password again.
            Sent("ab"),
            Also("cdefgh"),
            Out("Passwords don't match."),
            Out(""),
            Out("New password> "),
            // A confirmation that fails, then the password you keep,
            // twice.
            Sent("Fn7emberKite"),
            Out("Passwords don't match."),
            Out(""),
            Sent("Fn7emberKite"),
            Out(""),
            Sent("Fn7emberKite"),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    let expected: Vec<usize> = (2..=8).map(|n| sent_at(&rows, n)).collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn a_split_password_refused_in_a_password_change_keeps_every_later_try_blanked() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("p"),
            Out(""),
            Sent(SECRET_OLD),
            Out(""),
            Sent("ab"),
            Also("cdefgh"),
            Out("Password must be at least five characters."),
            Out(""),
            Out("New password> "),
            Sent("ab"),
            Also("cdefgh"),
            Out("Passwords don't match."),
            Out(""),
            Out("New password> "),
            Sent("Fn7emberKite"),
            Out("Passwords don't match."),
            Out(""),
            Sent("Fn7emberKite"),
            Out(""),
            Sent("Fn7emberKite"),
            Out(""),
            Out("Account password changed successfully."),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    let expected: Vec<usize> = [2, 4, 5, 6, 7, 8, 9, 10, 11]
        .iter()
        .map(|&n| sent_at(&rows, n))
        .collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn a_prompt_joined_to_the_next_line_still_reads_as_the_reply() {
    // Both pieces are too short. The prompt after the first one joins
    // the reply to the second.
    let rows = session(&[
        &greeting(),
        &[
            Sent("c"),
            Out(""),
            Sent("tester"),
            Out(""),
            Sent("ab"),
            Also("cd"),
            Out("Password must be at least five characters."),
            Out(""),
            Out("New password> Password must be at least five characters."),
            Out(""),
            Sent(SECRET_NEW),
            Out(""),
            Sent(SECRET_NEW),
        ],
        &account_menu(),
        &[Sent("1"), Out("Welcome."), Out(MOTD), Sent("look")],
    ]);
    let expected: Vec<usize> = (2..=5).map(|n| sent_at(&rows, n)).collect();
    assert_eq!(blanked(&rows), expected);
}

#[test]
fn an_immortal_password_split_in_two_is_blanked_whole() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[
            Sent("2"),
            Out(""),
            Sent("Kx4tallow"),
            Also("Heron"),
            Out("Login failed."),
        ],
    ]);
    assert_eq!(
        blanked(&rows),
        vec![sent_at(&rows, 2), sent_at(&rows, 4), sent_at(&rows, 5)]
    );
}

#[test]
fn a_session_that_ends_after_an_immortal_password_blanks_it() {
    let rows = session(&[
        &greeting(),
        &login(SECRET_ACCOUNT),
        &account_menu(),
        &[Sent("2"), Out(""), Sent(SECRET_IMM)],
    ]);
    assert_eq!(blanked(&rows), vec![sent_at(&rows, 2), sent_at(&rows, 4)]);
}

#[test]
fn sessions_replay_apart_even_when_their_rows_interleave() {
    let mut finder = PasswordFinder::new();
    let a = session(&[&greeting(), &login(SECRET_ACCOUNT)]);
    let b = session(&[&greeting(), &[Sent("e"), Out(""), Sent("look")]]);
    let mut id = 0;
    let mut expected = Vec::new();
    for i in 0..a.len().max(b.len()) {
        for (sid, rows) in [(1_i64, &a), (2_i64, &b)] {
            if let Some(row) = rows.get(i) {
                id += 1;
                feed(&mut finder, sid, id, &[*row]);
                if sid == 1 && i == a.len() - 1 {
                    expected.push((id, sid));
                }
            }
        }
    }
    let found = finder.finish();
    assert_eq!(found.lines, expected);
    assert_eq!(found.sessions(), 1);
}
