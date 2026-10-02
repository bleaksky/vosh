//! Splits a command line into the commands it holds.

/// Split on `;` and on newlines, treating `\;` as a literal `;`. Returns
/// the pieces with the escape removed. Callers skip the empty ones.
/// A trigger's Send template splits the same way, so `get 1.;wield 1.`
/// fires alike typed or sent.
pub(crate) fn split_commands(input: &str) -> Vec<String> {
    const SEPARATOR: char = ';';
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == SEPARATOR || next == '\\' || next == '\n' {
                    current.push(next);
                    chars.next();
                    continue;
                }
            }
            current.push(ch);
            continue;
        }
        // Newlines split commands the same way `;` does — lets aliases
        // authored in the multi-line code editor read one command per
        // line without the user needing to thread semicolons. Carriage
        // returns are dropped (no separate command) so Windows-style
        // input does not produce a stray blank command.
        if ch == '\r' {
            continue;
        }
        if ch == SEPARATOR || ch == '\n' {
            out.push(std::mem::take(&mut current));
            continue;
        }
        current.push(ch);
    }
    out.push(current);
    out
}
