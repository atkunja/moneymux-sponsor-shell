//! Optional, local-only harness activity. These hints are never visibility or billing evidence.
use serde::{Deserialize, Serialize};
use std::io::Read;

const MAX_INPUT_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
struct HookInput {
    hook_event_name: String,
    // All other fields (including prompts, transcripts, paths and tool input)
    // are discarded by serde; none are retained in the local message.
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Hint {
    harness: Harness,
    event: String,
}

fn read_hint(harness: Harness, reader: impl Read) -> Option<Hint> {
    let mut bytes = Vec::new();
    reader.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return None;
    }
    let input: HookInput = serde_json::from_slice(&bytes).ok()?;
    if !events(harness).contains(&input.hook_event_name.as_str()) {
        return None;
    }
    Some(Hint { harness, event: input.hook_event_name })
}

pub const HARNESS_ENV: &str = "SPONSOR_SHELL_HARNESS";
pub const SOCKET_ENV: &str = "SPONSOR_SHELL_HOOK_SOCKET";

const COMMON_EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest",
    "PostToolUse", "Stop", "SessionEnd",
];

pub fn events(harness: Harness) -> Vec<&'static str> {
    let mut names = COMMON_EVENTS.to_vec();
    match harness {
        Harness::Claude => names.extend(["Notification", "PostToolUseFailure"]),
        Harness::Codex => names.push("Interrupt"),
    }
    names
}

fn event_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "SessionStart" => "session started",
        "UserPromptSubmit" => "prompt submitted",
        "PreToolUse" => "tool requested",
        "PermissionRequest" => "permission requested",
        "PostToolUse" => "tool finished",
        "PostToolUseFailure" => "tool failed",
        "Notification" => "notification",
        "Stop" => "turn stopped",
        "Interrupt" => "turn interrupted",
        "SessionEnd" => "session ended",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_supported_harness_names_are_accepted() {
        assert_eq!(Harness::parse("claude"), Some(Harness::Claude));
        assert_eq!(Harness::parse("codex"), Some(Harness::Codex));
        for name in ["", "CODEX", "claude --yes", "../codex", "bash"] {
            assert_eq!(Harness::parse(name), None);
        }
    }

    #[test]
    fn hooks_discard_sensitive_fields_and_reject_unknown_events() {
        let input = br#"{"hook_event_name":"UserPromptSubmit","prompt":"secret","cwd":"/private","tool_input":{"password":"private"},"transcript_path":"/secret"}"#;
        let hint = read_hint(Harness::Claude, &input[..]).unwrap();
        assert_eq!(serde_json::to_string(&hint).unwrap(),
            r#"{"harness":"claude","event":"UserPromptSubmit"}"#);
        for input in [r#"{"hook_event_name":"Forged"}"#, "invalid", "{}", "[]"] {
            assert!(read_hint(Harness::Claude, input.as_bytes()).is_none());
        }
        assert!(read_hint(Harness::Claude, &vec![b' '; 65537][..]).is_none());
        assert!(read_hint(Harness::Claude, br#"{"hook_event_name":"Interrupt"}"#.as_slice()).is_none());
    }
}
