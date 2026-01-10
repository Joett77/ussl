//! USSP Command types

use ussl_core::{Strategy, Value};
use serde::{Deserialize, Serialize};

/// A parsed USSP command
#[derive(Debug, Clone)]
pub struct Command {
    pub kind: CommandKind,
    pub document_id: Option<String>,
}

/// All supported USSP commands
#[derive(Debug, Clone)]
pub enum CommandKind {
    /// CREATE <id> [STRATEGY <s>] [TTL <ms>]
    Create {
        strategy: Strategy,
        ttl: Option<u64>,
    },

    /// GET <id> [PATH <path>]
    Get {
        path: Option<String>,
    },

    /// SET <id> <path> <value>
    Set {
        path: String,
        value: Value,
    },

    /// DEL <id> [PATH <path>]
    Delete {
        path: Option<String>,
    },

    /// SUB <pattern> [PATH <path>]
    Subscribe {
        pattern: String,
        path: Option<String>,
    },

    /// UNSUB <pattern>
    Unsubscribe {
        pattern: String,
    },

    /// PUSH <id> <path> <value>
    Push {
        path: String,
        value: Value,
    },

    /// INC <id> <path> <delta>
    Increment {
        path: String,
        delta: i64,
    },

    /// PRESENCE <id> [DATA <json>]
    Presence {
        data: Option<serde_json::Value>,
    },

    /// PING
    Ping,

    /// QUIT
    Quit,

    /// INFO
    Info,

    /// KEYS [pattern]
    Keys {
        pattern: Option<String>,
    },
}

impl Command {
    pub fn create(id: String, strategy: Strategy, ttl: Option<u64>) -> Self {
        Command {
            kind: CommandKind::Create { strategy, ttl },
            document_id: Some(id),
        }
    }

    pub fn get(id: String, path: Option<String>) -> Self {
        Command {
            kind: CommandKind::Get { path },
            document_id: Some(id),
        }
    }

    pub fn set(id: String, path: String, value: Value) -> Self {
        Command {
            kind: CommandKind::Set { path, value },
            document_id: Some(id),
        }
    }

    pub fn delete(id: String, path: Option<String>) -> Self {
        Command {
            kind: CommandKind::Delete { path },
            document_id: Some(id),
        }
    }

    pub fn subscribe(pattern: String, path: Option<String>) -> Self {
        Command {
            kind: CommandKind::Subscribe { pattern: pattern.clone(), path },
            document_id: Some(pattern),
        }
    }

    pub fn unsubscribe(pattern: String) -> Self {
        Command {
            kind: CommandKind::Unsubscribe { pattern: pattern.clone() },
            document_id: Some(pattern),
        }
    }

    pub fn push(id: String, path: String, value: Value) -> Self {
        Command {
            kind: CommandKind::Push { path, value },
            document_id: Some(id),
        }
    }

    pub fn increment(id: String, path: String, delta: i64) -> Self {
        Command {
            kind: CommandKind::Increment { path, delta },
            document_id: Some(id),
        }
    }

    pub fn presence(id: String, data: Option<serde_json::Value>) -> Self {
        Command {
            kind: CommandKind::Presence { data },
            document_id: Some(id),
        }
    }

    pub fn ping() -> Self {
        Command {
            kind: CommandKind::Ping,
            document_id: None,
        }
    }

    pub fn quit() -> Self {
        Command {
            kind: CommandKind::Quit,
            document_id: None,
        }
    }

    pub fn info() -> Self {
        Command {
            kind: CommandKind::Info,
            document_id: None,
        }
    }

    pub fn keys(pattern: Option<String>) -> Self {
        Command {
            kind: CommandKind::Keys { pattern },
            document_id: None,
        }
    }
}
