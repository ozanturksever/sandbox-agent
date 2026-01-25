//! Generated types from AI coding agent JSON schemas.
//!
//! This crate provides Rust types for:
//! - OpenCode SDK
//! - Claude Code SDK
//! - Codex SDK
//! - AMP Code SDK

pub mod opencode {
    //! OpenCode SDK types extracted from OpenAPI 3.1.1 spec.
    include!(concat!(env!("OUT_DIR"), "/opencode.rs"));
}

pub mod claude {
    //! Claude Code SDK types extracted from TypeScript definitions.
    include!(concat!(env!("OUT_DIR"), "/claude.rs"));
}

pub mod codex {
    //! Codex SDK types.
    include!(concat!(env!("OUT_DIR"), "/codex.rs"));
}

pub mod amp {
    //! AMP Code SDK types.
    include!(concat!(env!("OUT_DIR"), "/amp.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_bash_input() {
        let input = claude::BashInput {
            command: "ls -la".to_string(),
            timeout: Some(5000.0),
            description: Some("List files".to_string()),
            run_in_background: None,
            simulated_sed_edit: None,
            dangerously_disable_sandbox: None,
        };

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("ls -la"));

        let parsed: claude::BashInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.command, "ls -la");
    }

    #[test]
    fn test_codex_thread_event() {
        let event = codex::ThreadEvent {
            type_: codex::ThreadEventType::ThreadCreated,
            thread_id: Some("thread-123".to_string()),
            item: None,
            error: serde_json::Map::new(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("thread.created"));
    }

    #[test]
    fn test_amp_message() {
        let msg = amp::Message {
            role: amp::MessageRole::User,
            content: "Hello".to_string(),
            tool_calls: vec![],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello"));
    }
}
