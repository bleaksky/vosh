//! The row the session log keeps for each connection.

#[test]
fn the_log_row_takes_the_first_character_char_status_names() {
    let mut session = super::LogSession::new(Some(7));
    assert_eq!(session.name("Orla"), Some((7, "Orla".to_string())));
    assert_eq!(session.name("Maren"), None, "later pulses name nothing");
    // A connection with logging off names nothing.
    assert_eq!(super::LogSession::new(None).name("Orla"), None);
}
