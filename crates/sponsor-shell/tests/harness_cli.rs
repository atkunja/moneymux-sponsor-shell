use std::io::Write;
use std::net::TcpListener;
use std::process::{Command, Stdio};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_sponsor-shell"))
}

#[test]
fn hook_entry_point_is_silent_and_offline_even_with_invalid_input() {
    let network = TcpListener::bind("127.0.0.1:0").unwrap();
    network.set_nonblocking(true).unwrap();
    for input in [
        "invalid json",
        r#"{"hook_event_name":"PermissionRequest","tool_input":{"secret":"never-print"}}"#,
        r#"{"hook_event_name":"UserPromptSubmit","prompt":"never-print"}"#,
        r#"{"hook_event_name":"Stop","last_assistant_message":"never-print"}"#,
    ] {
        let mut child = binary()
            .args(["harness-event", "codex"])
            .env("SPONSOR_SHELL_HARNESS", "codex")
            .env(
                "SPONSOR_SHELL_HOOK_SOCKET",
                "/nonexistent/moneymux-hook-socket",
            )
            .env(
                "SPONSOR_SHELL_API_BASE_URL",
                format!("http://{}", network.local_addr().unwrap()),
            )
            .env("SPONSOR_SHELL_DEVICE_ID", "synthetic-offline-test")
            .env("SPONSOR_SHELL_DEVICE_TOKEN", "synthetic-offline-token")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        network.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn unused_or_malformed_hook_invocations_are_successful_noops() {
    for args in [
        vec!["harness-event"],
        vec!["harness-event", "unknown"],
        vec!["harness-event", "claude", "unexpected"],
        vec!["harness-event", "claude"],
    ] {
        let output = binary()
            .args(args)
            .env_remove("SPONSOR_SHELL_HOOK_SOCKET")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn setup_prints_json_without_tmux_or_harness_installations() {
    for harness in ["claude", "codex"] {
        let output = binary()
            .args(["harness-hooks", harness])
            .env("PATH", "")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let config: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(config["hooks"]["PermissionRequest"].is_array());
    }
    let output = binary().args(["harness", "unknown"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}
