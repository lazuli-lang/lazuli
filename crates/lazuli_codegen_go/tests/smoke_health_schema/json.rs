//! Minimal hand-rolled JSON parser used by the smoke_health_schema
//! integration test. Kept here so the parent main.rs can stay focused
//! on the actual schema assertions; the parser API is intentionally
//! tiny — `JsonValue` plus `JsonParser::parse`.

#![cfg(feature = "smoke_e2e")]

use std::collections::BTreeMap;

#[derive(Debug)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            JsonValue::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn shape(&self) -> String {
        match self {
            JsonValue::Null => "null".to_owned(),
            JsonValue::Bool(value) => format!("bool({value})"),
            JsonValue::Number(value) => format!("number({value})"),
            JsonValue::String(value) => format!("string({value:?})"),
            JsonValue::Array(values) => {
                let inner = values
                    .iter()
                    .map(JsonValue::shape)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{inner}]")
            }
            JsonValue::Object(fields) => {
                let inner = fields
                    .iter()
                    .map(|(key, value)| format!("{key}: {}", value.shape()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{inner}}}")
            }
        }
    }
}

pub struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    pub fn parse(input: &'a str) -> Result<JsonValue, String> {
        let mut parser = Self {
            input: input.as_bytes(),
            pos: 0,
        };
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.pos != parser.input.len() {
            return Err(format!("trailing data at byte {}", parser.pos));
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(byte) => Err(format!(
                "unexpected byte {:?} at byte {}",
                byte as char, self.pos
            )),
            None => Err("unexpected end of input".to_owned()),
        }
    }

    fn parse_literal(&mut self, literal: &[u8], value: JsonValue) -> Result<JsonValue, String> {
        if self.input[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(format!(
                "expected literal {} at byte {}",
                String::from_utf8_lossy(literal),
                self.pos
            ))
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        let mut fields = BTreeMap::new();
        self.skip_ws();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(fields));
        }

        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            fields.insert(key, value);
            self.skip_ws();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }

        Ok(JsonValue::Object(fields))
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.skip_ws();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }

        loop {
            values.push(self.parse_value()?);
            self.skip_ws();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }

        Ok(JsonValue::Array(values))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = String::new();

        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(value),
                b'\\' => value.push(self.parse_escape()?),
                0x00..=0x1f => {
                    return Err(format!("unescaped control byte at byte {}", self.pos - 1));
                }
                _ => {
                    let start = self.pos - 1;
                    while let Some(next) = self.peek() {
                        if next == b'"' || next == b'\\' || next <= 0x1f {
                            break;
                        }
                        self.pos += 1;
                    }
                    let chunk = std::str::from_utf8(&self.input[start..self.pos])
                        .map_err(|err| format!("invalid UTF-8 string chunk: {err}"))?;
                    value.push_str(chunk);
                }
            }
        }

        Err("unterminated string".to_owned())
    }

    fn parse_escape(&mut self) -> Result<char, String> {
        match self.next() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{0008}'),
            Some(b'f') => Ok('\u{000c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.parse_unicode_escape(),
            Some(byte) => Err(format!(
                "invalid escape {:?} at byte {}",
                byte as char,
                self.pos - 1
            )),
            None => Err("unterminated escape".to_owned()),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let byte = self
                .next()
                .ok_or_else(|| "unterminated unicode escape".to_owned())?;
            code = code * 16
                + match byte {
                    b'0'..=b'9' => u32::from(byte - b'0'),
                    b'a'..=b'f' => u32::from(byte - b'a' + 10),
                    b'A'..=b'F' => u32::from(byte - b'A' + 10),
                    _ => {
                        return Err(format!(
                            "invalid unicode escape byte {:?} at byte {}",
                            byte as char,
                            self.pos - 1
                        ));
                    }
                };
        }
        char::from_u32(code).ok_or_else(|| format!("invalid unicode scalar U+{code:04X}"))
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        if self.consume(b'-') {
            self.require_digit()?;
        }

        if self.consume(b'0') {
            // Leading zero is the whole integer part.
        } else {
            self.require_digit()?;
            while self.consume_digit() {}
        }

        if self.consume(b'.') {
            self.require_digit()?;
            while self.consume_digit() {}
        }

        if self.consume(b'e') || self.consume(b'E') {
            let _ = self.consume(b'+') || self.consume(b'-');
            self.require_digit()?;
            while self.consume_digit() {}
        }

        let raw = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|err| format!("invalid UTF-8 number: {err}"))?;
        let value = raw
            .parse::<f64>()
            .map_err(|err| format!("invalid number {raw:?}: {err}"))?;
        Ok(JsonValue::Number(value))
    }

    fn require_digit(&mut self) -> Result<(), String> {
        if self.consume_digit() {
            Ok(())
        } else {
            Err(format!("expected digit at byte {}", self.pos))
        }
    }

    fn consume_digit(&mut self) -> bool {
        matches!(self.peek(), Some(b'0'..=b'9')) && {
            self.pos += 1;
            true
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(format!("expected {:?} at byte {}", byte as char, self.pos))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
}
