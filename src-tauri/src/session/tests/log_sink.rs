//! The row the session log keeps for each connection.

#[tokio::test]
async fn the_log_row_takes_the_first_character_char_status_names() {
    let mut store = vosh_log::LogStore::in_memory().unwrap();
    let id = store.start_session("h", 1, 0).unwrap();
    let logs: crate::logs::SharedLogStore =
        std::sync::Arc::new(tokio::sync::Mutex::new(Some(store)));
    let mut session = super::LogSession::new(Some(id));
    session.name(&logs, "Tester").await;
    session.name(&logs, "Other").await;
    let named = logs
        .lock()
        .await
        .as_ref()
        .unwrap()
        .session_character(id)
        .unwrap();
    assert_eq!(named.as_deref(), Some("Tester"));
    // A connection with logging off names nothing.
    super::LogSession::new(None).name(&logs, "Tester").await;
}
