//! Lua's string patterns, matched in Rust so the time limit reaches
//! inside them. The C matcher in lstrlib.c backtracks inside one C call,
//! where the hook cannot look, so a pattern like `.-.-.-.-b` on a long
//! line held the session loop for good. This is the same algorithm, step
//! for step from Lua 5.4.7, with a count of its steps that looks at the
//! clock now and then.
//!
//! Character classes read as they do in the C locale, the only one a
//! script sees, since the sandbox takes `os.setlocale` away.

/// How many captures a pattern may hold.
const MAXCAPTURES: usize = 32;
/// How deep `do_match` may go before the pattern is too complex.
const MAXCCALLS: usize = 200;
/// How many steps run between two looks at the clock.
const STEPS_PER_LOOK: u32 = 1024;
const L_ESC: u8 = b'%';
const SPECIALS: &[u8] = b"^$*+?.([%-";

/// Why a match gave up.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fail {
    /// The pattern or a replacement is not one Lua reads, with the line
    /// Lua gives.
    Error(String),
    /// The call ran out of time.
    Stopped,
}

fn error(text: impl Into<String>) -> Fail {
    Fail::Error(text.into())
}

/// What a capture holds once the match closed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capture {
    /// The bytes from the first index to the second.
    Text(usize, usize),
    /// A position capture, `()`, as the index Lua counts from 1.
    Position(usize),
}

#[derive(Debug, Clone, Copy)]
enum CapLen {
    Unfinished,
    Position,
    Len(usize),
}

/// The state of one match, as lstrlib.c keeps it.
pub(crate) struct Matcher<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    level: usize,
    capture: [(usize, CapLen); MAXCAPTURES],
    matchdepth: usize,
    steps: u32,
    /// True once the call is out of time.
    out_of_time: &'a mut dyn FnMut() -> bool,
}

impl<'a> Matcher<'a> {
    /// A matcher of `pat` against `src`. `out_of_time` says whether the
    /// call ran out of time, and the matcher asks it every few thousand
    /// steps.
    pub(crate) fn new(
        src: &'a [u8],
        pat: &'a [u8],
        out_of_time: &'a mut dyn FnMut() -> bool,
    ) -> Self {
        Self {
            src,
            pat,
            level: 0,
            capture: [(0, CapLen::Unfinished); MAXCAPTURES],
            matchdepth: MAXCCALLS,
            steps: 0,
            out_of_time,
        }
    }

    /// Count `n` steps, and give up once the call is out of time.
    fn tick(&mut self, n: u32) -> Result<(), Fail> {
        self.steps = self.steps.saturating_add(n);
        if self.steps >= STEPS_PER_LOOK {
            self.steps = 0;
            if (self.out_of_time)() {
                return Err(Fail::Stopped);
            }
        }
        Ok(())
    }

    /// The pattern byte at `i`, or the NUL that ends the pattern in C.
    fn p(&self, i: usize) -> u8 {
        self.pat.get(i).copied().unwrap_or(0)
    }

    /// The subject byte at `i`, or the NUL that ends the string in C.
    fn s(&self, i: usize) -> u8 {
        self.src.get(i).copied().unwrap_or(0)
    }

    /// Match the pattern from `p` against the subject from `s`, with no
    /// capture open, as `do_match` does after `reprepstate`.
    pub(crate) fn match_at(&mut self, s: usize, p: usize) -> Result<Option<usize>, Fail> {
        self.level = 0;
        self.do_match(s, p)
    }

    fn check_capture(&self, l: u8) -> Result<usize, Fail> {
        let l = i32::from(l) - i32::from(b'1');
        match usize::try_from(l) {
            Ok(l) if l < self.level && !matches!(self.capture[l].1, CapLen::Unfinished) => Ok(l),
            _ => Err(error(format!("invalid capture index %{}", l + 1))),
        }
    }

    fn capture_to_close(&self) -> Result<usize, Fail> {
        (0..self.level)
            .rev()
            .find(|l| matches!(self.capture[*l].1, CapLen::Unfinished))
            .ok_or_else(|| error("invalid pattern capture"))
    }

    fn classend(&self, p: usize) -> Result<usize, Fail> {
        let mut p = p;
        let c = self.p(p);
        p += 1;
        match c {
            L_ESC => {
                if p >= self.pat.len() {
                    return Err(error("malformed pattern (ends with '%')"));
                }
                Ok(p + 1)
            }
            b'[' => {
                if self.p(p) == b'^' {
                    p += 1;
                }
                loop {
                    if p >= self.pat.len() {
                        return Err(error("malformed pattern (missing ']')"));
                    }
                    let c = self.p(p);
                    p += 1;
                    if c == L_ESC && p < self.pat.len() {
                        p += 1;
                    }
                    if self.p(p) == b']' {
                        break;
                    }
                }
                Ok(p + 1)
            }
            _ => Ok(p),
        }
    }

    fn matchbracketclass(&self, c: u8, p: usize, ec: usize) -> bool {
        let mut p = p;
        let mut sig = true;
        if self.p(p + 1) == b'^' {
            sig = false;
            p += 1;
        }
        loop {
            p += 1;
            if p >= ec {
                break;
            }
            if self.p(p) == L_ESC {
                p += 1;
                if match_class(c, self.p(p)) {
                    return sig;
                }
            } else if self.p(p + 1) == b'-' && p + 2 < ec {
                p += 2;
                if self.p(p - 2) <= c && c <= self.p(p) {
                    return sig;
                }
            } else if self.p(p) == c {
                return sig;
            }
        }
        !sig
    }

    fn singlematch(&self, s: usize, p: usize, ep: usize) -> bool {
        let Some(&c) = self.src.get(s) else {
            return false;
        };
        match self.p(p) {
            b'.' => true,
            L_ESC => match_class(c, self.p(p + 1)),
            b'[' => self.matchbracketclass(c, p, ep - 1),
            other => other == c,
        }
    }

    fn matchbalance(&mut self, s: usize, p: usize) -> Result<Option<usize>, Fail> {
        if p + 1 >= self.pat.len() {
            return Err(error("malformed pattern (missing arguments to '%b')"));
        }
        if self.s(s) != self.p(p) {
            return Ok(None);
        }
        let open = self.p(p);
        let close = self.p(p + 1);
        let mut depth = 1;
        let mut s = s;
        loop {
            s += 1;
            if s >= self.src.len() {
                return Ok(None);
            }
            self.tick(1)?;
            let c = self.src[s];
            if c == close {
                depth -= 1;
                if depth == 0 {
                    return Ok(Some(s + 1));
                }
            } else if c == open {
                depth += 1;
            }
        }
    }

    fn max_expand(&mut self, s: usize, p: usize, ep: usize) -> Result<Option<usize>, Fail> {
        let mut i = 0;
        while self.singlematch(s + i, p, ep) {
            i += 1;
            self.tick(1)?;
        }
        loop {
            if let Some(res) = self.do_match(s + i, ep + 1)? {
                return Ok(Some(res));
            }
            if i == 0 {
                return Ok(None);
            }
            i -= 1;
        }
    }

    fn min_expand(&mut self, s: usize, p: usize, ep: usize) -> Result<Option<usize>, Fail> {
        let mut s = s;
        loop {
            if let Some(res) = self.do_match(s, ep + 1)? {
                return Ok(Some(res));
            }
            if self.singlematch(s, p, ep) {
                s += 1;
            } else {
                return Ok(None);
            }
        }
    }

    fn start_capture(&mut self, s: usize, p: usize, what: CapLen) -> Result<Option<usize>, Fail> {
        if self.level >= MAXCAPTURES {
            return Err(error("too many captures"));
        }
        self.capture[self.level] = (s, what);
        self.level += 1;
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.level -= 1;
        }
        Ok(res)
    }

    fn end_capture(&mut self, s: usize, p: usize) -> Result<Option<usize>, Fail> {
        let l = self.capture_to_close()?;
        self.capture[l].1 = CapLen::Len(s - self.capture[l].0);
        let res = self.do_match(s, p)?;
        if res.is_none() {
            self.capture[l].1 = CapLen::Unfinished;
        }
        Ok(res)
    }

    fn match_capture(&mut self, s: usize, l: u8) -> Result<Option<usize>, Fail> {
        let l = self.check_capture(l)?;
        let (init, len) = self.capture[l];
        let CapLen::Len(len) = len else {
            // A position capture never matches text.
            return Ok(None);
        };
        self.tick(u32::try_from(len / 64).unwrap_or(u32::MAX))?;
        if self.src.len() - s >= len && self.src[init..init + len] == self.src[s..s + len] {
            Ok(Some(s + len))
        } else {
            Ok(None)
        }
    }

    fn do_match(&mut self, s: usize, p: usize) -> Result<Option<usize>, Fail> {
        if self.matchdepth == 0 {
            return Err(error("pattern too complex"));
        }
        self.matchdepth -= 1;
        self.tick(1)?;
        let (mut s, mut p) = (s, p);
        let res = loop {
            if p == self.pat.len() {
                break Some(s);
            }
            match self.pat[p] {
                b'(' => {
                    break if self.p(p + 1) == b')' {
                        self.start_capture(s, p + 2, CapLen::Position)?
                    } else {
                        self.start_capture(s, p + 1, CapLen::Unfinished)?
                    };
                }
                b')' => break self.end_capture(s, p + 1)?,
                b'$' if p + 1 == self.pat.len() => {
                    break (s == self.src.len()).then_some(s);
                }
                L_ESC if self.p(p + 1) == b'b' => match self.matchbalance(s, p + 2)? {
                    Some(next) => {
                        s = next;
                        p += 4;
                    }
                    None => break None,
                },
                L_ESC if self.p(p + 1) == b'f' => {
                    p += 2;
                    if self.p(p) != b'[' {
                        return Err(error("missing '[' after '%f' in pattern"));
                    }
                    let ep = self.classend(p)?;
                    let previous = if s == 0 { 0 } else { self.src[s - 1] };
                    if !self.matchbracketclass(previous, p, ep - 1)
                        && self.matchbracketclass(self.s(s), p, ep - 1)
                    {
                        p = ep;
                    } else {
                        break None;
                    }
                }
                L_ESC if self.p(p + 1).is_ascii_digit() => {
                    match self.match_capture(s, self.p(p + 1))? {
                        Some(next) => {
                            s = next;
                            p += 2;
                        }
                        None => break None,
                    }
                }
                _ => {
                    let ep = self.classend(p)?;
                    let suffix = self.p(ep);
                    if !self.singlematch(s, p, ep) {
                        if matches!(suffix, b'*' | b'?' | b'-') {
                            p = ep + 1;
                            continue;
                        }
                        break None;
                    }
                    match suffix {
                        b'?' => {
                            if let Some(res) = self.do_match(s + 1, ep + 1)? {
                                break Some(res);
                            }
                            p = ep + 1;
                        }
                        b'+' => break self.max_expand(s + 1, p, ep)?,
                        b'*' => break self.max_expand(s, p, ep)?,
                        b'-' => break self.min_expand(s, p, ep)?,
                        _ => {
                            s += 1;
                            p = ep;
                        }
                    }
                }
            }
        };
        self.matchdepth += 1;
        Ok(res)
    }

    /// Capture `i` of a match from `s` to `e`. With no capture in the
    /// pattern, capture 0 is the whole match.
    fn capture_at(&self, i: usize, s: usize, e: usize) -> Result<Capture, Fail> {
        if i >= self.level {
            if i != 0 {
                return Err(error(format!("invalid capture index %{}", i + 1)));
            }
            return Ok(Capture::Text(s, e));
        }
        match self.capture[i] {
            (_, CapLen::Unfinished) => Err(error("unfinished capture")),
            (init, CapLen::Position) => Ok(Capture::Position(init + 1)),
            (init, CapLen::Len(len)) => Ok(Capture::Text(init, init + len)),
        }
    }

    /// What a match from `s` to `e` hands back: each capture, or the
    /// whole match when the pattern has none and `whole` is true.
    pub(crate) fn captures(&self, s: usize, e: usize, whole: bool) -> Result<Vec<Capture>, Fail> {
        let count = if self.level == 0 && whole {
            1
        } else {
            self.level
        };
        (0..count).map(|i| self.capture_at(i, s, e)).collect()
    }

    /// Add to `out` the replacement `repl` makes for a match from `s` to
    /// `e`, as `add_s` in lstrlib.c does.
    pub(crate) fn add_replacement(
        &self,
        out: &mut Vec<u8>,
        repl: &[u8],
        s: usize,
        e: usize,
    ) -> Result<(), Fail> {
        let mut i = 0;
        while let Some(at) = repl[i..].iter().position(|b| *b == L_ESC) {
            out.extend_from_slice(&repl[i..i + at]);
            let next = repl.get(i + at + 1).copied().unwrap_or(0);
            if next == L_ESC {
                out.push(L_ESC);
            } else if next == b'0' {
                out.extend_from_slice(&self.src[s..e]);
            } else if next.is_ascii_digit() {
                match self.capture_at(usize::from(next - b'1'), s, e)? {
                    Capture::Text(from, to) => out.extend_from_slice(&self.src[from..to]),
                    Capture::Position(at) => out.extend_from_slice(at.to_string().as_bytes()),
                }
            } else {
                return Err(error("invalid use of '%' in replacement string"));
            }
            i += at + 2;
        }
        out.extend_from_slice(&repl[i..]);
        Ok(())
    }

    /// Count one start position of a scan as a step.
    pub(crate) fn step(&mut self) -> Result<(), Fail> {
        self.tick(1)
    }
}

/// True when `pat` holds none of the bytes that make it a pattern.
pub(crate) fn no_specials(pat: &[u8]) -> bool {
    !pat.iter().any(|b| SPECIALS.contains(b))
}

fn match_class(c: u8, cl: u8) -> bool {
    let res = match cl.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'g' => c.is_ascii_graphic(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        // isspace in C counts the vertical tab, which Rust's whitespace
        // leaves out.
        b's' => matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        b'z' => c == 0,
        _ => return cl == c,
    };
    if cl.is_ascii_lowercase() {
        res
    } else {
        !res
    }
}

/// Where a 1-based or negative string position falls, as `posrelatI`
/// in lstrlib.c gives it: 1 for anything at or before the start.
pub(crate) fn start_position(pos: i64, len: usize) -> usize {
    match pos.cmp(&0) {
        std::cmp::Ordering::Greater => usize::try_from(pos).unwrap_or(usize::MAX),
        std::cmp::Ordering::Equal => 1,
        std::cmp::Ordering::Less => match usize::try_from(pos.unsigned_abs()) {
            Ok(back) if back <= len => len - back + 1,
            _ => 1,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(src: &str, pat: &str) -> Result<Option<(usize, usize)>, Fail> {
        let mut never = || false;
        let mut m = Matcher::new(src.as_bytes(), pat.as_bytes(), &mut never);
        for s in 0..=src.len() {
            if let Some(e) = m.match_at(s, 0)? {
                return Ok(Some((s, e)));
            }
        }
        Ok(None)
    }

    #[test]
    fn classes_read_as_in_the_c_locale() {
        for c in 0..=255u8 {
            assert_eq!(match_class(c, b's'), b" \t\n\x0b\x0c\r".contains(&c), "{c}");
            assert_eq!(
                match_class(c, b'p'),
                c.is_ascii_graphic() && !c.is_ascii_alphanumeric(),
                "{c}"
            );
            assert!(!match_class(c, b'a') || c < 128);
        }
        assert!(!match_class(b'A', b'U'));
        assert!(match_class(b'.', b'.'));
    }

    #[test]
    fn a_match_runs_as_lua_runs_it() {
        assert_eq!(find("hello world", "o w"), Ok(Some((4, 7))));
        assert_eq!(find("THE (quick) fox", "%((%a+)%)"), Ok(Some((4, 11))));
        assert_eq!(find("aaab", "a-b"), Ok(Some((0, 4))));
        assert_eq!(find("x = [[a]] y", "%b[]"), Ok(Some((4, 9))));
        assert_eq!(find("THE quick", "%f[%a]%a+"), Ok(Some((0, 3))));
        assert_eq!(
            find("abc", "[a-"),
            Err(error("malformed pattern (missing ']')"))
        );
        assert_eq!(
            find("abc", "%"),
            Err(error("malformed pattern (ends with '%')"))
        );
        assert_eq!(find("abc", "(()"), Ok(Some((0, 0))));
    }

    #[test]
    fn a_long_backtrack_gives_up_once_out_of_time() {
        let src = "a".repeat(3000);
        let mut looks = 0;
        let mut out_of_time = || {
            looks += 1;
            looks > 50
        };
        let mut m = Matcher::new(src.as_bytes(), b".-.-.-.-.-.-b", &mut out_of_time);
        let mut result = Ok(None);
        for s in 0..=src.len() {
            result = m.match_at(s, 0);
            if !matches!(result, Ok(None)) {
                break;
            }
        }
        assert_eq!(result, Err(Fail::Stopped));
    }
}
