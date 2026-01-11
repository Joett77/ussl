//! USSP Command Parser

use crate::command::{Command, CommandKind};
use crate::error::{ProtocolError, ProtocolResult};
use ussl_core::{Strategy, Value};
use bytes::BytesMut;

/// Maximum message size (1MB)
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// USSP Protocol Parser
pub struct Parser {
    buffer: BytesMut,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Add data to the parser buffer
    pub fn feed(&mut self, data: &[u8]) -> ProtocolResult<()> {
        if self.buffer.len() + data.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: self.buffer.len() + data.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    /// Try to parse a complete command from the buffer
    pub fn parse(&mut self) -> ProtocolResult<Option<Command>> {
        // Find line ending
        let line_end = match self.buffer.iter().position(|&b| b == b'\n') {
            Some(pos) => pos,
            None => return Ok(None), // Incomplete
        };

        // Extract line (excluding \r\n or \n)
        let line_len = if line_end > 0 && self.buffer[line_end - 1] == b'\r' {
            line_end - 1
        } else {
            line_end
        };

        let line = String::from_utf8_lossy(&self.buffer[..line_len]).to_string();

        // Remove the parsed line from buffer
        let _ = self.buffer.split_to(line_end + 1);

        // Parse the command
        Self::parse_line(&line).map(Some)
    }

    /// Parse a single command line
    fn parse_line(line: &str) -> ProtocolResult<Command> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ProtocolError::InvalidCommand("Empty command".into()));
        }

        let mut tokens = Tokenizer::new(line);
        let cmd = tokens.next()
            .ok_or_else(|| ProtocolError::InvalidCommand("Empty command".into()))?
            .to_uppercase();

        match cmd.as_str() {
            "AUTH" => Self::parse_auth(&mut tokens),
            "CREATE" => Self::parse_create(&mut tokens),
            "GET" => Self::parse_get(&mut tokens),
            "SET" => Self::parse_set(&mut tokens),
            "DEL" | "DELETE" => Self::parse_delete(&mut tokens),
            "SUB" | "SUBSCRIBE" => Self::parse_subscribe(&mut tokens),
            "UNSUB" | "UNSUBSCRIBE" => Self::parse_unsubscribe(&mut tokens),
            "PUSH" => Self::parse_push(&mut tokens),
            "INC" | "INCR" | "INCREMENT" => Self::parse_increment(&mut tokens),
            "PRESENCE" => Self::parse_presence(&mut tokens),
            "PING" => Ok(Command::ping()),
            "QUIT" => Ok(Command::quit()),
            "INFO" => Ok(Command::info()),
            "KEYS" => Self::parse_keys(&mut tokens),
            _ => Err(ProtocolError::InvalidCommand(format!("Unknown command: {}", cmd))),
        }
    }

    fn parse_auth(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let password = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("password".into()))?;

        Ok(Command::auth(password.to_string()))
    }

    fn parse_create(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;

        let mut strategy = Strategy::default();
        let mut ttl = None;

        while let Some(opt) = tokens.next() {
            match opt.to_uppercase().as_str() {
                "STRATEGY" => {
                    let s = tokens.next()
                        .ok_or_else(|| ProtocolError::MissingArgument("strategy value".into()))?;
                    strategy = s.parse()
                        .map_err(|_| ProtocolError::InvalidArgument(format!("Invalid strategy: {}", s)))?;
                }
                "TTL" => {
                    let t = tokens.next()
                        .ok_or_else(|| ProtocolError::MissingArgument("ttl value".into()))?;
                    ttl = Some(t.parse()
                        .map_err(|_| ProtocolError::InvalidArgument(format!("Invalid TTL: {}", t)))?);
                }
                _ => return Err(ProtocolError::InvalidArgument(format!("Unknown option: {}", opt))),
            }
        }

        Ok(Command::create(id.to_string(), strategy, ttl))
    }

    fn parse_get(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;

        let mut path = None;

        while let Some(opt) = tokens.next() {
            match opt.to_uppercase().as_str() {
                "PATH" => {
                    path = Some(tokens.next()
                        .ok_or_else(|| ProtocolError::MissingArgument("path value".into()))?
                        .to_string());
                }
                _ => {
                    // Treat as path directly
                    path = Some(opt.to_string());
                }
            }
        }

        Ok(Command::get(id.to_string(), path))
    }

    fn parse_set(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;
        let path = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("path".into()))?;
        let value_str = tokens.rest()
            .ok_or_else(|| ProtocolError::MissingArgument("value".into()))?;

        let value = parse_value(&value_str)?;

        Ok(Command::set(id.to_string(), path.to_string(), value))
    }

    fn parse_delete(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;

        let mut path = None;

        if let Some(opt) = tokens.next() {
            match opt.to_uppercase().as_str() {
                "PATH" => {
                    path = Some(tokens.next()
                        .ok_or_else(|| ProtocolError::MissingArgument("path value".into()))?
                        .to_string());
                }
                _ => {
                    path = Some(opt.to_string());
                }
            }
        }

        Ok(Command::delete(id.to_string(), path))
    }

    fn parse_subscribe(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let pattern = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("pattern".into()))?;

        let mut path = None;

        while let Some(opt) = tokens.next() {
            match opt.to_uppercase().as_str() {
                "PATH" => {
                    path = Some(tokens.next()
                        .ok_or_else(|| ProtocolError::MissingArgument("path value".into()))?
                        .to_string());
                }
                _ => return Err(ProtocolError::InvalidArgument(format!("Unknown option: {}", opt))),
            }
        }

        Ok(Command::subscribe(pattern.to_string(), path))
    }

    fn parse_unsubscribe(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let pattern = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("pattern".into()))?;

        Ok(Command::unsubscribe(pattern.to_string()))
    }

    fn parse_push(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;
        let path = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("path".into()))?;
        let value_str = tokens.rest()
            .ok_or_else(|| ProtocolError::MissingArgument("value".into()))?;

        let value = parse_value(&value_str)?;

        Ok(Command::push(id.to_string(), path.to_string(), value))
    }

    fn parse_increment(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;
        let path = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("path".into()))?;
        let delta_str = tokens.next().unwrap_or("1");
        let delta: i64 = delta_str.parse()
            .map_err(|_| ProtocolError::InvalidArgument(format!("Invalid delta: {}", delta_str)))?;

        Ok(Command::increment(id.to_string(), path.to_string(), delta))
    }

    fn parse_presence(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let id = tokens.next()
            .ok_or_else(|| ProtocolError::MissingArgument("document_id".into()))?;

        let mut data = None;

        while let Some(opt) = tokens.next() {
            match opt.to_uppercase().as_str() {
                "DATA" => {
                    let json_str = tokens.rest()
                        .ok_or_else(|| ProtocolError::MissingArgument("data value".into()))?;
                    data = Some(serde_json::from_str(&json_str)
                        .map_err(|e| ProtocolError::InvalidJson(e.to_string()))?);
                }
                _ => {
                    // Treat as JSON directly
                    let json_str = format!("{} {}", opt, tokens.rest().unwrap_or_default());
                    data = Some(serde_json::from_str(json_str.trim())
                        .map_err(|e| ProtocolError::InvalidJson(e.to_string()))?);
                    break;
                }
            }
        }

        Ok(Command::presence(id.to_string(), data))
    }

    fn parse_keys(tokens: &mut Tokenizer) -> ProtocolResult<Command> {
        let pattern = tokens.next().map(|s| s.to_string());
        Ok(Command::keys(pattern))
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple tokenizer that handles quoted strings
struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        // Skip whitespace
        while self.pos < self.input.len() && self.input[self.pos..].starts_with(' ') {
            self.pos += 1;
        }

        if self.pos >= self.input.len() {
            return None;
        }

        let remaining = &self.input[self.pos..];

        // Handle quoted string
        if remaining.starts_with('"') {
            if let Some(end) = remaining[1..].find('"') {
                let token = &remaining[1..end + 1];
                self.pos += end + 2;
                return Some(token);
            }
        }

        // Handle regular token
        let end = remaining.find(' ').unwrap_or(remaining.len());
        let token = &remaining[..end];
        self.pos += end;

        Some(token)
    }

    fn rest(&mut self) -> Option<String> {
        // Skip whitespace
        while self.pos < self.input.len() && self.input[self.pos..].starts_with(' ') {
            self.pos += 1;
        }

        if self.pos >= self.input.len() {
            return None;
        }

        let remaining = self.input[self.pos..].to_string();
        self.pos = self.input.len();
        Some(remaining)
    }
}

/// Parse a value from string (JSON-like)
fn parse_value(s: &str) -> ProtocolResult<Value> {
    let s = s.trim();

    // Try parsing as JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(s) {
        return json_to_value(json);
    }

    // Fall back to string
    Ok(Value::String(s.to_string()))
}

fn json_to_value(json: serde_json::Value) -> ProtocolResult<Value> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Number(ussl_core::crdt::Number::Integer(i)))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(ussl_core::crdt::Number::Float(f)))
            } else {
                Err(ProtocolError::InvalidArgument("Invalid number".into()))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s)),
        serde_json::Value::Array(arr) => {
            let values: ProtocolResult<Vec<Value>> = arr.into_iter().map(json_to_value).collect();
            Ok(Value::Array(values?))
        }
        serde_json::Value::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_value(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create() {
        let mut parser = Parser::new();
        parser.feed(b"CREATE user:123 STRATEGY lww\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Create { strategy: Strategy::Lww, ttl: None }));
        assert_eq!(cmd.document_id, Some("user:123".to_string()));
    }

    #[test]
    fn test_parse_get() {
        let mut parser = Parser::new();
        parser.feed(b"GET user:123 PATH name\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Get { path: Some(ref p) } if p == "name"));
    }

    #[test]
    fn test_parse_set() {
        let mut parser = Parser::new();
        parser.feed(b"SET user:123 name \"Alice\"\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Set { ref path, .. } if path == "name"));
    }

    #[test]
    fn test_parse_set_json() {
        let mut parser = Parser::new();
        parser.feed(b"SET user:123 data {\"age\": 30}\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Set { .. }));
    }

    #[test]
    fn test_parse_increment() {
        let mut parser = Parser::new();
        parser.feed(b"INC counter:views count 1\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Increment { delta: 1, .. }));
    }

    #[test]
    fn test_parse_ping() {
        let mut parser = Parser::new();
        parser.feed(b"PING\r\n").unwrap();

        let cmd = parser.parse().unwrap().unwrap();
        assert!(matches!(cmd.kind, CommandKind::Ping));
    }

    #[test]
    fn test_incomplete_command() {
        let mut parser = Parser::new();
        parser.feed(b"GET user:123").unwrap();

        assert!(parser.parse().unwrap().is_none());

        parser.feed(b"\r\n").unwrap();
        assert!(parser.parse().unwrap().is_some());
    }
}
