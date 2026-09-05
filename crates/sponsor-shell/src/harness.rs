//! Optional, local-only harness activity. These hints are never visibility or billing evidence.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
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
/// How long a phase may be trusted without any new event.
///
/// A turn can legitimately run for minutes with no hook in between, so this is
/// far longer than the label lease. It still expires: a harness that was killed
/// mid-turn must decay to `Unknown` rather than claim work forever.
const TURN_LEASE_MS: u64 = 15 * 60 * 1_000;

/// What the harness is doing, derived from the hook events already arriving.
///
/// The recent-hook label answers "what happened last"; it expires in ten
/// seconds because a stale event name is misleading. That makes it useless for
/// the thing a sidecar actually needs to know — whether a model turn is still
/// running — because a turn routinely lasts minutes with no intervening event.
/// So the phase is tracked separately, with its own much longer ceiling.
///
/// This remains advisory. Hooks can be missing, delayed, reordered, duplicated,
/// or emitted by a subagent sharing the wrapper environment. A phase is never
/// visibility, attention, or billing evidence, and it never zooms the ad over
/// the harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnPhase {
    /// Hooks are not configured, or nothing has been observed recently enough
    /// to describe. Reported honestly rather than guessed as idle.
    Unknown,
    /// No prompt is in flight: the session started, or the last turn ended.
    Idle,
    /// A prompt was submitted and no ending event has arrived. This is the
    /// waiting state the user sees a spinner for.
    Working,
    /// The harness is blocked on a human — a permission prompt or a
    /// notification. Deliberately distinct from `Working`: the model is not
    /// computing, and describing it as working would be a lie the user can see.
    AwaitingUser,
}

impl TurnPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "activity unavailable",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::AwaitingUser => "waiting on you",
        }
    }
}

pub struct Activity {
    harness: Harness,
    socket: Option<UnixDatagram>,
    latest: Option<Hint>,
    /// Held separately from `latest` so a long turn is not forgotten the moment
    /// its opening event stops being recent enough to name.
    phase: Option<(TurnPhase, u64)>,
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
            phase: None,
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
            self.phase = None;
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
                if let Some(phase) = event_phase(&hint.event) {
                    self.phase = Some((phase, hint.sent_at_ms));
                } else if let Some((current, _)) = self.phase {
                    // Proves the turn is still alive without deciding its
                    // phase, so refresh the lease and keep the current state.
                    self.phase = Some((current, hint.sent_at_ms));
                }
                self.latest = Some(hint);
            }
        }
    }

    pub fn phase(&self) -> TurnPhase {
        self.phase_at(now_ms().unwrap_or(0))
    }

    fn phase_at(&self, now: u64) -> TurnPhase {
        self.phase
            .filter(|(_, at)| now.checked_sub(*at).is_some_and(|age| age < TURN_LEASE_MS))
            .map_or(TurnPhase::Unknown, |(phase, _)| phase)
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
        // The phase leads because it is what the user is actually looking for:
        // whether the thing they are waiting on is still running. The recent
        // hook stays alongside it as the evidence the phase was derived from,
        // so a wrong phase is auditable rather than merely disbelieved.
        let phase = self.phase_at(now);
        match (phase, recent) {
            (TurnPhase::Unknown, None) => {
                format!("{} | activity unavailable", self.harness.label())
            }
            (TurnPhase::Unknown, Some(label)) => {
                format!("{} | recent hook: {label}", self.harness.label())
            }
            (phase, None) => format!("{} | {}", self.harness.label(), phase.label()),
            (phase, Some(label)) => format!(
                "{} | {} | recent hook: {label}",
                self.harness.label(),
                phase.label()
            ),
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

pub fn executable(harness: Harness) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let cwd = std::env::current_dir().ok()?;
    std::env::split_paths(&paths).find_map(|directory| {
        let path = cwd.join(directory).join(harness.command());
        let metadata = fs::metadata(&path).ok()?;
        (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then_some(path)
    })
}

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

/// Map an event to the phase it establishes, or `None` to leave the phase alone.
///
/// Returning `None` matters: `PreToolUse`/`PostToolUse` prove a turn is still
/// alive without themselves deciding the phase, so they must refresh the lease
/// without overwriting an `AwaitingUser` the permission prompt just set.
fn event_phase(name: &str) -> Option<TurnPhase> {
    Some(match name {
        "UserPromptSubmit" => TurnPhase::Working,
        "PermissionRequest" | "Notification" => TurnPhase::AwaitingUser,
        // A tool starting or finishing is the model acting, so it ends any
        // wait it was blocked on and resumes work.
        "PostToolUse" | "PostToolUseFailure" => TurnPhase::Working,
        "Stop" | "Interrupt" | "SessionEnd" | "SessionStart" => TurnPhase::Idle,
        // PreToolUse is deliberately absent: it fires while a permission prompt
        // may still be pending, and would otherwise erase AwaitingUser.
        _ => return None,
    })
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
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_channels_are_unique_and_cleanup_only_owned_files() {
        let first = BridgeDirectory::create().unwrap();
        let second = BridgeDirectory::create().unwrap();
        assert_ne!(first.path, second.path);
        assert_eq!(
            fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let socket = Activity::bind(&first.socket_path()).unwrap();
        let path = first.path.clone();
        drop(socket);
        drop(first);
        assert!(!path.exists());
        let foreign = second.path.join("not-ours");
        fs::write(&foreign, "preserve").unwrap();
        let path = second.path.clone();
        drop(second);
        assert_eq!(fs::read_to_string(&foreign).unwrap(), "preserve");
        fs::remove_file(foreign).unwrap();
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn independent_socket_channels_do_not_cross_sessions() {
        let first = BridgeDirectory::create().unwrap();
        let second = BridgeDirectory::create().unwrap();
        let mut activity = Activity {
            harness: Harness::Codex,
            socket: Some(Activity::bind(&first.socket_path()).unwrap()),
            latest: None,
            phase: None,
        };
        let mut other = Activity {
            harness: Harness::Codex,
            socket: Some(Activity::bind(&second.socket_path()).unwrap()),
            latest: None,
            phase: None,
        };
        let hint = read_hint(
            Harness::Codex,
            br#"{"hook_event_name":"Interrupt"}"#.as_slice(),
        )
        .unwrap();
        send_hint(&first.socket_path(), &hint).unwrap();
        activity.poll();
        other.poll();
        // The interrupt ends the turn, so the phase leads and the hook that
        // established it stays visible as the evidence.
        assert_eq!(
            activity.label(),
            "Codex | idle | recent hook: turn interrupted"
        );
        assert_eq!(other.label(), "Codex | activity unavailable");
        assert!(send_hint(&first.path.join("absent"), &hint).is_err());
    }

    fn phased(events: &[(&str, u64)]) -> Activity {
        let mut activity = Activity {
            harness: Harness::Claude,
            socket: None,
            latest: None,
            phase: None,
        };
        for (event, at) in events {
            if let Some(phase) = event_phase(event) {
                activity.phase = Some((phase, *at));
            } else if let Some((current, _)) = activity.phase {
                activity.phase = Some((current, *at));
            }
        }
        activity
    }

    #[test]
    fn a_submitted_prompt_starts_a_working_turn() {
        assert_eq!(
            phased(&[("UserPromptSubmit", 1_000)]).phase_at(1_000),
            TurnPhase::Working
        );
    }

    #[test]
    fn stopping_interrupting_or_ending_returns_to_idle() {
        for ending in ["Stop", "Interrupt", "SessionEnd"] {
            let activity = phased(&[("UserPromptSubmit", 1_000), (ending, 2_000)]);
            assert_eq!(activity.phase_at(2_000), TurnPhase::Idle, "{ending}");
        }
    }

    // The model is not computing while a human is being asked something, and
    // the user can see that on their own screen.
    #[test]
    fn a_permission_prompt_is_not_reported_as_working() {
        let activity = phased(&[("UserPromptSubmit", 1_000), ("PermissionRequest", 2_000)]);
        assert_eq!(activity.phase_at(2_000), TurnPhase::AwaitingUser);
    }

    // PreToolUse fires while the permission prompt may still be pending, so it
    // must not silently revert the wait to working.
    #[test]
    fn a_tool_request_does_not_erase_a_pending_permission_prompt() {
        let activity = phased(&[
            ("UserPromptSubmit", 1_000),
            ("PermissionRequest", 2_000),
            ("PreToolUse", 3_000),
        ]);
        assert_eq!(activity.phase_at(3_000), TurnPhase::AwaitingUser);
    }

    // ...but the tool actually running means the human answered.
    #[test]
    fn a_finished_tool_resumes_working_after_a_permission_prompt() {
        let activity = phased(&[
            ("UserPromptSubmit", 1_000),
            ("PermissionRequest", 2_000),
            ("PostToolUse", 3_000),
        ]);
        assert_eq!(activity.phase_at(3_000), TurnPhase::Working);
    }

    // A turn that outlives the ten-second label lease must still read as
    // working; that gap is the whole reason the phase is tracked separately.
    #[test]
    fn a_long_turn_outlives_the_recent_hook_label() {
        let activity = phased(&[("UserPromptSubmit", 1_000)]);
        assert_eq!(activity.phase_at(1_000 + HINT_LEASE_MS * 10), TurnPhase::Working);
    }

    // But a harness killed mid-turn must decay rather than claim work forever.
    #[test]
    fn a_stalled_turn_expires_into_unknown_at_the_boundary() {
        let activity = phased(&[("UserPromptSubmit", 1_000)]);
        assert_eq!(activity.phase_at(1_000 + TURN_LEASE_MS - 1), TurnPhase::Working);
        assert_eq!(activity.phase_at(1_000 + TURN_LEASE_MS), TurnPhase::Unknown);
    }

    #[test]
    fn no_observed_event_is_unknown_rather_than_guessed_idle() {
        let activity = phased(&[]);
        assert_eq!(activity.phase_at(1_000), TurnPhase::Unknown);
    }

    // A clock that disagrees must not resurrect an expired phase.
    #[test]
    fn a_backwards_clock_does_not_extend_a_phase() {
        let activity = phased(&[("UserPromptSubmit", 10_000)]);
        assert_eq!(activity.phase_at(9_999), TurnPhase::Unknown);
    }

    #[test]
    fn an_unrecognised_event_leaves_the_phase_alone() {
        let activity = phased(&[("UserPromptSubmit", 1_000), ("SomethingElse", 2_000)]);
        assert_eq!(activity.phase_at(2_000), TurnPhase::Working);
        assert_eq!(event_phase("SomethingElse"), None);
    }

    #[test]
    fn every_phase_has_a_distinct_human_label() {
        let labels = [
            TurnPhase::Unknown.label(),
            TurnPhase::Idle.label(),
            TurnPhase::Working.label(),
            TurnPhase::AwaitingUser.label(),
        ];
        let mut unique = labels.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len());
    }

    #[test]
    fn the_footer_shows_the_phase_with_the_hook_that_established_it() {
        let mut activity = phased(&[("UserPromptSubmit", 1_000)]);
        activity.latest = Some(Hint {
            harness: Harness::Claude,
            event: "UserPromptSubmit".into(),
            sent_at_ms: 1_000,
        });
        assert_eq!(
            activity.label_at(1_000),
            "Claude | working | recent hook: prompt submitted"
        );
    }

    // The hook expires long before the turn does. The phase must still be
    // reported, without a stale event name implying it just happened.
    #[test]
    fn a_working_turn_is_still_reported_once_its_hook_label_expires() {
        let mut activity = phased(&[("UserPromptSubmit", 1_000)]);
        activity.latest = Some(Hint {
            harness: Harness::Claude,
            event: "UserPromptSubmit".into(),
            sent_at_ms: 1_000,
        });
        assert_eq!(activity.label_at(1_000 + HINT_LEASE_MS), "Claude | working");
    }

    #[test]
    fn nothing_observed_still_reads_as_activity_unavailable() {
        let activity = phased(&[]);
        assert_eq!(activity.label_at(1_000), "Claude | activity unavailable");
    }

    #[test]
    fn hints_expire_at_the_boundary_and_reject_future_timestamps() {
        let hint = Hint {
            harness: Harness::Claude,
            event: "PermissionRequest".into(),
            sent_at_ms: 20_000,
        };
        assert!(!valid_hint(&hint, 19_999));
        assert!(valid_hint(&hint, 20_000));
        assert!(valid_hint(&hint, 29_999));
        assert!(!valid_hint(&hint, 30_000));
        let activity = Activity {
            harness: Harness::Claude,
            socket: None,
            latest: Some(hint),
            phase: None,
        };
        assert_eq!(
            activity.label_at(20_000),
            "Claude | recent hook: permission requested"
        );
        assert_eq!(activity.label_at(30_000), "Claude | activity unavailable");
    }

    #[test]
    fn malformed_foreign_and_expired_datagrams_cannot_set_activity() {
        let bridge = BridgeDirectory::create().unwrap();
        let mut activity = Activity {
            harness: Harness::Claude,
            socket: Some(Activity::bind(&bridge.socket_path()).unwrap()),
            latest: None,
            phase: None,
        };
        let socket = UnixDatagram::unbound().unwrap();
        socket.connect(bridge.socket_path()).unwrap();
        for bytes in [
            "not json",
            r#"{"harness":"claude","event":"Stop","sent_at_ms":0,"prompt":"private"}"#,
        ] {
            socket.send(bytes.as_bytes()).unwrap();
        }
        for (harness, event, sent_at_ms) in [
            (Harness::Codex, "Stop", now_ms().unwrap()),
            (Harness::Claude, "Stop", 0),
            (Harness::Claude, "Stop", u64::MAX),
            (Harness::Claude, "\u{1b}[2J", now_ms().unwrap()),
        ] {
            send_hint(
                &bridge.socket_path(),
                &Hint {
                    harness,
                    event: event.into(),
                    sent_at_ms,
                },
            )
            .unwrap();
        }
        activity.poll();
        assert_eq!(activity.label(), "Claude | activity unavailable");
    }

    #[test]
    fn only_explicit_supported_harness_names_are_accepted() {
        assert_eq!(Harness::parse("claude"), Some(Harness::Claude));
        assert_eq!(Harness::parse("codex"), Some(Harness::Codex));
        for name in ["", "CODEX", "claude --yes", "../codex", "bash"] {
            assert_eq!(Harness::parse(name), None);
        }
    }

    #[test]
    fn generated_hooks_quote_the_binary_and_never_emit_policy_overrides() {
        for harness in [Harness::Claude, Harness::Codex] {
            let config = hook_configuration(harness, "/tmp/a b/'quoted'/sponsor-shell");
            assert_eq!(config.as_object().unwrap().len(), 1);
            let hooks = config["hooks"].as_object().unwrap();
            assert_eq!(hooks.len(), events(harness).len());
            for (event, groups) in hooks {
                assert!(events(harness).contains(&event.as_str()));
                assert!(event_label(event).is_some());
                let handler = &groups[0]["hooks"][0];
                assert_eq!(handler.as_object().unwrap().len(), 3);
                assert_eq!(handler["type"], "command");
                assert_eq!(handler["timeout"], 1);
                assert_eq!(
                    handler["command"],
                    format!(
                        "'/tmp/a b/'\\''quoted'\\''/sponsor-shell' harness-event {}",
                        harness.command()
                    )
                );
            }
            assert!(!config.to_string().contains("async"));
            assert!(!config.to_string().contains("permissionDecision"));
        }
    }

    #[test]
    fn environment_clears_inherited_channels_before_optional_session_binding() {
        let cleared = vec!["env", "-u", HARNESS_ENV, "-u", SOCKET_ENV];
        assert_eq!(environment(None, None), cleared);
        assert_eq!(environment(Some(Harness::Codex), None), cleared);
        let configured = environment(Some(Harness::Claude), Some(Path::new("/tmp/a b/events")));
        assert_eq!(configured[..5], cleared);
        assert_eq!(configured[5], format!("{HARNESS_ENV}=claude"));
        assert_eq!(configured[6], format!("{SOCKET_ENV}=/tmp/a b/events"));
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
