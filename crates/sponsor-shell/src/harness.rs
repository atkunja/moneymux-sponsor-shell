//! Optional, local-only harness activity. These hints are never visibility or billing evidence.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

/// Hook failures must not change permissions, model context or harness exit status.
pub fn emit(harness: Harness) {
    let Some(path) = std::env::var_os(SOCKET_ENV) else {
        return;
    };
    if std::env::var(HARNESS_ENV).ok().as_deref() != Some(harness.command()) {
        return;
    }
    let Some(hint) = read_hint(harness, std::io::stdin().lock()) else {
        return;
    };
    let _ = send_hint(Path::new(&path), &hint);
}

fn send_hint(path: &Path, hint: &Hint) -> std::io::Result<()> {
    let socket = UnixDatagram::unbound()?;
    socket.set_nonblocking(true)?;
    socket.connect(path)?;
    let bytes = serde_json::to_vec(hint).map_err(std::io::Error::other)?;
    socket.send(&bytes)?;
    Ok(())
}

const HINT_LEASE_MS: u64 = 10_000;

pub struct Activity {
    harness: Harness,
    socket: Option<UnixDatagram>,
    latest: Option<Hint>,
}

impl Activity {
    pub fn from_env() -> Option<Self> {
        let harness = Harness::parse(&std::env::var(HARNESS_ENV).ok()?)?;
        let socket =
            std::env::var_os(SOCKET_ENV).and_then(|path| Self::bind(Path::new(&path)).ok());
        Some(Self {
            harness,
            socket,
            latest: None,
        })
    }

    fn bind(path: &Path) -> std::io::Result<UnixDatagram> {
        let socket = UnixDatagram::bind(path)?;
        socket.set_nonblocking(true)?;
        Ok(socket)
    }

    pub fn poll(&mut self) {
        let Some(socket) = &self.socket else { return };
        let Some(now) = now_ms() else {
            self.latest = None;
            return;
        };
        // Bound every frame's work even if a local process floods the socket.
        for _ in 0..64 {
            let mut bytes = [0_u8; 256];
            let Ok(size) = socket.recv(&mut bytes) else {
                break;
            };
            let Ok(hint) = serde_json::from_slice::<Hint>(&bytes[..size]) else {
                continue;
            };
            if hint.harness == self.harness
                && valid_hint(&hint, now)
                && self
                    .latest
                    .as_ref()
                    .is_none_or(|last| hint.sent_at_ms >= last.sent_at_ms)
            {
                self.latest = Some(hint);
            }
        }
    }

    pub fn label(&self) -> String {
        self.label_at(now_ms().unwrap_or(0))
    }

    fn label_at(&self, now: u64) -> String {
        let recent = self
            .latest
            .as_ref()
            .filter(|hint| valid_hint(hint, now))
            .and_then(|hint| event_label(&hint.event));
        match recent {
            Some(label) => format!("{} | recent hook: {label}", self.harness.label()),
            None => format!("{} | activity unavailable", self.harness.label()),
        }
    }
}

fn valid_hint(hint: &Hint, now: u64) -> bool {
    events(hint.harness).contains(&hint.event.as_str())
        && now
            .checked_sub(hint.sent_at_ms)
            .is_some_and(|age| age < HINT_LEASE_MS)
}

/// Print-only configuration: never overwrite existing hooks or bypass their trust review.
pub fn hook_configuration(harness: Harness, executable: &str) -> serde_json::Value {
    let command = crate::shell_join([
        executable.to_string(),
        "harness-event".to_string(),
        harness.command().to_string(),
    ]);
    let mut hooks = serde_json::Map::new();
    for event in events(harness) {
        hooks.insert(
            event.to_string(),
            serde_json::json!([{
                "hooks": [{"type": "command", "command": command, "timeout": 1}]
            }]),
        );
    }
    serde_json::json!({"hooks": hooks})
}

/// Only the wrapper owns this directory. No global hook log or session registry.
pub struct BridgeDirectory {
    path: PathBuf,
}

impl BridgeDirectory {
    pub fn create() -> std::io::Result<Self> {
        Self::create_under(&std::env::temp_dir())
    }

    fn create_under(parent: &Path) -> std::io::Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        for attempt in 0..32 {
            let path = parent.join(format!("mmh-{}-{nonce:x}-{attempt:x}", std::process::id()));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "hook directory collision",
        ))
    }

    pub fn socket_path(&self) -> PathBuf {
        self.path.join("events")
    }
}

impl Drop for BridgeDirectory {
    fn drop(&mut self) {
        // Remove only our known socket and the empty directory, never recursively.
        let _ = fs::remove_file(self.socket_path());
        let _ = fs::remove_dir(&self.path);
    }
}

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
    sent_at_ms: u64,
}

fn read_hint(harness: Harness, reader: impl Read) -> Option<Hint> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return None;
    }
    let input: HookInput = serde_json::from_slice(&bytes).ok()?;
    if !events(harness).contains(&input.hook_event_name.as_str()) {
        return None;
    }
    Some(Hint {
        harness,
        event: input.hook_event_name,
        sent_at_ms: now_ms()?,
    })
}

pub const HARNESS_ENV: &str = "SPONSOR_SHELL_HARNESS";
pub const SOCKET_ENV: &str = "SPONSOR_SHELL_HOOK_SOCKET";

pub fn environment(harness: Option<Harness>, socket: Option<&Path>) -> Vec<String> {
    // Do not inherit an unrelated wrapper's bridge from a long-lived tmux server.
    let mut args = vec![
        "env".into(),
        "-u".into(),
        HARNESS_ENV.into(),
        "-u".into(),
        SOCKET_ENV.into(),
    ];
    if let (Some(harness), Some(socket)) = (harness, socket) {
        args.push(format!("{HARNESS_ENV}={}", harness.command()));
        args.push(format!("{SOCKET_ENV}={}", socket.to_string_lossy()));
    }
    args
}

const COMMON_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
    "SessionEnd",
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
        let value = serde_json::to_value(&hint).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["harness"], "claude");
        assert_eq!(value["event"], "UserPromptSubmit");
        assert!(value["sent_at_ms"].is_u64());
        for input in [r#"{"hook_event_name":"Forged"}"#, "invalid", "{}", "[]"] {
            assert!(read_hint(Harness::Claude, input.as_bytes()).is_none());
        }
        assert!(read_hint(Harness::Claude, &vec![b' '; 65537][..]).is_none());
        assert!(read_hint(
            Harness::Claude,
            br#"{"hook_event_name":"Interrupt"}"#.as_slice()
        )
        .is_none());
    }
}
