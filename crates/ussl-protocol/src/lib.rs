//! USSP - Universal State Sync Protocol
//!
//! A text-based protocol inspired by Redis RESP for state synchronization.
//!
//! ## Command Format
//! ```text
//! COMMAND <document_id> [OPTIONS] [PAYLOAD]
//! ```
//!
//! ## Response Format
//! ```text
//! +OK                      # Success
//! -ERR <code> <message>    # Error
//! $<length>\r\n<data>      # Bulk data
//! *<count>\r\n<items>      # Array
//! #<version> <delta>       # Delta update
//! ```

pub mod command;
pub mod response;
pub mod parser;
pub mod error;

pub use command::{Command, CommandKind};
pub use response::Response;
pub use parser::Parser;
pub use error::{ProtocolError, ProtocolResult};
