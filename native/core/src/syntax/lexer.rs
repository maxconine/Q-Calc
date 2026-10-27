//! mathjs's tokenizer (`getToken` in mathjs `expression/parse.js`), character for character.

use super::ParseError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Null,
    Delimiter,
    Number,
    Symbol,
    Unknown,
}

const DELIMITERS: &[&str] = &[
    ",", "(", ")", "[", "]", "{", "}", "\"", "'", ";", "+", "-", "*", ".*", "/", "./", "%", "^",
    ".^", "~", "!", "&", "|", "^|", "=", ":", "?", "?.", "??", "==", "!=", "<", ">", "<=", ">=",
    "<<", ">>", ">>>",
];

pub(crate) const NAMED_DELIMITERS: &[&str] = &["mod", "to", "in", "and", "xor", "or", "not"];

/// The tokenizer's whole state; the parser copies it to look ahead and rewind, as mathjs does.
#[derive(Clone, Debug)]
pub struct Lexer {
    chars: std::rc::Rc<[char]>,
    pub index: usize,
    pub token: String,
    pub token_type: TokenType,
    pub nesting_level: usize,
    pub conditional_level: Option<usize>,
}

fn is_digit(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}

fn is_latin_or_greek(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || c == '$'
        || ('\u{00C0}'..='\u{02AF}').contains(&c)
        || ('\u{0370}'..='\u{03FF}').contains(&c)
        || ('\u{2100}'..='\u{214F}').contains(&c)
}

// mathjs checks surrogate pairs for U+1D400–U+1D7FF less its unassigned holes; one Rust char is one code point
fn is_math_symbol(c: char) -> bool {
    const HOLES: &[u32] = &[
        0x1D455, 0x1D49D, 0x1D4A0, 0x1D4A1, 0x1D4A3, 0x1D4A4, 0x1D4A7, 0x1D4A8, 0x1D4AD, 0x1D4BA,
        0x1D4BC, 0x1D4C4, 0x1D506, 0x1D50B, 0x1D50C, 0x1D515, 0x1D51D, 0x1D53A, 0x1D53F, 0x1D545,
        0x1D547, 0x1D548, 0x1D549, 0x1D551, 0x1D6A6, 0x1D6A7, 0x1D7CC, 0x1D7CD,
    ];
    let u = c as u32;
    (0x1D400..=0x1D7FF).contains(&u) && !HOLES.contains(&u)
}

fn is_alpha(c: Option<char>) -> bool {
    c.is_some_and(|c| is_latin_or_greek(c) || is_math_symbol(c))
}

fn is_whitespace(c: Option<char>, nesting: usize) -> bool {
    matches!(c, Some(' ' | '\t' | '\u{00A0}')) || (c == Some('\n') && nesting > 0)
}

fn is_decimal_mark(c: Option<char>, next: Option<char>) -> bool {
    c == Some('.') && !matches!(next, Some('/' | '*' | '^'))
}

impl Lexer {
    pub fn new(expression: &str) -> Self {
        Lexer {
            chars: expression.chars().collect(),
            index: 0,
            token: String::new(),
            token_type: TokenType::Null,
            nesting_level: 0,
            conditional_level: None,
        }
    }

    fn at(&self, i: usize) -> Option<char> {
        self.chars.get(i).copied()
    }

    fn cur(&self) -> Option<char> {
        self.at(self.index)
    }

    fn next_char(&self) -> Option<char> {
        self.at(self.index + 1)
    }

    fn take(&mut self) {
        if let Some(c) = self.cur() {
            self.token.push(c);
        }
        self.index += 1;
    }

    fn peek_str(&self, len: usize) -> String {
        self.chars.iter().skip(self.index).take(len).collect()
    }

    pub fn error(&self, message: &str) -> ParseError {
        let col = (self.index + 1).saturating_sub(self.token.chars().count());
        ParseError {
            message: format!("{message} (char {col})"),
            char: col,
        }
    }

    /// Reads the next token into `token` and `token_type`.
    pub fn advance(&mut self) -> Result<(), ParseError> {
        self.token_type = TokenType::Null;
        self.token.clear();
        loop {
            if self.cur() == Some('#') {
                while !matches!(self.cur(), Some('\n') | None) {
                    self.index += 1;
                }
            }
            if is_whitespace(self.cur(), self.nesting_level) {
                self.index += 1;
            } else {
                break;
            }
        }
        let Some(c1) = self.cur() else {
            self.token_type = TokenType::Delimiter;
            return Ok(());
        };
        if c1 == '\n' && self.nesting_level == 0 {
            self.token_type = TokenType::Delimiter;
            self.take();
            return Ok(());
        }
        let c3 = self.peek_str(3);
        if c3.chars().count() == 3 && DELIMITERS.contains(&c3.as_str()) {
            self.token_type = TokenType::Delimiter;
            for _ in 0..3 {
                self.take();
            }
            return Ok(());
        }
        let c2 = self.peek_str(2);
        if c2.chars().count() == 2
            && DELIMITERS.contains(&c2.as_str())
            && (c2 != "?." || !is_digit(self.at(self.index + 2)))
        {
            self.token_type = TokenType::Delimiter;
            self.take();
            self.take();
            return Ok(());
        }
        if DELIMITERS.contains(&c1.to_string().as_str()) {
            self.token_type = TokenType::Delimiter;
            self.take();
            return Ok(());
        }
        if c1.is_ascii_digit() || c1 == '.' {
            return self.number();
        }
        if is_alpha(Some(c1)) {
            while is_alpha(self.cur()) || is_digit(self.cur()) {
                self.take();
            }
            self.token_type = if NAMED_DELIMITERS.contains(&self.token.as_str()) {
                TokenType::Delimiter
            } else {
                TokenType::Symbol
            };
            return Ok(());
        }
        self.token_type = TokenType::Unknown;
        while self.cur().is_some() {
            self.take();
        }
        Err(self.error(&format!("Syntax error in part \"{}\"", self.token)))
    }

    fn number(&mut self) -> Result<(), ParseError> {
        self.token_type = TokenType::Number;
        let prefix = self.peek_str(2);
        if prefix == "0b" || prefix == "0o" || prefix == "0x" {
            self.take();
            self.take();
            while is_alpha(self.cur()) || is_digit(self.cur()) {
                self.take();
            }
            if self.cur() == Some('.') {
                self.take();
                while is_alpha(self.cur()) || is_digit(self.cur()) {
                    self.take();
                }
            } else if self.cur() == Some('i') {
                self.take();
                while is_digit(self.cur()) {
                    self.take();
                }
            }
            return Ok(());
        }
        if self.cur() == Some('.') {
            self.take();
            if !is_digit(self.cur()) {
                // just a dot (dot notation)
                self.token_type = TokenType::Delimiter;
                return Ok(());
            }
        } else {
            while is_digit(self.cur()) {
                self.take();
            }
            if is_decimal_mark(self.cur(), self.next_char()) {
                self.take();
            }
        }
        while is_digit(self.cur()) {
            self.take();
        }
        if matches!(self.cur(), Some('e' | 'E')) {
            if is_digit(self.next_char()) || matches!(self.next_char(), Some('-' | '+')) {
                self.take();
                if matches!(self.cur(), Some('+' | '-')) {
                    self.take();
                }
                if !is_digit(self.cur()) {
                    return Err(self.error(&format!(
                        "Digit expected, got \"{}\"",
                        self.cur().unwrap_or_default()
                    )));
                }
                while is_digit(self.cur()) {
                    self.take();
                }
                if is_decimal_mark(self.cur(), self.next_char()) {
                    return Err(self.error(&format!(
                        "Digit expected, got \"{}\"",
                        self.cur().unwrap_or_default()
                    )));
                }
            } else if is_decimal_mark(self.next_char(), self.at(self.index + 2)) {
                self.index += 1;
                return Err(self.error(&format!(
                    "Digit expected, got \"{}\"",
                    self.cur().unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    /// The next token, skipping newlines.
    pub fn next_skip_newline(&mut self) -> Result<(), ParseError> {
        loop {
            self.advance()?;
            if self.token != "\n" {
                return Ok(());
            }
        }
    }

    /// Reads a string literal's body after its opening quote (mathjs `parseStringToken`).
    pub fn string_body(&mut self, quote: char) -> Result<String, ParseError> {
        let mut out = String::new();
        while let Some(c) = self.cur() {
            if c == quote {
                break;
            }
            if c == '\\' {
                self.index += 1;
                let Some(e) = self.cur() else { break };
                match e {
                    'u' => {
                        let hex: String = self.chars.iter().skip(self.index + 1).take(4).collect();
                        let code = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
                        match code {
                            Some(ch) if hex.len() == 4 => {
                                out.push(ch);
                                self.index += 4;
                            }
                            _ => {
                                return Err(
                                    self.error(&format!("Invalid unicode character \\u{hex}"))
                                );
                            }
                        }
                    }
                    '"' | '\'' | '\\' | '/' => out.push(e),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    other => return Err(self.error(&format!("Bad escape character \\{other}"))),
                }
            } else {
                out.push(c);
            }
            self.index += 1;
        }
        if self.cur() != Some(quote) {
            return Err(self.error(&format!("End of string {quote} expected")));
        }
        self.index += 1;
        self.advance()?;
        Ok(out)
    }
}
