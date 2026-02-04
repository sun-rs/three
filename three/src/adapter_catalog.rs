use crate::config::{AdapterCatalog, AdapterConfig, FilesystemCapability, OutputParserConfig, OutputPick};
use std::collections::BTreeMap;

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

pub fn embedded_adapter_catalog() -> AdapterCatalog {
    let mut adapters = BTreeMap::new();

    adapters.insert(
        "codex".to_string(),
        AdapterConfig {
            filesystem_capabilities: Some(vec![
                FilesystemCapability::ReadOnly,
                FilesystemCapability::ReadWrite,
            ]),
            args_template: v(&[
                "exec",
                "{% if capabilities.filesystem == 'read-only' %}--sandbox{% endif %}",
                "{% if capabilities.filesystem == 'read-only' %}read-only{% endif %}",
                "{% if capabilities.filesystem == 'read-write' %}--sandbox{% endif %}",
                "{% if capabilities.filesystem == 'read-write' %}workspace-write{% endif %}",
                "{% if capabilities.filesystem == 'danger-full-access' %}--sandbox{% endif %}",
                "{% if capabilities.filesystem == 'danger-full-access' %}danger-full-access{% endif %}",
                "{% if not session_id and model != 'default' %}--model{% endif %}",
                "{% if not session_id and model != 'default' %}{{ model }}{% endif %}",
                "{% if session_id and model != 'default' %}-c{% endif %}",
                "{% if session_id and model != 'default' %}model={{ model }}{% endif %}",
                "{% if options.model_reasoning_effort %}-c{% endif %}",
                "{% if options.model_reasoning_effort %}model_reasoning_effort={{ options.model_reasoning_effort }}{% endif %}",
                "{% if options.text_verbosity %}-c{% endif %}",
                "{% if options.text_verbosity %}text_verbosity={{ options.text_verbosity }}{% endif %}",
                "--skip-git-repo-check",
                "{% if not session_id %}-C{% endif %}",
                "{% if not session_id %}{{ workdir }}{% endif %}",
                "--json",
                "{% if session_id %}resume{% endif %}",
                "{% if session_id %}{{ session_id }}{% endif %}",
                "{% if prompt %}{{ prompt }}{% endif %}",
            ]),
            output_parser: OutputParserConfig::JsonStream {
                session_id_path: "thread_id".to_string(),
                message_path: "item.text".to_string(),
                pick: Some(OutputPick::Last),
            },
        },
    );

    adapters.insert(
        "claude".to_string(),
        AdapterConfig {
            filesystem_capabilities: Some(vec![
                FilesystemCapability::ReadOnly,
                FilesystemCapability::ReadWrite,
            ]),
            args_template: v(&[
                "--print",
                "{{ prompt }}",
                "--output-format",
                "json",
                "{% if model != 'default' %}--model{% endif %}",
                "{% if model != 'default' %}{{ model }}{% endif %}",
                "{% if capabilities.filesystem == 'read-write' %}--dangerously-skip-permissions{% endif %}",
                "{% if capabilities.filesystem == 'read-only' %}--permission-mode{% endif %}",
                "{% if capabilities.filesystem == 'read-only' %}plan{% endif %}",
                "{% if session_id %}--resume{% endif %}",
                "{% if session_id %}{{ session_id }}{% endif %}",
            ]),
            output_parser: OutputParserConfig::JsonObject {
                session_id_path: Some("session_id".to_string()),
                message_path: "result".to_string(),
            },
        },
    );

    adapters.insert(
        "gemini".to_string(),
        AdapterConfig {
            filesystem_capabilities: Some(vec![
                FilesystemCapability::ReadOnly,
                FilesystemCapability::ReadWrite,
            ]),
            args_template: v(&[
                "--output-format",
                "json",
                "{% if capabilities.filesystem == 'read-only' %}--approval-mode{% endif %}",
                "{% if capabilities.filesystem == 'read-only' %}plan{% endif %}",
                "{% if capabilities.filesystem != 'read-only' %}-y{% endif %}",
                "{% if model != 'default' %}-m{% endif %}",
                "{% if model != 'default' %}{{ model }}{% endif %}",
                "{% if capabilities.filesystem == 'read-only' %}--sandbox{% endif %}",
                "{% if include_directories %}--include-directories{% endif %}",
                "{{ include_directories }}",
                "{% if session_id %}--resume{% endif %}",
                "{% if session_id %}{{ session_id }}{% endif %}",
                "--prompt",
                "{{ prompt }}",
            ]),
            output_parser: OutputParserConfig::JsonObject {
                session_id_path: Some("session_id".to_string()),
                message_path: "response".to_string(),
            },
        },
    );

    adapters.insert(
        "opencode".to_string(),
        AdapterConfig {
            filesystem_capabilities: Some(vec![FilesystemCapability::ReadWrite]),
            args_template: v(&[
                "run",
                "{% if model != 'default' %}-m{% endif %}",
                "{% if model != 'default' %}{{ model }}{% endif %}",
                "{% if session_id %}-s{% endif %}",
                "{% if session_id %}{{ session_id }}{% endif %}",
                "--format",
                "json",
                "{{ prompt }}",
            ]),
            output_parser: OutputParserConfig::JsonStream {
                session_id_path: "part.sessionID".to_string(),
                message_path: "part.text".to_string(),
                pick: Some(OutputPick::Last),
            },
        },
    );

    adapters.insert(
        "kimi".to_string(),
        AdapterConfig {
            filesystem_capabilities: Some(vec![FilesystemCapability::ReadWrite]),
            args_template: v(&[
                "--print",
                "--thinking",
                "--output-format",
                "text",
                "--final-message-only",
                "--work-dir",
                "{{ workdir }}",
                "{% if model != 'default' %}--model{% endif %}",
                "{% if model != 'default' %}{{ model }}{% endif %}",
                "{% if session_id %}--session{% endif %}",
                "{% if session_id %}{{ session_id }}{% endif %}",
                "--prompt",
                "{% if capabilities.filesystem == 'read-only' %}{{ prompt }}\n\u{4e0d}\u{5141}\u{8bb8}\u{5199}\u{6587}\u{4ef6}{% else %}{{ prompt }}{% endif %}",
            ]),
            output_parser: OutputParserConfig::Text,
        },
    );

    AdapterCatalog { adapters }
}
