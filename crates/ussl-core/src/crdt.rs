//! CRDT strategies and value types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Conflict resolution strategy for a document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    /// Last-Writer-Wins based on timestamp
    Lww,
    /// Convergent counter operations
    CrdtCounter,
    /// Add/Remove set operations
    CrdtSet,
    /// Nested map with LWW per key
    CrdtMap,
    /// Collaborative text editing (Y.Text)
    CrdtText,
}

impl Default for Strategy {
    fn default() -> Self {
        Self::Lww
    }
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Strategy::Lww => write!(f, "lww"),
            Strategy::CrdtCounter => write!(f, "crdt-counter"),
            Strategy::CrdtSet => write!(f, "crdt-set"),
            Strategy::CrdtMap => write!(f, "crdt-map"),
            Strategy::CrdtText => write!(f, "crdt-text"),
        }
    }
}

impl std::str::FromStr for Strategy {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "lww" => Ok(Strategy::Lww),
            "crdt-counter" | "counter" => Ok(Strategy::CrdtCounter),
            "crdt-set" | "set" => Ok(Strategy::CrdtSet),
            "crdt-map" | "map" => Ok(Strategy::CrdtMap),
            "crdt-text" | "text" => Ok(Strategy::CrdtText),
            _ => Err(crate::Error::InvalidPath(format!("Unknown strategy: {}", s))),
        }
    }
}

/// A value that can be stored in a document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(Number::Integer(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(Number::Float(n)) => Some(*n),
            Value::Number(Number::Integer(n)) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get a value at a path (e.g., "items[0].name")
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return Some(self);
        }

        let mut current = self;
        for segment in PathSegment::parse(path) {
            match segment {
                PathSegment::Key(key) => {
                    current = current.as_object()?.get(key)?;
                }
                PathSegment::Index(idx) => {
                    current = current.as_array()?.get(idx)?;
                }
            }
        }
        Some(current)
    }

    /// Set a value at a path, creating intermediate objects/arrays as needed
    pub fn set_path(&mut self, path: &str, value: Value) -> crate::Result<()> {
        if path.is_empty() {
            *self = value;
            return Ok(());
        }

        let segments: Vec<PathSegment> = PathSegment::parse(path).collect();
        let mut current = self;

        for (i, segment) in segments.iter().enumerate() {
            let is_last = i == segments.len() - 1;

            match segment {
                PathSegment::Key(key) => {
                    if !matches!(current, Value::Object(_)) {
                        *current = Value::Object(HashMap::new());
                    }

                    if let Value::Object(map) = current {
                        if is_last {
                            map.insert(key.to_string(), value);
                            return Ok(());
                        } else {
                            current = map.entry(key.to_string()).or_insert(Value::Null);
                        }
                    }
                }
                PathSegment::Index(idx) => {
                    if !matches!(current, Value::Array(_)) {
                        *current = Value::Array(Vec::new());
                    }

                    if let Value::Array(arr) = current {
                        while arr.len() <= *idx {
                            arr.push(Value::Null);
                        }
                        if is_last {
                            arr[*idx] = value;
                            return Ok(());
                        } else {
                            current = &mut arr[*idx];
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Number(Number::Integer(v))
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Number(Number::Float(v))
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

/// Number type supporting both integers and floats
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Integer(i64),
    Float(f64),
}

/// Path segment for navigating document structure
#[derive(Debug, Clone)]
pub enum PathSegment<'a> {
    Key(&'a str),
    Index(usize),
}

impl<'a> PathSegment<'a> {
    /// Parse a path string into segments
    /// Examples: "foo.bar", "items[0]", "users[0].name"
    pub fn parse(path: &'a str) -> impl Iterator<Item = PathSegment<'a>> {
        PathParser { path, pos: 0 }
    }
}

struct PathParser<'a> {
    path: &'a str,
    pos: usize,
}

impl<'a> Iterator for PathParser<'a> {
    type Item = PathSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.path.len() {
            return None;
        }

        let remaining = &self.path[self.pos..];

        // Skip leading dot
        let remaining = remaining.strip_prefix('.').unwrap_or(remaining);
        if remaining.is_empty() {
            return None;
        }
        self.pos = self.path.len() - remaining.len();

        // Check for array index
        if remaining.starts_with('[') {
            if let Some(end) = remaining.find(']') {
                let idx_str = &remaining[1..end];
                self.pos += end + 1;
                if let Ok(idx) = idx_str.parse::<usize>() {
                    return Some(PathSegment::Index(idx));
                }
            }
        }

        // Find next delimiter
        let end = remaining
            .find(|c| c == '.' || c == '[')
            .unwrap_or(remaining.len());

        let key = &remaining[..end];
        self.pos += end;

        if key.is_empty() {
            self.next()
        } else {
            Some(PathSegment::Key(key))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_parsing() {
        let segments: Vec<_> = PathSegment::parse("foo.bar[0].baz").collect();
        assert!(matches!(segments[0], PathSegment::Key("foo")));
        assert!(matches!(segments[1], PathSegment::Key("bar")));
        assert!(matches!(segments[2], PathSegment::Index(0)));
        assert!(matches!(segments[3], PathSegment::Key("baz")));
    }

    #[test]
    fn test_value_get_path() {
        let mut obj = HashMap::new();
        obj.insert("name".to_string(), Value::String("Alice".to_string()));
        let value = Value::Object(obj);

        assert_eq!(
            value.get_path("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_value_set_path() {
        let mut value = Value::Object(HashMap::new());
        value.set_path("user.name", Value::String("Bob".to_string())).unwrap();

        assert_eq!(
            value.get_path("user.name"),
            Some(&Value::String("Bob".to_string()))
        );
    }
}
