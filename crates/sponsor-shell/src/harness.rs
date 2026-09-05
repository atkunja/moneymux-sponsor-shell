//! Optional, local-only harness activity. These hints are never visibility or billing evidence.
use serde::{Deserialize, Serialize};

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
}
