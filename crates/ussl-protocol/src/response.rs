//! USSP Response types

use bytes::{BufMut, BytesMut};
use ussl_core::Value;

/// A USSP response
#[derive(Debug, Clone)]
pub enum Response {
    /// +OK [message]
    Ok(Option<String>),

    /// -ERR <code> <message>
    Error { code: String, message: String },

    /// $<length>\r\n<data>
    Bulk(Vec<u8>),

    /// *<count>\r\n<items>
    Array(Vec<Response>),

    /// #<version> <delta>
    Delta { version: u64, data: Vec<u8> },

    /// :<integer>
    Integer(i64),

    /// Simple string value (for GET responses)
    Value(Value),

    /// Null response
    Null,

    /// PONG
    Pong,
}

impl Response {
    pub fn ok() -> Self {
        Response::Ok(None)
    }

    pub fn ok_with_message(msg: impl Into<String>) -> Self {
        Response::Ok(Some(msg.into()))
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Response::Error {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn not_found(id: &str) -> Self {
        Response::Error {
            code: "NOT_FOUND".into(),
            message: format!("Document not found: {}", id),
        }
    }

    pub fn invalid_command(msg: &str) -> Self {
        Response::Error {
            code: "INVALID_CMD".into(),
            message: msg.to_string(),
        }
    }

    pub fn bulk(data: impl Into<Vec<u8>>) -> Self {
        Response::Bulk(data.into())
    }

    pub fn integer(n: i64) -> Self {
        Response::Integer(n)
    }

    pub fn value(v: Value) -> Self {
        Response::Value(v)
    }

    pub fn delta(version: u64, data: Vec<u8>) -> Self {
        Response::Delta { version, data }
    }

    pub fn pong() -> Self {
        Response::Pong
    }

    pub fn null() -> Self {
        Response::Null
    }

    pub fn array(items: Vec<Response>) -> Self {
        Response::Array(items)
    }

    /// Encode the response to bytes
    pub fn encode(&self) -> BytesMut {
        let mut buf = BytesMut::new();
        self.encode_into(&mut buf);
        buf
    }

    /// Encode the response into an existing buffer
    pub fn encode_into(&self, buf: &mut BytesMut) {
        match self {
            Response::Ok(None) => {
                buf.put_slice(b"+OK\r\n");
            }
            Response::Ok(Some(msg)) => {
                buf.put_slice(b"+OK ");
                buf.put_slice(msg.as_bytes());
                buf.put_slice(b"\r\n");
            }
            Response::Error { code, message } => {
                buf.put_slice(b"-ERR ");
                buf.put_slice(code.as_bytes());
                buf.put_slice(b" ");
                buf.put_slice(message.as_bytes());
                buf.put_slice(b"\r\n");
            }
            Response::Bulk(data) => {
                buf.put_slice(b"$");
                buf.put_slice(data.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                buf.put_slice(data);
                buf.put_slice(b"\r\n");
            }
            Response::Array(items) => {
                buf.put_slice(b"*");
                buf.put_slice(items.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                for item in items {
                    item.encode_into(buf);
                }
            }
            Response::Delta { version, data } => {
                buf.put_slice(b"#");
                buf.put_slice(version.to_string().as_bytes());
                buf.put_slice(b" ");
                // Encode delta as base64 for text protocol
                let encoded = base64_encode(data);
                buf.put_slice(encoded.as_bytes());
                buf.put_slice(b"\r\n");
            }
            Response::Integer(n) => {
                buf.put_slice(b":");
                buf.put_slice(n.to_string().as_bytes());
                buf.put_slice(b"\r\n");
            }
            Response::Value(v) => {
                let json = serde_json::to_string(v).unwrap_or_else(|_| "null".to_string());
                buf.put_slice(b"$");
                buf.put_slice(json.len().to_string().as_bytes());
                buf.put_slice(b"\r\n");
                buf.put_slice(json.as_bytes());
                buf.put_slice(b"\r\n");
            }
            Response::Null => {
                buf.put_slice(b"$-1\r\n");
            }
            Response::Pong => {
                buf.put_slice(b"+PONG\r\n");
            }
        }
    }
}

/// Simple base64 encoding (no external dependency)
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        result.push(ALPHABET[(b0 >> 2) as usize] as char);
        result.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_ok() {
        let resp = Response::ok();
        assert_eq!(resp.encode().as_ref(), b"+OK\r\n");
    }

    #[test]
    fn test_encode_error() {
        let resp = Response::error("NOT_FOUND", "Document not found");
        assert_eq!(resp.encode().as_ref(), b"-ERR NOT_FOUND Document not found\r\n");
    }

    #[test]
    fn test_encode_bulk() {
        let resp = Response::bulk(b"hello".to_vec());
        assert_eq!(resp.encode().as_ref(), b"$5\r\nhello\r\n");
    }

    #[test]
    fn test_encode_integer() {
        let resp = Response::integer(42);
        assert_eq!(resp.encode().as_ref(), b":42\r\n");
    }

    #[test]
    fn test_encode_null() {
        let resp = Response::null();
        assert_eq!(resp.encode().as_ref(), b"$-1\r\n");
    }

    #[test]
    fn test_encode_array() {
        let resp = Response::array(vec![
            Response::ok(),
            Response::integer(1),
        ]);
        assert_eq!(resp.encode().as_ref(), b"*2\r\n+OK\r\n:1\r\n");
    }
}
