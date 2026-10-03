//! A small parser for Valve's text KeyValues format ("VDF"), as used by
//! `libraryfolders.vdf` and `appmanifest_*.acf`.
//!
//! The format is tiny: quoted or bare keys, quoted or bare string values, and
//! `{ }` blocks; `//` starts a comment. Everything here is a string. We parse
//! into a tree and look values up by path; nothing is written back.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Obj(Obj),
}

/// An object keeps insertion order and allows duplicate keys (Valve does).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Obj(pub Vec<(String, Value)>);

impl Obj {
    /// First value stored under `key`, compared case-insensitively.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s.as_str()),
            Value::Obj(_) => None,
        }
    }

    pub fn obj(&self, key: &str) -> Option<&Obj> {
        match self.get(key)? {
            Value::Obj(o) => Some(o),
            Value::Str(_) => None,
        }
    }

    /// Every child object, in file order, as `(key, object)`.
    pub fn objects(&self) -> impl Iterator<Item = (&str, &Obj)> {
        self.0.iter().filter_map(|(k, v)| match v {
            Value::Obj(o) => Some((k.as_str(), o)),
            Value::Str(_) => None,
        })
    }

    /// All string children as a map, for the common flat case.
    pub fn strings(&self) -> BTreeMap<String, String> {
        self.0
            .iter()
            .filter_map(|(k, v)| match v {
                Value::Str(s) => Some((k.clone(), s.clone())),
                Value::Obj(_) => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Parse a whole document. The top level is an object whose keys are the
/// document's root keys (usually exactly one, e.g. `"AppState"`).
pub fn parse(text: &str) -> Result<Obj, ParseError> {
    let mut lexer = Lexer::new(text);
    let obj = parse_body(&mut lexer, true)?;
    Ok(obj)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Str(String),
    Open,
    Close,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
            line: 1,
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            message: message.into(),
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        loop {
            let Some(c) = self.chars.next() else {
                return Ok(None);
            };
            match c {
                '\n' => self.line += 1,
                c if c.is_whitespace() => {}
                '/' if self.chars.peek() == Some(&'/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            self.line += 1;
                            break;
                        }
                    }
                }
                '{' => return Ok(Some(Token::Open)),
                '}' => return Ok(Some(Token::Close)),
                '"' => return self.quoted().map(Some),
                c => return Ok(Some(Token::Str(self.bare(c)))),
            }
        }
    }

    fn quoted(&mut self) -> Result<Token, ParseError> {
        let mut out = String::new();
        loop {
            match self.chars.next() {
                None => return Err(self.error("unterminated string")),
                Some('"') => return Ok(Token::Str(out)),
                Some('\\') => match self.chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(c) => out.push(c),
                    None => return Err(self.error("unterminated escape")),
                },
                Some('\n') => {
                    self.line += 1;
                    out.push('\n');
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn bare(&mut self, first: char) -> String {
        let mut out = String::from(first);
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                break;
            }
            out.push(c);
            self.chars.next();
        }
        out
    }
}

fn parse_body(lexer: &mut Lexer<'_>, top_level: bool) -> Result<Obj, ParseError> {
    let mut obj = Obj::default();
    loop {
        let key = match lexer.next_token()? {
            None if top_level => return Ok(obj),
            None => return Err(lexer.error("unexpected end of file, expected '}'")),
            Some(Token::Close) if !top_level => return Ok(obj),
            Some(Token::Close) => return Err(lexer.error("unexpected '}'")),
            Some(Token::Open) => return Err(lexer.error("unexpected '{', expected a key")),
            Some(Token::Str(k)) => k,
        };
        let value = match lexer.next_token()? {
            Some(Token::Str(s)) => Value::Str(s),
            Some(Token::Open) => Value::Obj(parse_body(lexer, false)?),
            Some(Token::Close) => return Err(lexer.error(format!("key {key:?} has no value"))),
            None => return Err(lexer.error(format!("key {key:?} has no value"))),
        };
        obj.0.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_objects_and_strings() {
        let text = r#"
            "AppState"
            {
                "appid"     "1091500"
                "name"      "Cyberpunk 2077"
                "StateFlags" "4"
                "UserConfig"
                {
                    "language" "english"
                }
            }
        "#;
        let doc = parse(text).unwrap();
        let app = doc.obj("appstate").unwrap();
        assert_eq!(app.str("appid"), Some("1091500"));
        assert_eq!(app.str("name"), Some("Cyberpunk 2077"));
        assert_eq!(
            app.obj("UserConfig").unwrap().str("language"),
            Some("english")
        );
        assert_eq!(app.str("UserConfig"), None);
    }

    #[test]
    fn handles_comments_escapes_and_bare_tokens() {
        let text = "// header\nroot { key \"va\\\"lue\" other bare }";
        let doc = parse(text).unwrap();
        let root = doc.obj("root").unwrap();
        assert_eq!(root.str("key"), Some("va\"lue"));
        assert_eq!(root.str("other"), Some("bare"));
    }

    #[test]
    fn keeps_duplicate_keys_and_order() {
        let doc = parse(r#""r" { "a" "1" "a" "2" "b" "3" }"#).unwrap();
        let r = doc.obj("r").unwrap();
        assert_eq!(r.str("a"), Some("1"));
        assert_eq!(r.0.len(), 3);
        let keys: Vec<_> = r.strings().into_keys().collect();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let err = parse("\"a\"\n{\n\"b\"").unwrap_err();
        assert_eq!(err.line, 3);
        let err = parse("\"a\" { \"b\" \"unterminated").unwrap_err();
        assert!(err.message.contains("unterminated"));
        assert!(parse("}").is_err());
        assert!(parse("{").is_err());
    }

    #[test]
    fn empty_document_is_an_empty_object() {
        assert_eq!(parse("").unwrap(), Obj::default());
        assert_eq!(parse("  // nothing\n").unwrap(), Obj::default());
    }
}
