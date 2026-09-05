use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossterm::cursor;
use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use crossterm::style;
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode, ClearType};
use crossterm::{execute, queue};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

mod harness;

const ANIMATION_TICK: Duration = Duration::from_millis(250);
const AD_PANE_ARG: &str = "--sponsor-shell-ad-pane";
const DEFAULT_APP_PANE_PERCENT: &str = "75";
const IDLE_FULLSCREEN_DELAY: Duration = Duration::from_secs(30);
const MIN_RENDERED_VISIBLE_DURATION: Duration = Duration::from_secs(1);
const SPONSOR_SESSION_ENV: &str = "SPONSOR_SHELL_TMUX_SESSION";
const SPONSOR_APP_PERCENT_ENV: &str = "SPONSOR_SHELL_APP_PANE_PERCENT";
const SPONSOR_AD_FILE_ENV: &str = "SPONSOR_SHELL_AD_FILE";
const SPONSOR_API_BASE_ENV: &str = "SPONSOR_SHELL_API_BASE_URL";
const SPONSOR_DEVICE_ID_ENV: &str = "SPONSOR_SHELL_DEVICE_ID";
const SPONSOR_DEVICE_TOKEN_ENV: &str = "SPONSOR_SHELL_DEVICE_TOKEN";
const SPONSOR_WRAPPED_COMMAND_ENV: &str = "SPONSOR_SHELL_WRAPPED_COMMAND";
const SPONSOR_CONFIG_ENV: &str = "SPONSOR_SHELL_CONFIG";
const SPONSOR_IDLE_FULLSCREEN_SECONDS_ENV: &str = "SPONSOR_SHELL_IDLE_FULLSCREEN_SECONDS";
/// Interactivity decided by the OUTER process and handed to the ad pane.
///
/// It cannot be recomputed inside the pane: tmux gives every pane a PTY, so a
/// TTY check there is true by construction, and a pane inherits the tmux
/// SERVER's environment rather than the invoking client's — so `CI` is invisible
/// there whenever a tmux server was already running. Both signals only exist in
/// the process the user actually launched.
const SPONSOR_INTERACTIVE_ENV: &str = "SPONSOR_SHELL_INTERACTIVE";
// Opt-in: also expand the ad while the wrapped harness is "working" (loading /
// streaming) — i.e. its pane is producing output while the user is not typing.
// This is the harness-agnostic way to show an ad during a tool's loading state
// (Claude Code / Codex / any TUI) without touching the harness itself. Default
// off so no existing behavior changes.
const SPONSOR_AD_WHILE_WORKING_ENV: &str = "SPONSOR_SHELL_AD_WHILE_WORKING";
// While the harness is working, expand the ad once the user has been idle this
// long and the app pane produced output within this recent window.
const AD_WHILE_WORKING_USER_IDLE: Duration = Duration::from_secs(2);
const AD_WHILE_WORKING_ACTIVITY_WINDOW: Duration = Duration::from_secs(2);
const REMOTE_AD_REFRESH: Duration = Duration::from_secs(30);
const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
static CLICK_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static API_AGENT: OnceLock<ureq::Agent> = OnceLock::new();
const RAILWAY_LOGO: &[&str] = &[
    "    ____  ___    ______ _       _______  __",
    "   / __ \\/   |  /  _/ /| |     / /   \\ \\/ /",
    "  / /_/ / /| |  / // / | | /| / / /| |\\  / ",
    " / _, _/ ___ |_/ // /__| |/ |/ / ___ |/ /  ",
    "/_/ |_/_/  |_/___/_____/__/|__/_/  |_/_/   ",
];
const RAILWAY_SMALL_LOGO: &[&str] = &[
    " ___    _   ___ _ __      _____   __",
    "| _ \\  /_\\ |_ _| |\\ \\    / /_\\ \\ / /",
    "|   / / _ \\ | || |_\\ \\/\\/ / _ \\ V / ",
    "|_|_\\/_/ \\_\\___|____\\_/\\_/_/ \\_\\_|  ",
];
const RAILWAY_LINKS: &[&str] = &[
    "https://railway.app",
    "https://railway.app/templates",
    "https://railway.app/new",
];

struct LogoVariants {
    large: Vec<String>,
    small: Vec<String>,
}

struct AdCreative {
    id: String,
    ad_decision_id: Option<String>,
    decision_token: Option<String>,
    sponsor: String,
    url: String,
    headline: String,
    subheadline: String,
    cta: String,
    disclosure: String,
    idle_fullscreen_seconds: u64,
    logos: LogoVariants,
    route: Vec<String>,
    links: Vec<String>,
    stats: Vec<String>,
}

struct PendingImpressionReport {
    ad_decision_id: String,
    body: String,
    in_flight: bool,
    next_attempt_at: Instant,
    created_at: Instant,
    attempts: u32,
}

const IMPRESSION_REPORT_RETRY_WINDOW: Duration = Duration::from_secs(4 * 60);

#[derive(Deserialize)]
struct LocalAdCreative {
    enabled: Option<bool>,
    sponsor: Option<String>,
    url: Option<String>,
    headline: Option<String>,
    subheadline: Option<String>,
    cta: Option<String>,
    disclosure: Option<String>,
    #[serde(rename = "idleFullscreenSeconds")]
    idle_fullscreen_seconds: Option<u64>,
    #[serde(rename = "asciiArt")]
    ascii_art: Option<String>,
    #[serde(rename = "asciiArtSmall")]
    ascii_art_small: Option<String>,
    route: Option<Vec<String>>,
    links: Option<Vec<String>>,
    stats: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RemoteAdDecision {
    #[serde(rename = "adDecisionId")]
    ad_decision_id: Option<String>,
    #[serde(rename = "decisionToken")]
    decision_token: Option<String>,
    creative: Option<LocalAdCreative>,
}

#[derive(Deserialize)]
struct RemoteTerminalSession {
    id: String,
}

#[derive(Default, Deserialize, Serialize)]
struct CliConfig {
    #[serde(rename = "apiBaseUrl", skip_serializing_if = "Option::is_none")]
    api_base_url: Option<String>,
    #[serde(rename = "deviceId", skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(rename = "deviceToken", skip_serializing_if = "Option::is_none")]
    device_token: Option<String>,
}

fn railway_example_creative() -> AdCreative {
    AdCreative {
        id: "railway-demo".to_string(),
        ad_decision_id: None,
        decision_token: None,
        sponsor: "RAILWAY".to_string(),
        url: "https://railway.app".to_string(),
        headline: "Deploy apps, workers, databases, and cron in minutes.".to_string(),
        subheadline:
            "Preview URLs, background jobs, and production deploys from one terminal flow."
                .to_string(),
        cta: "Start a project at railway.app/new".to_string(),
        disclosure: "Sponsored terminal time".to_string(),
        idle_fullscreen_seconds: IDLE_FULLSCREEN_DELAY.as_secs(),
        logos: LogoVariants {
            large: string_lines(RAILWAY_LOGO),
            small: string_lines(RAILWAY_SMALL_LOGO),
        },
        route: vec![
            "repo".to_string(),
            "build".to_string(),
            "preview".to_string(),
            "ship".to_string(),
        ],
        links: string_lines(RAILWAY_LINKS),
        stats: vec![
            "apps".to_string(),
            "workers".to_string(),
            "databases".to_string(),
            "cron".to_string(),
        ],
    }
}

fn inactive_creative() -> AdCreative {
    AdCreative {
        id: "local-disabled".to_string(),
        ad_decision_id: None,
        decision_token: None,
        sponsor: "SPONSOR SLOT".to_string(),
        url: "localhost:3000".to_string(),
        headline: "No local ad is currently injected.".to_string(),
        subheadline: "Open the local editor to create ASCII art and submit a sponsor creative."
            .to_string(),
        cta: "Run pnpm dev and visit localhost:3000".to_string(),
        disclosure: "Local terminal inventory".to_string(),
        idle_fullscreen_seconds: IDLE_FULLSCREEN_DELAY.as_secs(),
        logos: LogoVariants {
            large: vec![
                " ____  ____   ___  _   _ ____   ___  ____  ".to_string(),
                "/ ___||  _ \\ / _ \\| \\ | / ___| / _ \\|  _ \\ ".to_string(),
                "\\___ \\| |_) | | | |  \\| \\___ \\| | | | |_) |".to_string(),
                " ___) |  __/| |_| | |\\  |___) | |_| |  _ < ".to_string(),
                "|____/|_|    \\___/|_| \\_|____/ \\___/|_| \\_\\".to_string(),
            ],
            small: vec!["SPONSOR SLOT".to_string()],
        },
        route: vec![
            "local".to_string(),
            "preview".to_string(),
            "inject".to_string(),
        ],
        links: vec!["localhost:3000".to_string()],
        stats: vec![
            "editor".to_string(),
            "sqlite".to_string(),
            "terminal".to_string(),
        ],
    }
}

struct PaneGuard;

impl PaneGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable sponsor pane raw mode")?;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            event::EnableMouseCapture,
            cursor::Hide
        )
        .context("failed to enter sponsor pane")?;
        Ok(Self)
    }
}

impl Drop for PaneGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            style::ResetColor,
            cursor::Show,
            event::DisableMouseCapture,
            terminal::LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

fn string_lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

#[derive(Clone, Copy)]
struct Layout {
    cols: u16,
    rows: u16,
}

impl Layout {
    fn current() -> Self {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        Self::new(cols, rows)
    }

    fn new(cols: u16, rows: u16) -> Self {
        let cols = if cols == 0 { 80 } else { cols.max(2) };
        let rows = if rows == 0 { 24 } else { rows.max(1) };
        Self { cols, rows }
    }
}

fn main() {
    let exit_code = match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("sponsor-shell: {error:#}");
            1
        }
    };
    std::process::exit(exit_code);
}

fn run() -> Result<i32> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "harness-event") {
        // This entry point never prints diagnostics or emits model/permission output.
        if args.len() == 2 {
            if let Some(harness) = harness::Harness::parse(&args[1]) {
                harness::emit(harness);
            }
        }
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == "harness-hooks") {
        let harness = args
            .get(1)
            .and_then(|value| harness::Harness::parse(value))
            .filter(|_| args.len() == 2)
            .context("usage: sponsor-shell harness-hooks <claude|codex> (prints JSON only)")?;
        let executable = env::current_exe().context("failed to locate sponsor-shell")?;
        let executable = executable
            .to_str()
            .context("hook executable path must be UTF-8")?;
        println!(
            "{}",
            serde_json::to_string_pretty(&harness::hook_configuration(harness, executable))?
        );
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == "harness") {
        let harness = args
            .get(1)
            .and_then(|value| harness::Harness::parse(value))
            .context("usage: sponsor-shell harness <claude|codex> [-- arguments...]")?;
        let remaining = &args[2..];
        let forwarded = remaining
            .strip_prefix(&["--".to_string()])
            .unwrap_or(remaining);
        let executable = harness::executable(harness)
            .context("selected harness is not executable on the invoking shell's PATH")?;
        let executable = executable
            .to_str()
            .context("harness executable path must be UTF-8")?;
        return run_tmux_shell(executable, forwarded, Some(harness));
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--help" || arg == "-h" || arg == "help")
    {
        print_help();
        return Ok(0);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "--version" || arg == "-V" || arg == "version")
    {
        println!("sponsor-shell {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == AD_PANE_ARG) {
        run_sponsor_pane()?;
        return Ok(0);
    }
    if args.first().is_some_and(|arg| arg == "login") {
        return run_login(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "link") {
        return run_link(&args[1..]);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "unlink" || arg == "logout")
    {
        return run_unlink(&args[1..]);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "config" || arg == "configure")
    {
        return run_configure(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "status") {
        return run_status();
    }
    if args.first().is_some_and(|arg| arg == "doctor") {
        return run_doctor(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "install-tmux") {
        return run_install_tmux(&args[1..]);
    }

    let command = args.first().cloned().unwrap_or_else(default_shell_command);
    let command_args = if args.is_empty() {
        Vec::new()
    } else {
        args[1..].to_vec()
    };

    run_tmux_shell(&command, &command_args, None)
}

fn print_help() {
    for line in help_lines() {
        println!("{line}");
    }
}

fn help_lines() -> &'static [&'static str] {
    &[
        "Sponsor Shell — transparent sponsored terminal inventory",
        "",
        "Usage:",
        "  sponsor-shell [command-to-wrap] [arguments...]",
        "  sponsor-shell <management-command>",
        "  sponsor-shell harness <claude|codex> [-- arguments...]",
        "",
        "Management commands:",
        "  login          Open the MoneyMux developer onboarding page",
        "  link           Store a terminal device ID and token",
        "  unlink         Remove stored device credentials (alias: logout)",
        "  configure      Set the MoneyMux API base URL",
        "  status         Show the current local configuration",
        "  doctor         Run secret-free local diagnostics",
        "  install-tmux   Explicitly install the required tmux dependency",
        "  harness        Wrap Claude/Codex with a protected split pane and local hook hints",
        "  harness-hooks  Print optional hook JSON; does not install or replace settings",
        "  help           Show this help",
        "  version        Show the installed Sponsor Shell version",
    ]
}

fn default_shell_command() -> String {
    env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        }
    })
}

fn platform_base_url() -> String {
    env::var(SPONSOR_API_BASE_ENV)
        .ok()
        .or_else(|| {
            load_cli_config()
                .ok()
                .and_then(|config| clean_config_value(config.api_base_url))
        })
        .unwrap_or_else(|| "http://localhost:4000".to_string())
}

fn run_login(args: &[String]) -> Result<i32> {
    let options = parse_api_base_args(args, "login")?;
    if options.help {
        print_login_help();
        return Ok(0);
    }
    let base_url = configure_api_base_url(options.api_base_url)?.unwrap_or_else(platform_base_url);
    let onboarding_url = publisher_onboarding_url(&base_url)?;
    println!("sponsor-shell publisher login");
    println!("api: {base_url}");
    println!("Open this URL to create a publisher account and connect Stripe:");
    println!("{onboarding_url}");
    println!();
    println!("After registering a terminal device, run:");
    println!("sponsor-shell link --device-id <device_id> --device-token <ssdev_token>");
    open_url(&onboarding_url).ok();
    Ok(0)
}

fn print_login_help() {
    println!("sponsor-shell login [--api-base-url https://sponsor-shell.example.com]");
    println!("stores the API URL when provided, then opens publisher account access");
}

fn publisher_onboarding_url(base_url: &str) -> Result<String> {
    let mut url = Url::parse(base_url).context("invalid publisher onboarding base URL")?;
    url.set_path("/app");
    url.set_query(Some(
        "section=auth&mode=signup&role=publisher&next=publisher",
    ));
    url.set_fragment(None);
    Ok(url.into())
}

fn run_configure(args: &[String]) -> Result<i32> {
    let options = parse_api_base_args(args, "configure")?;
    if options.help {
        print_configure_help();
        return Ok(0);
    }
    let configured = configure_api_base_url(options.api_base_url)?
        .context("missing API URL; pass --api-base-url https://sponsor-shell.example.com")?;
    println!("sponsor-shell configured");
    println!("config: {}", config_path().display());
    println!("api: {configured}");
    Ok(0)
}

fn print_configure_help() {
    println!("sponsor-shell configure --api-base-url https://sponsor-shell.example.com");
    println!("writes the hosted API URL without changing linked device credentials");
}

fn run_link(args: &[String]) -> Result<i32> {
    let mut api_base_url = None;
    let mut device_id = None;
    let mut device_token = None;
    let mut positional = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--api-base-url" => {
                index += 1;
                api_base_url = args.get(index).cloned();
            }
            "--device-id" => {
                index += 1;
                device_id = args.get(index).cloned();
            }
            "--device-token" => {
                index += 1;
                device_token = args.get(index).cloned();
            }
            "--help" | "-h" => {
                print_link_help();
                return Ok(0);
            }
            value => positional.push(value.to_string()),
        }
        index += 1;
    }

    if device_id.is_none() {
        device_id = positional.first().cloned();
    }
    if device_token.is_none() {
        device_token = positional.get(1).cloned();
    }

    let device_id =
        clean_config_value(device_id).context("missing device id; pass --device-id <device_id>")?;
    let device_token = clean_config_value(device_token)
        .context("missing device token; pass --device-token <ssdev_token>")?;
    let mut config = load_cli_config().unwrap_or_default();
    let api_base_url = clean_config_value(api_base_url).unwrap_or_else(platform_base_url);
    config.api_base_url = Some(validate_api_base_url(&api_base_url)?);
    config.device_id = Some(device_id);
    config.device_token = Some(device_token);
    save_cli_config(&config)?;

    println!("sponsor-shell linked");
    println!("config: {}", config_path().display());
    println!(
        "api: {}",
        config
            .api_base_url
            .as_deref()
            .unwrap_or("http://localhost:4000")
    );
    Ok(0)
}

fn print_link_help() {
    println!("sponsor-shell link --device-id <device_id> --device-token <ssdev_token>");
    println!("optional: --api-base-url https://sponsor-shell.example.com");
}

fn run_unlink(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("sponsor-shell unlink");
        println!("removes the stored device ID and token while preserving the configured API URL");
        return Ok(0);
    }
    if let Some(option) = args.first() {
        anyhow::bail!("unknown unlink option: {option}");
    }

    let path = config_path();
    let mut removed = false;
    if path.exists() {
        let mut config = load_cli_config()?;
        removed = clear_device_credentials(&mut config);
        if removed {
            save_cli_config(&config)?;
        }
    }

    if removed {
        println!("sponsor-shell unlinked");
        println!("removed stored device credentials from {}", path.display());
    } else {
        println!("sponsor-shell is already unlinked");
    }
    let overrides = active_credential_override_names();
    if !overrides.is_empty() {
        println!(
            "environment overrides remain active: {}",
            overrides.join(", ")
        );
    }
    Ok(0)
}

fn clear_device_credentials(config: &mut CliConfig) -> bool {
    let had_credentials = config.device_id.is_some() || config.device_token.is_some();
    config.device_id = None;
    config.device_token = None;
    had_credentials
}

fn active_credential_override_names() -> Vec<&'static str> {
    [SPONSOR_DEVICE_ID_ENV, SPONSOR_DEVICE_TOKEN_ENV]
        .into_iter()
        .filter(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
        .collect()
}

fn validate_api_base_url(value: &str) -> Result<String> {
    let normalized = value.trim().trim_end_matches('/');
    let parsed = Url::parse(normalized).context("invalid API base URL")?;
    if parsed.host().is_none() {
        anyhow::bail!("API base URL must include a host");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        anyhow::bail!("API base URL must not contain embedded credentials");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        anyhow::bail!("API base URL must not contain a query string or fragment");
    }

    match parsed.scheme() {
        "https" => {}
        "http" if is_local_api_host(&parsed) => {}
        "http" => anyhow::bail!("remote API base URLs must use https://"),
        _ => anyhow::bail!("API base URL must use https://, or http:// for local development"),
    }

    Ok(normalized.to_string())
}

fn is_local_api_host(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost")
                || host.to_ascii_lowercase().ends_with(".localhost")
        }
        Some(Host::Ipv4(host)) => host.is_loopback() || host.is_unspecified(),
        Some(Host::Ipv6(host)) => host.is_loopback() || host.is_unspecified(),
        None => false,
    }
}

#[derive(Default)]
struct ApiBaseOptions {
    api_base_url: Option<String>,
    help: bool,
}

fn parse_api_base_args(args: &[String], command: &str) -> Result<ApiBaseOptions> {
    let mut options = ApiBaseOptions::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--api-base-url" => {
                index += 1;
                options.api_base_url = args.get(index).cloned();
            }
            "--help" | "-h" => {
                options.help = true;
            }
            value if !value.starts_with('-') && options.api_base_url.is_none() => {
                options.api_base_url = Some(value.to_string());
            }
            value => anyhow::bail!("unknown {command} option: {value}"),
        }
        index += 1;
    }

    Ok(options)
}

fn configure_api_base_url(api_base_url: Option<String>) -> Result<Option<String>> {
    let Some(api_base_url) = clean_config_value(api_base_url) else {
        return Ok(None);
    };
    let api_base_url = validate_api_base_url(&api_base_url)?;
    let mut config = load_cli_config().unwrap_or_default();
    config.api_base_url = Some(api_base_url.clone());
    save_cli_config(&config)?;
    Ok(Some(api_base_url))
}

fn run_status() -> Result<i32> {
    println!("sponsor-shell");
    println!("api: {}", platform_base_url());
    println!("config: {}", config_path().display());
    println!(
        "device: {}",
        if device_id().is_some() {
            "linked"
        } else {
            "not linked"
        }
    );
    println!("ad file: {}", ad_file_path().display());
    println!("default command: {}", default_shell_command());
    println!(
        "tmux: {}",
        if tmux_available() {
            "available"
        } else {
            "missing"
        }
    );
    Ok(0)
}

struct DoctorSnapshot {
    api_base_url: String,
    api_transport: &'static str,
    config_state: &'static str,
    device_state: &'static str,
    tmux_available: bool,
    interactive: bool,
    credential_overrides: Vec<&'static str>,
}

fn run_doctor(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("sponsor-shell doctor");
        println!("prints local diagnostics without displaying device credential values");
        return Ok(0);
    }
    if let Some(option) = args.first() {
        anyhow::bail!("unknown doctor option: {option}");
    }

    let snapshot = collect_doctor_snapshot();
    for line in doctor_lines(&snapshot) {
        println!("{line}");
    }
    let ready = snapshot.api_transport != "invalid" && snapshot.tmux_available;
    Ok(if ready { 0 } else { 1 })
}

fn collect_doctor_snapshot() -> DoctorSnapshot {
    let api_base_url = platform_base_url();
    let api_transport = match validate_api_base_url(&api_base_url) {
        Ok(_) if api_base_url.trim().starts_with("https://") => "https",
        Ok(_) => "local-http",
        Err(_) => "invalid",
    };
    let path = config_path();
    let config_state = if !path.exists() {
        "missing"
    } else if load_cli_config().is_ok() {
        "valid"
    } else {
        "invalid"
    };
    let device_state = match (device_id().is_some(), device_token().is_some()) {
        (true, true) => "linked",
        (false, false) => "not-linked",
        _ => "incomplete",
    };

    DoctorSnapshot {
        api_base_url,
        api_transport,
        config_state,
        device_state,
        tmux_available: tmux_available(),
        interactive: outer_terminal_is_interactive(),
        credential_overrides: active_credential_override_names(),
    }
}

fn doctor_lines(snapshot: &DoctorSnapshot) -> Vec<String> {
    vec![
        "sponsor-shell doctor".to_string(),
        format!("version: {}", env!("CARGO_PKG_VERSION")),
        format!("platform: {}/{}", env::consts::OS, env::consts::ARCH),
        format!("api: {}", snapshot.api_base_url),
        format!("api transport: {}", snapshot.api_transport),
        format!("config: {}", snapshot.config_state),
        format!("device: {}", snapshot.device_state),
        format!(
            "tmux: {}",
            if snapshot.tmux_available {
                "available"
            } else {
                "missing"
            }
        ),
        format!(
            "interactive terminal: {}",
            if snapshot.interactive { "yes" } else { "no" }
        ),
        format!(
            "credential environment overrides: {}",
            if snapshot.credential_overrides.is_empty() {
                "none".to_string()
            } else {
                snapshot.credential_overrides.join(", ")
            }
        ),
    ]
}

fn config_path() -> PathBuf {
    if let Ok(path) = env::var(SPONSOR_CONFIG_ENV) {
        return PathBuf::from(path);
    }
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".sponsor-shell")
        .join("config.json")
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var("USERPROFILE")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        })
}

fn load_cli_config() -> Result<CliConfig> {
    let raw = fs::read_to_string(config_path()).context("failed to read sponsor-shell config")?;
    serde_json::from_str(&raw).context("failed to parse sponsor-shell config")
}

fn save_cli_config(config: &CliConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create sponsor-shell config directory")?;
    }
    let raw = serde_json::to_string_pretty(config).context("failed to serialize config")?;
    let contents = format!("{raw}\n");

    // The config stores the device bearer token, so keep it owner-only (0600).
    // Create it with restrictive permissions and re-assert them in case the file
    // already existed with a looser mode.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .context("failed to write sponsor-shell config")?;
        file.write_all(contents.as_bytes())
            .context("failed to write sponsor-shell config")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .context("failed to restrict sponsor-shell config permissions")?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(&path, contents).context("failed to write sponsor-shell config")
    }
}

fn clean_config_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn device_id() -> Option<String> {
    env::var(SPONSOR_DEVICE_ID_ENV)
        .ok()
        .and_then(|value| clean_config_value(Some(value)))
        .or_else(|| {
            load_cli_config()
                .ok()
                .and_then(|config| clean_config_value(config.device_id))
        })
}

fn device_token() -> Option<String> {
    env::var(SPONSOR_DEVICE_TOKEN_ENV)
        .ok()
        .and_then(|value| clean_config_value(Some(value)))
        .or_else(|| {
            load_cli_config()
                .ok()
                .and_then(|config| clean_config_value(config.device_token))
        })
}

fn default_ad_creative() -> AdCreative {
    railway_example_creative()
}

fn ad_file_path() -> PathBuf {
    env::var(SPONSOR_AD_FILE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".sponsor-shell/ad.json"))
}

fn ad_file_modified_at() -> Option<SystemTime> {
    fs::metadata(ad_file_path())
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn load_ad_creative() -> Option<AdCreative> {
    let raw = fs::read_to_string(ad_file_path()).ok()?;
    let local: LocalAdCreative = serde_json::from_str(&raw).ok()?;
    Some(local.into_ad_creative())
}

fn load_active_ad_creative(
    layout: Layout,
    session_id: Option<&str>,
    seconds_since_last_ad: Option<u64>,
    ads_shown_this_session: u64,
) -> Option<AdCreative> {
    load_remote_ad_creative(
        layout,
        session_id,
        seconds_since_last_ad,
        ads_shown_this_session,
    )
    .or_else(load_ad_creative)
}

fn remote_ads_enabled() -> bool {
    device_id().is_some()
}

fn load_remote_ad_creative(
    layout: Layout,
    session_id: Option<&str>,
    seconds_since_last_ad: Option<u64>,
    ads_shown_this_session: u64,
) -> Option<AdCreative> {
    let device_id = device_id()?;
    let url = api_url("/api/ad-decision");
    let body = remote_ad_decision_body(
        &device_id,
        layout,
        session_id,
        seconds_since_last_ad,
        ads_shown_this_session,
    );

    let response = api_post(&url, body).ok()?;
    let remote: RemoteAdDecision = serde_json::from_str(&response).ok()?;
    let mut creative = remote.creative?.into_ad_creative();
    creative.ad_decision_id = remote.ad_decision_id.clone();
    creative.decision_token = remote.decision_token;
    if let Some(decision_id) = remote.ad_decision_id {
        creative.id = format!("remote-{decision_id}");
    } else {
        creative.id = "remote".to_string();
    }
    Some(creative)
}

/// Interactivity as judged by the process the user actually launched.
///
/// Must only be called from the OUTER process (before tmux is spawned). Inside
/// a tmux pane both signals are useless: the pane always has a PTY, and the
/// pane inherits the tmux server's environment, so `CI` is invisible whenever a
/// server was already running.
fn outer_terminal_is_interactive() -> bool {
    use std::io::IsTerminal;
    if running_in_ci() {
        return false;
    }
    std::io::stdout().is_terminal() && std::io::stderr().is_terminal()
}

/// Common CI markers. `CI` alone is not enough — some providers set only their
/// own variable, and a self-hosted runner may set none, which is why the TTY
/// check above is evaluated in the outer process rather than relied on here.
fn running_in_ci() -> bool {
    const MARKERS: [&str; 8] = [
        "CI",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "BUILDKITE",
        "CIRCLECI",
        "TRAVIS",
        "TEAMCITY_VERSION",
        "JENKINS_URL",
    ];
    MARKERS.iter().any(|key| {
        env::var(key).is_ok_and(|value| !value.is_empty() && value != "0" && value != "false")
    })
}

/// Interactivity for the ad pane: the verdict handed down by the outer process.
///
/// Falls back to a local check only when the variable is absent (an ad pane
/// launched directly, outside the normal flow).
fn terminal_is_interactive() -> bool {
    match env::var(SPONSOR_INTERACTIVE_ENV) {
        Ok(value) if value == "1" => true,
        Ok(_) => false,
        Err(_) => outer_terminal_is_interactive(),
    }
}

fn remote_ad_decision_body(
    device_id: &str,
    layout: Layout,
    session_id: Option<&str>,
    seconds_since_last_ad: Option<u64>,
    ads_shown_this_session: u64,
) -> String {
    // Report the REAL terminal state. These were hardcoded `true`, which made
    // them worthless as signals and — because the platform bills for qualified
    // impressions — meant an ad rendered into a CI log or a redirected file
    // could still be charged to an advertiser and credited to a publisher.
    // A human has to be able to see the ad for it to be worth anything.
    let mut body = serde_json::json!({
        "deviceId": device_id,
        "width": layout.cols,
        "height": layout.rows,
        "placement": "prompt_boundary",
        "isTty": terminal_is_interactive(),
        "isInteractive": terminal_is_interactive(),
        "adsShownThisSession": ads_shown_this_session,
    });
    if let Some(session_id) = session_id {
        body["sessionId"] = serde_json::json!(session_id);
    }
    if let Some(seconds) = seconds_since_last_ad {
        body["secondsSinceLastAd"] = serde_json::json!(seconds);
    }
    body.to_string()
}

fn api_url(path: &str) -> String {
    format!("{}{}", platform_base_url().trim_end_matches('/'), path)
}

fn api_post(url: &str, body: String) -> Result<String> {
    let token = device_token();
    api_post_with_token(url, &body, token.as_deref())
}

fn api_post_with_token(url: &str, body: &str, token: Option<&str>) -> Result<String> {
    validate_api_base_url(url).context("refusing unsafe Sponsor Shell API request")?;
    let agent = API_AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(API_REQUEST_TIMEOUT))
            .build()
            .into()
    });
    let mut request = agent.post(url).header("Content-Type", "application/json");
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = request
        .send(body)
        .context("failed to post sponsor-shell API request")?;
    response
        .body_mut()
        .read_to_string()
        .context("failed to read sponsor-shell API response")
}

fn enqueue_impression_if_needed(
    creative: &AdCreative,
    reported_ad_decision_ids: &HashSet<String>,
    exhausted_ad_decision_ids: &HashSet<String>,
    pending_reports: &mut Vec<PendingImpressionReport>,
    layout: Layout,
    next_render_sequence: &mut u64,
    visible_duration_ms: u128,
) -> bool {
    let Some(ad_decision_id) = &creative.ad_decision_id else {
        return false;
    };
    if reported_ad_decision_ids.contains(ad_decision_id)
        || exhausted_ad_decision_ids.contains(ad_decision_id)
        || pending_reports
            .iter()
            .any(|report| report.ad_decision_id == *ad_decision_id)
    {
        return false;
    }

    let client_sequence_number = *next_render_sequence;
    *next_render_sequence += 1;
    let mut body = serde_json::json!({
        "adDecisionId": ad_decision_id,
        "terminalColumns": layout.cols,
        "terminalRows": layout.rows,
        "lineCount": ad_height(creative, layout.cols),
        "visibleDurationMs": visible_duration_ms,
        "clientSequenceNumber": client_sequence_number,
    });
    if let Some(token) = creative.decision_token.as_deref() {
        body["decisionToken"] = serde_json::json!(token);
    }
    pending_reports.push(PendingImpressionReport {
        ad_decision_id: ad_decision_id.clone(),
        body: body.to_string(),
        in_flight: false,
        next_attempt_at: Instant::now(),
        created_at: Instant::now(),
        attempts: 0,
    });
    true
}

fn pump_impression_reports(
    pending_reports: &mut Vec<PendingImpressionReport>,
    reported_ad_decision_ids: &mut HashSet<String>,
    exhausted_ad_decision_ids: &mut HashSet<String>,
    result_rx: &Receiver<(String, bool)>,
    result_tx: &Sender<(String, bool)>,
) {
    while let Ok((ad_decision_id, success)) = result_rx.try_recv() {
        if success {
            reported_ad_decision_ids.insert(ad_decision_id.clone());
            pending_reports.retain(|report| report.ad_decision_id != ad_decision_id);
        } else if let Some(report) = pending_reports
            .iter_mut()
            .find(|report| report.ad_decision_id == ad_decision_id)
        {
            report.in_flight = false;
            report.attempts = report.attempts.saturating_add(1);
            report.next_attempt_at = Instant::now() + impression_retry_delay(report.attempts);
        }
    }

    for report in pending_reports
        .iter()
        .filter(|report| report.created_at.elapsed() >= IMPRESSION_REPORT_RETRY_WINDOW)
    {
        exhausted_ad_decision_ids.insert(report.ad_decision_id.clone());
    }
    pending_reports.retain(|report| report.created_at.elapsed() < IMPRESSION_REPORT_RETRY_WINDOW);

    let now = Instant::now();
    for report in pending_reports
        .iter_mut()
        .filter(|report| !report.in_flight && report.next_attempt_at <= now)
    {
        report.in_flight = true;
        let ad_decision_id = report.ad_decision_id.clone();
        let body = report.body.clone();
        let result_tx = result_tx.clone();
        thread::spawn(move || {
            let success = api_post(&api_url("/api/events/impression"), body).is_ok();
            let _ = result_tx.send((ad_decision_id, success));
        });
    }
}

fn impression_retry_delay(attempts: u32) -> Duration {
    Duration::from_secs(2_u64.saturating_pow(attempts.min(5)).min(30))
}

fn report_click(creative: &AdCreative, url: &str) {
    let Some(ad_decision_id) = &creative.ad_decision_id else {
        return;
    };
    let mut body = serde_json::json!({
        "adDecisionId": ad_decision_id,
        "clientEventId": next_click_event_id(ad_decision_id),
        "url": url,
    });
    if let Some(token) = creative.decision_token.as_deref() {
        body["decisionToken"] = serde_json::json!(token);
    }
    report_api_event("/api/events/click", body.to_string());
}

fn next_click_event_id(ad_decision_id: &str) -> String {
    let sequence = CLICK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "click-{}-{}-{}-{}",
        std::process::id(),
        timestamp_millis,
        sequence,
        ad_decision_id
    )
}

fn report_api_event(path: &str, body: String) {
    let url = api_url(path);
    thread::spawn(move || {
        let _ = api_post(&url, body);
    });
}

struct RemoteTerminalSessionGuard {
    id: Option<String>,
}

impl RemoteTerminalSessionGuard {
    fn start(command: &str) -> Self {
        Self {
            id: start_remote_terminal_session(command),
        }
    }

    fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

impl Drop for RemoteTerminalSessionGuard {
    fn drop(&mut self) {
        if let Some(session_id) = &self.id {
            end_remote_terminal_session(session_id);
        }
    }
}

fn start_remote_terminal_session(command: &str) -> Option<String> {
    let device_id = device_id()?;
    let command = safe_command_name(command);
    let body = serde_json::json!({
        "deviceId": device_id,
        "command": if command.is_empty() { "shell".to_string() } else { command },
    })
    .to_string();
    let response = api_post(&api_url("/api/terminal-sessions"), body).ok()?;
    serde_json::from_str::<RemoteTerminalSession>(&response)
        .ok()
        .map(|session| session.id)
}

fn end_remote_terminal_session(session_id: &str) {
    let path = format!("/api/terminal-sessions/{session_id}/end");
    let _ = api_post(&api_url(&path), "{}".to_string());
}

fn sponsor_wrapped_command() -> String {
    env::var(SPONSOR_WRAPPED_COMMAND_ENV)
        .ok()
        .and_then(|value| clean_config_value(Some(value)))
        .unwrap_or_else(default_shell_command)
}

fn wrapped_command_label(command: &str, command_args: &[String]) -> String {
    let _ = command_args;
    safe_command_name(command)
}

fn safe_command_name(command: &str) -> String {
    PathBuf::from(command.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| truncate_chars(name, 80))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "shell".to_string())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

impl LocalAdCreative {
    fn into_ad_creative(self) -> AdCreative {
        if self.enabled == Some(false) {
            return inactive_creative();
        }

        let mut creative = railway_example_creative();
        creative.id = "local-web".to_string();
        creative.ad_decision_id = None;
        creative.decision_token = None;

        if let Some(value) = clean_text(self.sponsor) {
            creative.sponsor = value;
        }
        if let Some(value) = clean_text(self.url) {
            creative.url = value;
        }
        if let Some(value) = clean_text(self.headline) {
            creative.headline = value;
        }
        if let Some(value) = clean_text(self.subheadline) {
            creative.subheadline = value;
        }
        if let Some(value) = clean_text(self.cta) {
            creative.cta = value;
        }
        if let Some(value) = clean_text(self.disclosure) {
            creative.disclosure = value;
        }
        if let Some(seconds) = self.idle_fullscreen_seconds.filter(|seconds| *seconds > 0) {
            creative.idle_fullscreen_seconds = seconds;
        }
        if let Some(lines) = clean_block_lines(self.ascii_art) {
            creative.logos = LogoVariants {
                large: lines.clone(),
                small: lines,
            };
        }
        // A dedicated small-screen creative wins over the large-art fallback.
        if let Some(lines) = clean_block_lines(self.ascii_art_small) {
            creative.logos.small = lines;
        }
        if let Some(lines) = clean_list(self.route) {
            creative.route = lines;
        }
        if let Some(lines) = clean_list(self.links) {
            creative.links = lines;
        }
        if let Some(lines) = clean_list(self.stats) {
            creative.stats = lines;
        }

        creative
    }
}

fn idle_fullscreen_delay(creative: &AdCreative) -> Duration {
    Duration::from_secs(
        idle_fullscreen_delay_from_env().unwrap_or(creative.idle_fullscreen_seconds),
    )
}

fn idle_fullscreen_delay_from_env() -> Option<u64> {
    env::var(SPONSOR_IDLE_FULLSCREEN_SECONDS_ENV)
        .ok()
        .and_then(|value| parse_positive_seconds(&value))
}

fn parse_positive_seconds(value: &str) -> Option<u64> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|seconds| *seconds > 0)
}

fn clean_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| sanitize_terminal_text(&value).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_list(value: Option<Vec<String>>) -> Option<Vec<String>> {
    let lines: Vec<String> = value?
        .into_iter()
        .map(|line| sanitize_terminal_text(&line).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

fn clean_block_lines(value: Option<String>) -> Option<Vec<String>> {
    let mut lines: Vec<String> = value?
        .lines()
        .map(|line| sanitize_terminal_text(line).trim_end().to_string())
        .collect();
    // Blank lines inside the art are intentional spacing — keep them. Only
    // strip empty lines from the top and bottom of the block.
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

fn sanitize_terminal_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            let code = u32::from(*character);
            !character.is_control()
                && code != 0x061c
                && !matches!(code, 0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069)
        })
        .collect()
}

fn run_sponsor_pane() -> Result<()> {
    let _guard = PaneGuard::enter()?;
    let mut activity = harness::Activity::from_env();
    let mut stdout = io::stdout();
    let terminal_session = RemoteTerminalSessionGuard::start(&sponsor_wrapped_command());
    let mut ads_shown_this_session = 0_u64;
    let mut next_render_sequence = 1_u64;
    let mut last_reported_impression_at: Option<Instant> = None;
    let mut creative = load_active_ad_creative(
        Layout::current(),
        terminal_session.id(),
        None,
        ads_shown_this_session,
    )
    .unwrap_or_else(default_ad_creative);
    let mut last_loaded_at = ad_file_modified_at();
    let mut frame = 0_u64;
    let mut last_animation = Instant::now();
    let mut last_idle_check = Instant::now();
    let mut last_ad_check = Instant::now();
    let mut last_remote_check = Instant::now();
    let mut reported_ad_decision_ids = HashSet::new();
    let mut exhausted_ad_decision_ids = HashSet::new();
    let mut pending_impression_reports = Vec::new();
    let (impression_result_tx, impression_result_rx) = mpsc::channel();
    let mut hovered_ad_cell = None;
    let mut hovered_link = None;
    let mut fullscreen = false;
    let ad_while_working = ad_while_working_enabled();
    let mut idle_delay = idle_fullscreen_delay(&creative);
    let mut creative_visible_since = Instant::now();
    let mut last_cols = Layout::current().cols;
    let tmux = SponsorTmux::from_env();

    render_fullscreen_ad(
        &mut stdout,
        Layout::current(),
        frame,
        &creative,
        hovered_ad_cell,
        activity.as_ref(),
    )?;
    // The app pane may not exist yet (the launcher splits it right after this
    // process starts), so the initial fit happens in the loop below once it is.
    let mut pane_fitted = false;

    loop {
        if let Some(activity) = &mut activity {
            activity.poll();
        }
        while event::poll(Duration::from_millis(0)).unwrap_or(false) {
            match event::read().context("failed to read sponsor pane input")? {
                event::Event::Mouse(mouse) => {
                    let layout = Layout::current();
                    let next_hovered_link =
                        link_at_cell(&creative, layout, frame, mouse.row, mouse.column);
                    if next_hovered_link != hovered_link {
                        hovered_link = next_hovered_link.clone();
                        hovered_ad_cell = next_hovered_link.map(|_| (mouse.row, mouse.column));
                        render_fullscreen_ad(
                            &mut stdout,
                            layout,
                            frame,
                            &creative,
                            hovered_ad_cell,
                            activity.as_ref(),
                        )?;
                    }

                    if !is_mouse_click(mouse.kind) {
                        continue;
                    }

                    if let Some(link) =
                        link_at_cell(&creative, layout, frame, mouse.row, mouse.column)
                    {
                        let url = link_url(&link);
                        open_url(&url).ok();
                        report_click(&creative, &url);
                        render_fullscreen_ad(
                            &mut stdout,
                            layout,
                            frame,
                            &creative,
                            hovered_ad_cell,
                            activity.as_ref(),
                        )?;
                    } else if fullscreen {
                        if let Some(tmux) = &tmux {
                            tmux.collapse_sponsor_pane(ad_height(&creative, layout.cols))
                                .ok();
                            tmux.select_app_pane().ok();
                        }
                        fullscreen = false;
                    }
                }
                event::Event::Resize(_, _) => {
                    let layout = Layout::current();
                    render_fullscreen_ad(
                        &mut stdout,
                        layout,
                        frame,
                        &creative,
                        hovered_ad_cell,
                        activity.as_ref(),
                    )?;
                    // A width change can switch the art variant, so re-fit the
                    // pane to the ad. Height-only changes are left alone — the
                    // divider stays draggable.
                    if !fullscreen && layout.cols != last_cols {
                        last_cols = layout.cols;
                        if let Some(tmux) = &tmux {
                            tmux.fit_sponsor_pane(ad_height(&creative, layout.cols))
                                .ok();
                        }
                    }
                }
                event::Event::Key(key) => {
                    if let Some(tmux) = &tmux {
                        if fullscreen {
                            tmux.collapse_sponsor_pane(ad_height(
                                &creative,
                                Layout::current().cols,
                            ))
                            .ok();
                            fullscreen = false;
                        }
                        tmux.select_app_pane().ok();
                        if let Some(tmux_key) = key_to_tmux_send_key(key) {
                            tmux.send_key_to_app(tmux_key).ok();
                        }
                    }
                }
                _ => {}
            }
        }

        if last_idle_check.elapsed() >= Duration::from_secs(1) {
            if let Some(tmux) = &tmux {
                if !pane_fitted && tmux.app_pane_exists() {
                    tmux.fit_sponsor_pane(ad_height(&creative, Layout::current().cols))
                        .ok();
                    pane_fitted = true;
                }

                if !tmux.app_pane_exists() || tmux.app_pane_dead() {
                    tmux.kill_session().ok();
                    return Ok(());
                }

                // Lifecycle hints do not prove the app is safe to obscure. Even
                // stale/missing hooks, idle input and the legacy opt-in must not zoom it.
                if !fullscreen && activity.is_none() {
                    let client_idle = tmux.client_idle_for();
                    // Only query the app pane when the opt-in mode needs it.
                    let app_pane_idle = if ad_while_working {
                        tmux.app_pane_idle_for()
                    } else {
                        None
                    };
                    if should_expand_ad(client_idle, app_pane_idle, idle_delay, ad_while_working)
                        && tmux.expand_sponsor_pane().is_ok()
                    {
                        fullscreen = true;
                        hovered_ad_cell = None;
                        hovered_link = None;
                        render_fullscreen_ad(
                            &mut stdout,
                            Layout::current(),
                            frame,
                            &creative,
                            hovered_ad_cell,
                            activity.as_ref(),
                        )?;
                    }
                }
            }
            last_idle_check = Instant::now();
        }

        if last_ad_check.elapsed() >= Duration::from_secs(1) {
            let layout = Layout::current();
            pump_impression_reports(
                &mut pending_impression_reports,
                &mut reported_ad_decision_ids,
                &mut exhausted_ad_decision_ids,
                &impression_result_rx,
                &impression_result_tx,
            );
            if creative_visible_since.elapsed() >= MIN_RENDERED_VISIBLE_DURATION
                && enqueue_impression_if_needed(
                    &creative,
                    &reported_ad_decision_ids,
                    &exhausted_ad_decision_ids,
                    &mut pending_impression_reports,
                    layout,
                    &mut next_render_sequence,
                    creative_visible_since.elapsed().as_millis().min(86_400_000),
                )
            {
                ads_shown_this_session += 1;
                last_reported_impression_at = Some(Instant::now());
            }

            let modified_at = ad_file_modified_at();
            let should_poll_remote =
                remote_ads_enabled() && last_remote_check.elapsed() >= REMOTE_AD_REFRESH;
            if should_poll_remote {
                last_remote_check = Instant::now();
            }
            if modified_at != last_loaded_at || should_poll_remote {
                let seconds_since_last_ad =
                    last_reported_impression_at.map(|instant| instant.elapsed().as_secs());
                let next_creative = if modified_at != last_loaded_at {
                    last_loaded_at = modified_at;
                    load_active_ad_creative(
                        layout,
                        terminal_session.id(),
                        seconds_since_last_ad,
                        ads_shown_this_session,
                    )
                } else {
                    load_remote_ad_creative(
                        layout,
                        terminal_session.id(),
                        seconds_since_last_ad,
                        ads_shown_this_session,
                    )
                };

                if let Some(next_creative) = next_creative {
                    creative = next_creative;
                    creative_visible_since = Instant::now();
                    idle_delay = idle_fullscreen_delay(&creative);
                    hovered_ad_cell = None;
                    hovered_link = None;
                    render_fullscreen_ad(
                        &mut stdout,
                        layout,
                        frame,
                        &creative,
                        hovered_ad_cell,
                        activity.as_ref(),
                    )?;
                    // New creative, new height — snap the pane to it.
                    if !fullscreen {
                        if let Some(tmux) = &tmux {
                            tmux.fit_sponsor_pane(ad_height(&creative, layout.cols))
                                .ok();
                        }
                    }
                }
            }
            last_ad_check = Instant::now();
        }

        if last_animation.elapsed() >= ANIMATION_TICK {
            frame = frame.wrapping_add(1);
            render_fullscreen_ad(
                &mut stdout,
                Layout::current(),
                frame,
                &creative,
                hovered_ad_cell,
                activity.as_ref(),
            )?;
            last_animation = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_tmux_shell(
    command: &str,
    command_args: &[String],
    harness: Option<harness::Harness>,
) -> Result<i32> {
    ensure_tmux_available()?;

    let bridge = harness
        .map(|_| harness::BridgeDirectory::create())
        .transpose()
        .context("failed to create private harness hook channel")?;
    let socket_path = bridge.as_ref().map(harness::BridgeDirectory::socket_path);
    let lifecycle_environment = harness::environment(harness, socket_path.as_deref());

    let session = format!("sponsor-shell-{}", std::process::id());
    let _session_guard = TmuxSessionGuard::new(session.clone());
    let current_exe = env::current_exe().context("failed to locate sponsor-shell executable")?;
    let cwd = env::current_dir().context("failed to read current directory")?;
    let exit_file = env::temp_dir().join(format!("{session}.status"));
    let _ = fs::remove_file(&exit_file);
    let app_percent = if harness.is_some() {
        DEFAULT_APP_PANE_PERCENT.to_string()
    } else {
        app_pane_percent()
    };
    let wrapped_command = wrapped_command_label(command, command_args);
    let idle_fullscreen_seconds =
        idle_fullscreen_delay_from_env().map(|seconds| seconds.to_string());

    // Decided HERE, in the process the user actually launched, because this is
    // the only place the real answer exists (see SPONSOR_INTERACTIVE_ENV).
    let interactive = outer_terminal_is_interactive();

    let ad_command = sponsor_ad_command(
        &current_exe.to_string_lossy(),
        &session,
        &app_percent,
        &wrapped_command,
        idle_fullscreen_seconds.as_deref(),
        interactive,
    );
    let ad_command = format!("{} {ad_command}", shell_join(lifecycle_environment.clone()));
    let mut app_parts = lifecycle_environment;
    app_parts.push(command.to_string());
    app_parts.extend(command_args.iter().cloned());
    let app_command = sponsor_app_command(&exit_file.to_string_lossy(), &session, app_parts);

    run_tmux([
        "new-session",
        "-d",
        "-s",
        &session,
        "-n",
        "sponsor",
        "-c",
        &cwd.to_string_lossy(),
        &ad_command,
    ])?;
    run_tmux(["set-option", "-t", &session, "status", "off"])?;
    run_tmux(["set-option", "-t", &session, "mouse", "on"])?;
    run_tmux_allow_failure(["set-option", "-t", &session, "allow-passthrough", "on"]);
    run_tmux([
        "split-window",
        "-v",
        "-p",
        &app_percent,
        "-t",
        &session,
        "-c",
        &cwd.to_string_lossy(),
        &app_command,
    ])?;
    let kill_session_command = format!("kill-session -t {}", session);
    run_tmux_allow_failure([
        "set-hook",
        "-t",
        &session,
        "pane-died",
        &kill_session_command,
    ]);
    run_tmux_allow_failure([
        "set-hook",
        "-t",
        &session,
        "pane-exited",
        &kill_session_command,
    ]);
    run_tmux(["select-pane", "-t", &format!("{session}:0.1")])?;

    let attach_status = Command::new("tmux")
        .env_remove("TMUX")
        .args(["attach-session", "-t", &session])
        .stderr(Stdio::null())
        .status()
        .context("failed to attach tmux session")?;

    let exit_code = fs::read_to_string(&exit_file)
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
        .unwrap_or_else(|| attach_status.code().unwrap_or(1));
    let _ = fs::remove_file(exit_file);
    Ok(exit_code)
}

fn sponsor_app_command(exit_file: &str, session: &str, app_parts: Vec<String>) -> String {
    // tmux uses the user's default shell. In zsh, `status` is read-only.
    let exit_trap = format!(
        "sponsor_exit_code=$?; printf \"%s\\n\" \"$sponsor_exit_code\" > {}; tmux kill-session -t {}; exit \"$sponsor_exit_code\"",
        shell_quote(exit_file),
        shell_quote(session),
    );
    format!(
        "trap {} EXIT; {}",
        shell_quote(&exit_trap),
        shell_join(app_parts)
    )
}

fn sponsor_ad_command(
    current_exe: &str,
    session: &str,
    app_percent: &str,
    wrapped_command: &str,
    idle_fullscreen_seconds: Option<&str>,
    interactive: bool,
) -> String {
    let mut parts = vec![
        format!("{}={}", SPONSOR_SESSION_ENV, shell_quote(session)),
        format!("{}={}", SPONSOR_APP_PERCENT_ENV, shell_quote(app_percent)),
        format!(
            "{}={}",
            SPONSOR_WRAPPED_COMMAND_ENV,
            shell_quote(wrapped_command)
        ),
        // Injected inline for the same reason the vars above are: tmux does not
        // carry the invoking client's environment into a pane.
        format!(
            "{}={}",
            SPONSOR_INTERACTIVE_ENV,
            if interactive { "1" } else { "0" }
        ),
    ];
    if let Some(seconds) = idle_fullscreen_seconds {
        parts.push(format!(
            "{}={}",
            SPONSOR_IDLE_FULLSCREEN_SECONDS_ENV,
            shell_quote(seconds)
        ));
    }
    parts.push(shell_join([
        current_exe.to_string(),
        AD_PANE_ARG.to_string(),
    ]));
    parts.join(" ")
}

struct TmuxSessionGuard {
    session: String,
}

impl TmuxSessionGuard {
    fn new(session: String) -> Self {
        Self { session }
    }
}

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.session])
            .status();
    }
}

struct SponsorTmux {
    session: String,
}

impl SponsorTmux {
    fn from_env() -> Option<Self> {
        let session = env::var(SPONSOR_SESSION_ENV).ok()?;
        Some(Self { session })
    }

    fn expand_sponsor_pane(&self) -> Result<()> {
        self.select_sponsor_pane()?;
        if !self.window_zoomed() {
            self.tmux(["resize-pane", "-Z", "-t", &self.sponsor_target()])?;
        }
        Ok(())
    }

    fn collapse_sponsor_pane(&self, ad_rows: u16) -> Result<()> {
        if self.window_zoomed() {
            self.tmux(["resize-pane", "-Z", "-t", &self.sponsor_target()])?;
        }
        self.fit_sponsor_pane(ad_rows)
    }

    fn window_height(&self) -> u16 {
        self.tmux_output([
            "display-message",
            "-p",
            "-t",
            &self.session,
            "#{window_height}",
        ])
        .ok()
        .and_then(|raw| raw.trim().parse::<u16>().ok())
        .unwrap_or(24)
        .max(2)
    }

    fn window_zoomed(&self) -> bool {
        self.tmux_output([
            "display-message",
            "-p",
            "-t",
            &self.session,
            "#{window_zoomed_flag}",
        ])
        .ok()
        .is_some_and(|raw| raw.trim() == "1")
    }

    // Shrink the sponsor pane to exactly the ad's height so the app below gets
    // every remaining row — no dead space under the ad. Capped at half the
    // window so an oversized creative can't squeeze the app out.
    fn fit_sponsor_pane(&self, ad_rows: u16) -> Result<()> {
        let height = self.window_height();
        let protected = env::var(harness::HARNESS_ENV)
            .ok()
            .and_then(|value| harness::Harness::parse(&value))
            .is_some();
        let rows = sponsor_pane_rows(ad_rows, height, protected).to_string();
        self.tmux(["resize-pane", "-t", &self.sponsor_target(), "-y", &rows])
    }

    fn select_app_pane(&self) -> Result<()> {
        self.tmux(["select-pane", "-t", &self.app_target()])
    }

    fn select_sponsor_pane(&self) -> Result<()> {
        self.tmux(["select-pane", "-t", &self.sponsor_target()])
    }

    fn send_key_to_app(&self, key: &str) -> Result<()> {
        self.tmux(["send-keys", "-t", &self.app_target(), key])
    }

    fn app_pane_exists(&self) -> bool {
        self.tmux_output([
            "display-message",
            "-p",
            "-t",
            &self.app_target(),
            "#{pane_id}",
        ])
        .is_ok()
    }

    fn app_pane_dead(&self) -> bool {
        self.tmux_output([
            "display-message",
            "-p",
            "-t",
            &self.app_target(),
            "#{pane_dead}",
        ])
        .ok()
        .is_some_and(|output| output.trim() == "1")
    }

    // Time since the wrapped app pane last produced output. Small values mean the
    // harness is actively working (a spinner or a streaming response).
    fn app_pane_idle_for(&self) -> Option<Duration> {
        let output = self
            .tmux_output([
                "display-message",
                "-p",
                "-t",
                &self.app_target(),
                "#{pane_activity}",
            ])
            .ok()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        client_idle_from_activity_output(&output, now)
    }

    fn kill_session(&self) -> Result<()> {
        self.tmux(["kill-session", "-t", &self.session])
    }

    fn client_idle_for(&self) -> Option<Duration> {
        let output = self
            .tmux_output([
                "list-clients",
                "-t",
                &self.session,
                "-F",
                "#{client_activity}",
            ])
            .ok()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        client_idle_from_activity_output(&output, now)
    }

    fn sponsor_target(&self) -> String {
        format!("{}:0.0", self.session)
    }

    fn app_target(&self) -> String {
        format!("{}:0.1", self.session)
    }

    fn tmux<const N: usize>(&self, args: [&str; N]) -> Result<()> {
        run_tmux(args)
    }

    fn tmux_output<const N: usize>(&self, args: [&str; N]) -> Result<String> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .context("failed to query tmux")?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            anyhow::bail!("tmux query failed with status {}", output.status)
        }
    }
}

fn sponsor_pane_rows(ad_rows: u16, height: u16, protected: bool) -> u16 {
    let divisor = if protected { 4 } else { 2 };
    ad_rows.clamp(2, (height / divisor).max(2))
}

fn ad_while_working_enabled() -> bool {
    env::var(SPONSOR_AD_WHILE_WORKING_ENV)
        .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Decide whether to expand the sponsor pane to a full-screen ad.
///
/// - Classic trigger: the user has been idle at least `idle_delay` (they stepped
///   away).
/// - `ad_while_working` trigger (opt-in): the wrapped harness is producing output
///   (app pane active within the recent window) while the user is not typing —
///   i.e. a "loading"/streaming state. This is the harness-agnostic realization
///   of "show an ad while the tool is loading".
fn should_expand_ad(
    client_idle: Option<Duration>,
    app_pane_idle: Option<Duration>,
    idle_delay: Duration,
    ad_while_working: bool,
) -> bool {
    if client_idle.is_some_and(|idle| idle >= idle_delay) {
        return true;
    }
    if ad_while_working {
        let user_waiting = client_idle.is_some_and(|idle| idle >= AD_WHILE_WORKING_USER_IDLE);
        let harness_working =
            app_pane_idle.is_some_and(|idle| idle <= AD_WHILE_WORKING_ACTIVITY_WINDOW);
        return user_waiting && harness_working;
    }
    false
}

fn client_idle_from_activity_output(output: &str, now_epoch_seconds: u64) -> Option<Duration> {
    output
        .lines()
        .filter_map(|line| {
            let activity_epoch = line.trim().parse::<u64>().ok()?;
            if activity_epoch == 0 {
                return None;
            }
            now_epoch_seconds
                .checked_sub(activity_epoch)
                .map(Duration::from_secs)
        })
        .min()
}

fn key_to_tmux_send_key(key: KeyEvent) -> Option<&'static str> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }

    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some("C-c")
        }
        _ => None,
    }
}

fn ensure_tmux_available() -> Result<()> {
    if tmux_available() {
        return Ok(());
    }

    anyhow::bail!(tmux_missing_message())
}

fn tmux_missing_message() -> &'static str {
    "tmux is required for Sponsor Shell and was not found. Install tmux manually, or run \
     `sponsor-shell install-tmux` to explicitly let Sponsor Shell invoke a supported package manager."
}

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run_install_tmux(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("sponsor-shell install-tmux");
        println!("explicitly installs tmux using Homebrew or a supported Linux package manager");
        return Ok(0);
    }
    if let Some(option) = args.first() {
        anyhow::bail!("unknown install-tmux option: {option}");
    }
    if tmux_available() {
        println!("tmux is already installed");
        return Ok(0);
    }

    install_tmux()?;
    if tmux_available() {
        println!("tmux installed successfully");
        Ok(0)
    } else {
        anyhow::bail!("tmux installation completed, but `tmux -V` still failed")
    }
}

fn install_tmux() -> Result<()> {
    if cfg!(target_os = "macos") {
        eprintln!("sponsor-shell: explicitly installing tmux with Homebrew...");
        let status = Command::new("brew")
            .args(["install", "tmux"])
            .status()
            .context("tmux is required, and Homebrew was not found to install it")?;
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("Homebrew failed to install tmux with status {status}")
        }
    } else if cfg!(target_os = "linux") {
        install_tmux_linux()
    } else {
        anyhow::bail!("tmux is required for sponsor-shell split-pane mode. Install tmux and retry.")
    }
}

/// Best-effort tmux install across common Linux package managers. Uses sudo when
/// not already root and sudo is available. Returns an error listing the manual
/// command if no supported manager succeeds.
fn install_tmux_linux() -> Result<()> {
    let use_sudo = !running_as_root() && command_exists("sudo");
    // (manager, install args). Ordered by prevalence.
    let managers: [(&str, &[&str]); 6] = [
        ("apt-get", &["install", "-y", "tmux"]),
        ("dnf", &["install", "-y", "tmux"]),
        ("yum", &["install", "-y", "tmux"]),
        ("pacman", &["-S", "--noconfirm", "tmux"]),
        ("zypper", &["--non-interactive", "install", "tmux"]),
        ("apk", &["add", "tmux"]),
    ];

    let mut attempted = false;
    for (manager, args) in managers {
        if !command_exists(manager) {
            continue;
        }
        attempted = true;
        eprintln!("sponsor-shell: explicitly installing tmux with {manager}...");
        let mut command = if use_sudo {
            let mut c = Command::new("sudo");
            c.arg(manager);
            c
        } else {
            Command::new(manager)
        };
        command.args(args);
        match command.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                eprintln!("sponsor-shell: {manager} exited with {status}; trying the next manager")
            }
            Err(error) => eprintln!("sponsor-shell: could not run {manager}: {error}"),
        }
    }

    if attempted {
        anyhow::bail!("automatic tmux installation failed; install tmux manually and retry")
    } else {
        anyhow::bail!(
            "tmux is required and no supported package manager (apt-get, dnf, yum, \
             pacman, zypper, apk) was found. Install tmux manually and retry."
        )
    }
}

fn running_as_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|value| value.trim() == "0")
}

fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run_tmux<const N: usize>(args: [&str; N]) -> Result<()> {
    let status = Command::new("tmux")
        .args(args)
        .status()
        .context("failed to run tmux")?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("tmux command failed with status {status}");
    }
}

fn run_tmux_allow_failure<const N: usize>(args: [&str; N]) {
    let _ = Command::new("tmux").args(args).status();
}

fn app_pane_percent() -> String {
    env::var("SPONSOR_SHELL_APP_PANE_PERCENT")
        .ok()
        .filter(|raw| {
            raw.parse::<u8>()
                .is_ok_and(|value| (20..=95).contains(&value))
        })
        .unwrap_or_else(|| DEFAULT_APP_PANE_PERCENT.to_string())
}

fn shell_join(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .map(|part| shell_quote(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_./:=+".contains(&byte))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn is_mouse_click(kind: MouseEventKind) -> bool {
    matches!(kind, MouseEventKind::Down(_))
}

fn write_ad_line(
    stdout: &mut io::Stdout,
    line: &str,
    creative: &AdCreative,
    hovered_col: Option<usize>,
) -> Result<()> {
    stdout.write_all(linkified_line(line, creative, hovered_col).as_bytes())?;
    Ok(())
}

#[cfg(test)]
fn ad_link_at(
    creative: &AdCreative,
    layout: Layout,
    frame: u64,
    row: u16,
    col: u16,
) -> Option<String> {
    link_at_cell(creative, layout, frame, row, col).map(|link| link_url(&link))
}

fn link_at_cell(
    creative: &AdCreative,
    layout: Layout,
    frame: u64,
    row: u16,
    col: u16,
) -> Option<String> {
    let lines = ad_lines(creative, layout.cols, layout.rows, frame);
    let line = lines.get(usize::from(row))?;
    link_at_column(line, creative, usize::from(col))
}

// Only schema-validated, explicitly declared destinations are clickable.
// Longest-first so nested declared links resolve to the longer span.
fn link_candidates(_line: &str, creative: &AdCreative) -> Vec<String> {
    let mut candidates = creative.links.clone();
    candidates.push(creative.url.clone());
    candidates.retain(|candidate| !candidate.is_empty());
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.len()));
    candidates.dedup();
    candidates
}

fn link_at_column(line: &str, creative: &AdCreative, col: usize) -> Option<String> {
    for candidate in link_candidates(line, creative) {
        let mut search_start = 0;
        while search_start < line.len() {
            let Some(position) = line[search_start..].find(&candidate) else {
                break;
            };
            let start = search_start + position;
            let end = start + candidate.len();
            if (start..end).contains(&col) {
                return Some(candidate);
            }
            search_start = end;
        }
    }

    None
}

fn open_url(url: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    };

    let mut command = Command::new(opener);
    if cfg!(target_os = "windows") {
        command.args(["/C", "start", "", url]);
    } else {
        command.arg(url);
    }

    command
        .spawn()
        .with_context(|| format!("failed to open {url}"))?;
    Ok(())
}

fn linkified_line(line: &str, creative: &AdCreative, hovered_col: Option<usize>) -> String {
    let candidates = link_candidates(line, creative);

    let mut output = String::new();
    let mut offset = 0;
    while offset < line.len() {
        let next = candidates
            .iter()
            .filter_map(|candidate| {
                line[offset..]
                    .find(candidate)
                    .map(|position| (offset + position, candidate))
            })
            .min_by(|(left_position, left), (right_position, right)| {
                left_position
                    .cmp(right_position)
                    .then_with(|| right.len().cmp(&left.len()))
            });

        let Some((position, candidate)) = next else {
            output.push_str(&line[offset..]);
            break;
        };

        output.push_str(&line[offset..position]);
        let end = position + candidate.len();
        let is_hovered = hovered_col.is_some_and(|col| (position..end).contains(&col));
        output.push_str(&terminal_hyperlink(
            candidate,
            &link_url(candidate),
            is_hovered,
        ));
        offset = position + candidate.len();
    }

    output
}

fn terminal_hyperlink(label: &str, url: &str, hovered: bool) -> String {
    let label = sanitize_terminal_text(label);
    let url = canonical_https_url(url);
    let body = if hovered {
        format!("\x1b[1m\x1b[4m{label}\x1b[24m\x1b[22m")
    } else {
        label.to_string()
    };
    format!("\x1b]8;;{url}\x1b\\{body}\x1b]8;;\x1b\\")
}

fn link_url(link: &str) -> String {
    canonical_https_url(link)
}

fn canonical_https_url(value: &str) -> String {
    let safe = sanitize_terminal_text(value);
    let trimmed = safe.trim();
    if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("https://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        format!("https://{trimmed}")
    }
}

fn render_fullscreen_ad(
    stdout: &mut io::Stdout,
    layout: Layout,
    frame: u64,
    creative: &AdCreative,
    hovered_cell: Option<(u16, u16)>,
    activity: Option<&harness::Activity>,
) -> Result<()> {
    queue!(stdout, cursor::Hide, terminal::Clear(ClearType::All))?;
    let mut lines = ad_lines(creative, layout.cols, layout.rows, frame);
    if let Some(activity) = activity {
        add_activity_footer(&mut lines, layout.cols, &activity.label());
    }
    for row in 0..layout.rows {
        queue!(
            stdout,
            cursor::MoveTo(0, row),
            style::ResetColor,
            terminal::Clear(ClearType::CurrentLine)
        )?;
        if let Some(line) = lines.get(usize::from(row)) {
            let hovered_col = hovered_cell
                .filter(|(hovered_row, _)| *hovered_row == row)
                .map(|(_, hovered_col)| usize::from(hovered_col));
            write_ad_line(stdout, line, creative, hovered_col)?;
        }
    }
    stdout.flush()?;
    Ok(())
}

fn add_activity_footer(lines: &mut [String], cols: u16, label: &str) {
    // Replace only the non-clickable closing border, never an ad body or app row.
    if lines.len() > 1 {
        if let Some(last) = lines.last_mut() {
            *last = inside_line(usize::from(cols), label);
        }
    }
}

// The ad is a box that hugs its art: the variant is picked by the horizontal
// space available (large art when it fits, small otherwise) and the box is
// exactly as tall as the art — never padded with empty rows.
fn ad_lines(creative: &AdCreative, cols: u16, rows: u16, _frame: u64) -> Vec<String> {
    let width = usize::from(cols);
    if rows == 0 {
        return Vec::new();
    }
    if rows == 1 {
        return vec![border_line(width)];
    }

    // Borders take 2 columns; the art is drawn one space in from the left.
    let inner_width = width.saturating_sub(2);
    let body = ad_body_lines(creative, inner_width);
    if should_render_expanded_ad(body.len(), width, rows) {
        return expanded_ad_lines(creative, width, rows);
    }
    let body_rows = body.len().min(usize::from(rows).saturating_sub(2));

    let mut lines = Vec::with_capacity(body_rows + 2);
    lines.push(border_line(width));
    for line in body.iter().take(body_rows) {
        lines.push(inside_line(width, line));
    }
    lines.push(border_line(width));
    lines
}

fn should_render_expanded_ad(compact_body_rows: usize, width: usize, rows: u16) -> bool {
    width >= 72 && rows >= 22 && usize::from(rows) > compact_body_rows.saturating_add(8)
}

fn expanded_ad_lines(creative: &AdCreative, width: usize, rows: u16) -> Vec<String> {
    let inner_width = width.saturating_sub(2);
    let body_rows = usize::from(rows).saturating_sub(2);
    let mut body = expanded_ad_body_lines(creative, inner_width, body_rows);
    body.truncate(body_rows);
    while body.len() < body_rows {
        body.push(String::new());
    }

    let mut lines = Vec::with_capacity(usize::from(rows));
    lines.push(border_line(width));
    lines.extend(body.iter().map(|line| inside_line(width, line)));
    lines.push(border_line(width));
    lines
}

fn expanded_ad_body_lines(
    creative: &AdCreative,
    inner_width: usize,
    body_rows: usize,
) -> Vec<String> {
    let content_width = inner_width.saturating_sub(2).max(1);
    let panel_width = content_width.clamp(1, 96);
    let mut header = wrap_text(
        &format!(
            "{} / Sponsored terminal time / {}",
            creative.sponsor, creative.url
        ),
        content_width,
        2,
    );
    header.extend(route_timeline_lines(creative, content_width));

    let art = center_block(pick_art(creative, content_width), content_width);
    let mut pitch = Vec::new();
    pitch.extend(wrap_text(
        &creative.headline,
        panel_width.saturating_sub(4),
        3,
    ));
    pitch.extend(wrap_text(
        &creative.subheadline,
        panel_width.saturating_sub(4),
        4,
    ));
    pitch.extend(wrap_text(
        &format!("> {}", creative.cta),
        panel_width.saturating_sub(4),
        2,
    ));
    pitch.extend(wrap_text(
        &format!("[{}]", creative.disclosure),
        panel_width.saturating_sub(4),
        2,
    ));
    let pitch_panel = center_block(&panel_lines("campaign", &pitch, panel_width), content_width);
    let detail_panel = center_block(
        &panel_lines(
            "terminal inventory",
            &structured_creative_lines(creative, panel_width.saturating_sub(4)),
            panel_width,
        ),
        content_width,
    );
    let footer = footer_lines(creative, content_width);

    distribute_sections(
        vec![header, art, pitch_panel, detail_panel, footer],
        body_rows,
    )
    .into_iter()
    .map(|line| format!(" {line}"))
    .collect()
}

fn panel_lines(title: &str, body: &[String], width: usize) -> Vec<String> {
    if width < 12 {
        return body.to_vec();
    }

    let inner_width = width.saturating_sub(4);
    let title = format!(" {} ", title);
    let top = if title.chars().count() + 2 < width {
        format!(
            "+{}{}+",
            title,
            "-".repeat(width.saturating_sub(title.chars().count() + 2))
        )
    } else {
        border_line(width)
    };
    let mut lines = vec![top];
    for line in body {
        let clipped = clip_ascii(line, inner_width);
        lines.push(format!("| {clipped:<inner_width$} |"));
    }
    lines.push(border_line(width));
    lines
}

fn route_timeline_lines(creative: &AdCreative, width: usize) -> Vec<String> {
    if creative.route.is_empty() {
        return Vec::new();
    }
    wrap_text(
        &format!("flow: {}", creative.route.join(" ----> ")),
        width,
        2,
    )
}

fn footer_lines(creative: &AdCreative, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("-".repeat(width.min(96)));
    lines.extend(wrap_text(&creative.cta, width, 2));
    lines.extend(wrap_text(
        &format!("links: {}", creative.links.join("  ")),
        width,
        3,
    ));
    lines
}

fn distribute_sections(sections: Vec<Vec<String>>, rows: usize) -> Vec<String> {
    let sections: Vec<Vec<String>> = sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect();
    let content_rows: usize = sections.iter().map(Vec::len).sum();
    if rows <= content_rows {
        return sections.into_iter().flatten().take(rows).collect();
    }

    let gaps = sections.len().saturating_add(1).max(1);
    let extra_rows = rows - content_rows;
    let base_gap = extra_rows / gaps;
    let remainder = extra_rows % gaps;
    let mut lines = Vec::with_capacity(rows);

    for gap_index in 0..=sections.len() {
        let gap_rows = base_gap + usize::from(gap_index < remainder);
        lines.extend(std::iter::repeat_n(String::new(), gap_rows));
        if let Some(section) = sections.get(gap_index) {
            lines.extend(section.iter().cloned());
        }
    }

    lines.truncate(rows);
    lines
}

fn center_block(lines: &[String], width: usize) -> Vec<String> {
    let block_width = logo_width(lines);
    let padding = width.saturating_sub(block_width) / 2;
    lines
        .iter()
        .map(|line| {
            if block_width >= width {
                line.to_string()
            } else {
                format!("{}{}", " ".repeat(padding), line)
            }
        })
        .collect()
}

fn structured_creative_lines(creative: &AdCreative, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    append_labeled_lines(&mut lines, "pipeline", &creative.route, " -> ", width, 2);
    append_labeled_lines(&mut lines, "inventory", &creative.stats, " / ", width, 2);
    append_labeled_lines(&mut lines, "links", &creative.links, "  ", width, 3);
    lines
}

fn append_labeled_lines(
    lines: &mut Vec<String>,
    label: &str,
    values: &[String],
    separator: &str,
    width: usize,
    max_lines: usize,
) {
    if values.is_empty() {
        return;
    }
    lines.extend(wrap_text(
        &format!("{label}: {}", values.join(separator)),
        width,
        max_lines,
    ));
}

// Rows the ad needs at this width: the chosen art, creative copy, and borders.
fn ad_height(creative: &AdCreative, cols: u16) -> u16 {
    let inner_width = usize::from(cols).saturating_sub(2);
    let body = ad_body_lines(creative, inner_width);
    u16::try_from(body.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
}

fn ad_body_lines(creative: &AdCreative, inner_width: usize) -> Vec<String> {
    let content_width = inner_width.saturating_sub(1).max(1);
    let art = pick_art(creative, content_width);
    let mut lines: Vec<String> = art.iter().map(|line| format!(" {line}")).collect();
    let copy = creative_copy_lines(creative, content_width);
    if !copy.is_empty() {
        lines.push(String::new());
        lines.extend(copy.into_iter().map(|line| format!(" {line}")));
    }
    lines
}

fn creative_copy_lines(creative: &AdCreative, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    lines.extend(wrap_text(
        &format!("{} / {}", creative.sponsor, creative.url),
        width,
        2,
    ));
    lines.extend(wrap_text(&creative.headline, width, 3));
    lines.extend(wrap_text(&creative.subheadline, width, 3));
    lines.extend(wrap_text(&format!("> {}", creative.cta), width, 3));
    lines.extend(wrap_text(&format!("[{}]", creative.disclosure), width, 2));
    let structured = structured_creative_lines(creative, width);
    if !structured.is_empty() {
        lines.push(String::new());
        lines.extend(structured);
    }
    lines
}

fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in normalized.split(' ') {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }

        while current.chars().count() > width {
            let split_at = current
                .char_indices()
                .nth(width)
                .map(|(idx, _)| idx)
                .unwrap_or(current.len());
            let rest = current[split_at..].to_string();
            lines.push(current[..split_at].to_string());
            current = rest;
        }

        if lines.len() >= max_lines {
            break;
        }
    }

    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }

    lines.truncate(max_lines);
    lines
}

fn pick_art(creative: &AdCreative, available_cols: usize) -> &[String] {
    let large = &creative.logos.large;
    let small = &creative.logos.small;
    if logo_width(large) <= available_cols || small.is_empty() {
        large
    } else {
        small
    }
}

fn border_line(width: usize) -> String {
    match width {
        0 => String::new(),
        1 => "+".to_string(),
        _ => format!("+{}+", "-".repeat(width.saturating_sub(2))),
    }
}

// Widest art line in display columns (chars, not bytes — block glyphs are
// multi-byte but occupy one column).
fn logo_width<T: AsRef<str>>(logo: &[T]) -> usize {
    logo.iter()
        .map(|line| line.as_ref().chars().count())
        .max()
        .unwrap_or_default()
}

fn inside_line(width: usize, content: &str) -> String {
    match width {
        0 => String::new(),
        1 => "|".to_string(),
        _ => {
            let inner_width = width.saturating_sub(2);
            let clipped = clip_ascii(content, inner_width);
            format!("|{clipped:<inner_width$}|")
        }
    }
}

// Clip to at most `width` characters, never splitting a multi-byte char.
fn clip_ascii(text: &str, width: usize) -> &str {
    match text.char_indices().nth(width) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn protected_panes_reserve_three_quarters_for_the_harness() {
        assert_eq!(super::sponsor_pane_rows(100, 40, true), 10);
        assert_eq!(super::sponsor_pane_rows(100, 40, false), 20);
        assert_eq!(super::sponsor_pane_rows(4, 40, true), 4);
        for rows in 0..12 {
            assert!((2..=3).contains(&super::sponsor_pane_rows(100, rows, true)));
        }
    }

    #[test]
    fn activity_footer_preserves_body_and_stays_inside_narrow_panes() {
        let creative = super::railway_example_creative();
        for cols in 0..120 {
            for rows in 0..45 {
                let mut lines = super::ad_lines(&creative, cols, rows, 0);
                let original = lines.clone();
                super::add_activity_footer(
                    &mut lines,
                    cols,
                    "Claude | recent hook: permission requested",
                );
                assert_eq!(lines.len(), original.len());
                if lines.len() > 1 {
                    assert_eq!(lines[..lines.len() - 1], original[..original.len() - 1]);
                    assert!(lines.last().unwrap().chars().count() <= usize::from(cols));
                    for col in 0..cols {
                        assert!(super::link_at_column(
                            lines.last().unwrap(),
                            &creative,
                            usize::from(col)
                        )
                        .is_none());
                    }
                } else {
                    assert_eq!(lines, original);
                }
            }
        }
    }

    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_process_environment() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn serve_one_http_request(listener: TcpListener, status: &str, response_body: &str) -> String {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let mut request = Vec::new();
        let mut expected_length = None;
        loop {
            let mut chunk = [0_u8; 1024];
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);

            if expected_length.is_none() {
                if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    expected_length = Some(header_end + 4 + content_length);
                }
            }

            if expected_length.is_some_and(|length| request.len() >= length) {
                break;
            }
            assert!(request.len() < 64 * 1024, "test request exceeded 64 KiB");
        }

        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    }

    #[test]
    fn app_exit_trap_preserves_failure_in_bash_and_zsh() {
        for shell in ["/bin/bash", "/bin/zsh"] {
            if !std::path::Path::new(shell).exists() {
                continue;
            }
            let exit_file = env::temp_dir().join(format!(
                "sponsor exit 'quoted'-{}-{}.status",
                std::process::id(),
                shell.rsplit('/').next().unwrap()
            ));
            let command = sponsor_app_command(
                &exit_file.to_string_lossy(),
                "test-session",
                vec!["/bin/sh".into(), "-c".into(), "exit 37".into()],
            );
            let output = Command::new(shell)
                .args(["-c", &format!("tmux() {{ :; }}; {command}")])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(37), "{shell}: {output:?}");
            assert!(output.stderr.is_empty(), "{shell}: {output:?}");
            assert_eq!(fs::read_to_string(&exit_file).unwrap(), "37\n");
            fs::remove_file(exit_file).unwrap();
        }
    }

    #[test]
    fn ad_expands_on_long_user_idle_regardless_of_mode() {
        let delay = Duration::from_secs(30);
        // User idle past the delay → expand, whether or not the working mode is on.
        assert!(should_expand_ad(
            Some(Duration::from_secs(31)),
            None,
            delay,
            false
        ));
        assert!(should_expand_ad(
            Some(Duration::from_secs(31)),
            None,
            delay,
            true
        ));
        // Below the delay, with working mode off → do not expand.
        assert!(!should_expand_ad(
            Some(Duration::from_secs(5)),
            None,
            delay,
            false
        ));
    }

    #[test]
    fn ad_while_working_expands_during_a_loading_state_only() {
        let delay = Duration::from_secs(30);
        // Harness working (app pane active 1s ago) + user waiting (idle 3s) → expand.
        assert!(should_expand_ad(
            Some(Duration::from_secs(3)),
            Some(Duration::from_secs(1)),
            delay,
            true,
        ));
        // User actively typing (idle 0s) → not a loading state, do not expand.
        assert!(!should_expand_ad(
            Some(Duration::from_secs(0)),
            Some(Duration::from_secs(1)),
            delay,
            true,
        ));
        // Harness not producing output (app pane idle 10s) → not loading.
        assert!(!should_expand_ad(
            Some(Duration::from_secs(3)),
            Some(Duration::from_secs(10)),
            delay,
            true,
        ));
        // Same loading signals but the opt-in mode is OFF → do not expand.
        assert!(!should_expand_ad(
            Some(Duration::from_secs(3)),
            Some(Duration::from_secs(1)),
            delay,
            false,
        ));
    }

    #[test]
    fn api_base_url_requires_https_except_for_local_development() {
        assert!(validate_api_base_url("http://moneymux.com").is_err());
        assert!(validate_api_base_url("http://203.0.113.10:4000/api").is_err());

        assert_eq!(
            validate_api_base_url("http://localhost:4000/").unwrap(),
            "http://localhost:4000"
        );
        assert!(validate_api_base_url("http://127.0.0.1:4000").is_ok());
        assert!(validate_api_base_url("http://[::1]:4000").is_ok());
        assert!(validate_api_base_url("http://app.localhost").is_ok());
        assert!(validate_api_base_url("https://moneymux.com").is_ok());
    }

    #[test]
    fn api_base_url_rejects_credential_and_url_smuggling_fields() {
        assert!(validate_api_base_url("https://user:secret@moneymux.com").is_err());
        assert!(validate_api_base_url("https://moneymux.com?target=evil").is_err());
        assert!(validate_api_base_url("https://moneymux.com#fragment").is_err());
        assert!(validate_api_base_url("file:///tmp/fake-api").is_err());
    }

    #[test]
    fn api_post_sends_json_and_bearer_credentials() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/api/events/impression",
            listener.local_addr().unwrap()
        );
        let server =
            thread::spawn(move || serve_one_http_request(listener, "200 OK", r#"{"ok":true}"#));

        let response = api_post_with_token(
            &url,
            r#"{"adDecisionId":"decision-1"}"#,
            Some("ssdev_test_token"),
        )
        .unwrap();
        let request = server.join().unwrap();
        let request_lowercase = request.to_ascii_lowercase();

        assert_eq!(response, r#"{"ok":true}"#);
        assert!(request.starts_with("POST /api/events/impression HTTP/1.1\r\n"));
        assert!(request_lowercase.contains("\r\ncontent-type: application/json\r\n"));
        assert!(request_lowercase.contains("\r\nauthorization: bearer ssdev_test_token\r\n"));
        assert!(request.ends_with(r#"{"adDecisionId":"decision-1"}"#));
    }

    #[test]
    fn api_post_rejects_non_success_statuses() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/api/events/impression",
            listener.local_addr().unwrap()
        );
        let server = thread::spawn(move || {
            serve_one_http_request(listener, "401 Unauthorized", r#"{"error":"unauthorized"}"#)
        });

        let result = api_post_with_token(&url, "{}", None);
        server.join().unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn unlink_removes_credentials_but_preserves_the_api_url() {
        let mut config = CliConfig {
            api_base_url: Some("https://staging.moneymux.com".to_string()),
            device_id: Some("device_1".to_string()),
            device_token: Some("ssdev_secret".to_string()),
        };

        assert!(clear_device_credentials(&mut config));
        assert_eq!(
            config.api_base_url.as_deref(),
            Some("https://staging.moneymux.com")
        );
        assert!(config.device_id.is_none());
        assert!(config.device_token.is_none());
        assert!(!clear_device_credentials(&mut config));

        let serialized = serde_json::to_string(&config).unwrap();
        assert!(!serialized.contains("ssdev_secret"));
        assert!(!serialized.contains("device_1"));
    }

    #[test]
    fn doctor_output_names_credential_sources_without_values() {
        let snapshot = DoctorSnapshot {
            api_base_url: "https://staging.moneymux.com".to_string(),
            api_transport: "https",
            config_state: "valid",
            device_state: "linked",
            tmux_available: true,
            interactive: true,
            credential_overrides: vec![SPONSOR_DEVICE_TOKEN_ENV],
        };
        let output = doctor_lines(&snapshot).join("\n");

        assert!(output.contains(SPONSOR_DEVICE_TOKEN_ENV));
        assert!(output.contains("device: linked"));
        assert!(!output.contains("ssdev_secret"));
        assert!(!output.contains("config.json"));
    }

    #[cfg(unix)]
    #[test]
    fn saved_config_is_owner_only() {
        let _environment = lock_process_environment();
        use std::os::unix::fs::PermissionsExt;
        let dir = env::temp_dir().join(format!("sponsor-shell-cfgtest-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("config.json");
        // Pre-create with loose perms to prove we tighten an existing file too.
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let prev = env::var(SPONSOR_CONFIG_ENV).ok();
        env::set_var(SPONSOR_CONFIG_ENV, &path);
        let config = CliConfig {
            api_base_url: Some("https://moneymux.com".to_string()),
            device_id: Some("device_1".to_string()),
            device_token: Some("ssdev_secret".to_string()),
        };
        save_cli_config(&config).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "config must be owner-only");
        match prev {
            Some(value) => env::set_var(SPONSOR_CONFIG_ENV, value),
            None => env::remove_var(SPONSOR_CONFIG_ENV),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsigned_impressions_omit_the_decision_token_and_exhaust_only_once() {
        let mut creative = railway_example_creative();
        creative.ad_decision_id = Some("decision-1".to_string());
        let mut pending = Vec::new();
        let reported = HashSet::new();
        let mut exhausted = HashSet::new();
        let mut sequence = 1;

        assert!(enqueue_impression_if_needed(
            &creative,
            &reported,
            &exhausted,
            &mut pending,
            Layout::new(80, 24),
            &mut sequence,
            1_000,
        ));
        let body: serde_json::Value = serde_json::from_str(&pending[0].body).unwrap();
        assert!(body.get("decisionToken").is_none());

        pending[0].created_at = Instant::now() - IMPRESSION_REPORT_RETRY_WINDOW;
        let (_result_tx, result_rx) = mpsc::channel();
        let (send_tx, _send_rx) = mpsc::channel();
        let mut reported = HashSet::new();
        pump_impression_reports(
            &mut pending,
            &mut reported,
            &mut exhausted,
            &result_rx,
            &send_tx,
        );
        assert!(pending.is_empty());
        assert!(exhausted.contains("decision-1"));
        assert!(!enqueue_impression_if_needed(
            &creative,
            &reported,
            &exhausted,
            &mut pending,
            Layout::new(80, 24),
            &mut sequence,
            1_000,
        ));
    }

    #[test]
    fn impression_retry_backoff_is_exponential_and_capped() {
        assert_eq!(impression_retry_delay(0), Duration::from_secs(1));
        assert_eq!(impression_retry_delay(1), Duration::from_secs(2));
        assert_eq!(impression_retry_delay(4), Duration::from_secs(16));
        assert_eq!(impression_retry_delay(5), Duration::from_secs(30));
        assert_eq!(impression_retry_delay(99), Duration::from_secs(30));
    }

    #[test]
    fn remote_decisions_deserialize_signed_tokens() {
        let decision: RemoteAdDecision = serde_json::from_str(
            r#"{"adDecisionId":"decision-1","decisionToken":"123.signature","creative":null}"#,
        )
        .unwrap();
        assert_eq!(decision.ad_decision_id.as_deref(), Some("decision-1"));
        assert_eq!(decision.decision_token.as_deref(), Some("123.signature"));
    }

    #[test]
    fn strips_terminal_controls_and_direction_overrides() {
        let unsafe_text = "safe\u{1b}]8;;https://evil.example\u{7}click\u{202e}hidden";
        assert_eq!(
            sanitize_terminal_text(unsafe_text),
            "safe]8;;https://evil.exampleclickhidden"
        );
    }

    #[test]
    fn hyperlinks_are_https_and_cannot_nest_terminal_sequences() {
        let rendered = terminal_hyperlink(
            "click\u{1b}]8;;https://evil.example\u{7}",
            "http://example.com\u{1b}]8;;https://evil.example",
            false,
        );
        assert!(rendered.starts_with("\u{1b}]8;;https://example.com]8;;https://evil.example"));
        assert!(!rendered.contains("\u{7}"));
        assert_eq!(rendered.matches("\u{1b}]8;;").count(), 2);
    }

    #[test]
    fn multibyte_art_renders_without_panicking() {
        // Box-drawing glyphs and an ellipsis are multi-byte UTF-8; clipping them
        // at a byte offset used to panic on a non-char boundary.
        let creative = LocalAdCreative {
            enabled: Some(true),
            sponsor: Some("BØX".to_string()),
            url: None,
            headline: None,
            subheadline: None,
            cta: None,
            disclosure: None,
            idle_fullscreen_seconds: None,
            ascii_art: Some(
                "╔══════════════╗\n║  café … déjà ║\n║ ▓▓▒▒░░ ✓ ✗ ★ ║\n╚══════════════╝"
                    .to_string(),
            ),
            ascii_art_small: None,
            route: None,
            links: None,
            stats: None,
        }
        .into_ad_creative();

        // Render across many widths, including ones that land mid-glyph.
        for width in [12, 16, 17, 18, 20, 40, 80] {
            for height in [4u16, 8, 18, 40] {
                let lines = ad_lines(&creative, width, height, 0);
                // The box hugs the art: never taller than the pane.
                assert!(lines.len() <= usize::from(height));
                for line in &lines {
                    // Must always be valid UTF-8 and fit the terminal columns.
                    assert!(line.chars().count() <= usize::from(width));
                }
            }
        }
    }

    #[test]
    fn banner_respects_many_terminal_sizes() {
        let widths = [1, 2, 8, 20, 32, 50, 80, 120, 200];
        let heights = [1, 2, 4, 7, 8, 12, 18, 30, 60];

        for width in widths {
            for height in heights {
                let creative = default_ad_creative();
                let lines = ad_lines(&creative, width, height, 17);

                // The box hugs the art, so it may be shorter than the pane
                // but must never overflow it.
                assert!(lines.len() <= usize::from(height));
                for line in lines {
                    assert!(
                        line.len() <= usize::from(width),
                        "line is wider than terminal: width={width}, height={height}, line={line:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn api_base_arg_parser_accepts_flag_and_positional_url() {
        let flagged = parse_api_base_args(
            &[
                "--api-base-url".to_string(),
                "https://sponsor-shell.example.com".to_string(),
            ],
            "configure",
        )
        .unwrap();
        assert_eq!(
            flagged.api_base_url,
            Some("https://sponsor-shell.example.com".to_string())
        );

        let positional =
            parse_api_base_args(&["https://terminal.example.com".to_string()], "login").unwrap();
        assert_eq!(
            positional.api_base_url,
            Some("https://terminal.example.com".to_string())
        );
    }

    #[test]
    fn publisher_login_opens_the_role_specific_app_auth_flow() {
        assert_eq!(
            publisher_onboarding_url("https://staging.moneymux.com").unwrap(),
            "https://staging.moneymux.com/app?section=auth&mode=signup&role=publisher&next=publisher"
        );
        assert_eq!(
            publisher_onboarding_url("http://localhost:4000/api").unwrap(),
            "http://localhost:4000/app?section=auth&mode=signup&role=publisher&next=publisher"
        );
    }

    #[test]
    fn remote_ad_decision_body_includes_terminal_session_when_available() {
        let _environment = lock_process_environment();
        let body = remote_ad_decision_body(
            "device_1",
            Layout::new(120, 40),
            Some("session_1"),
            Some(30),
            2,
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(parsed["deviceId"], "device_1");
        assert_eq!(parsed["sessionId"], "session_1");
        assert_eq!(parsed["width"], 120);
        assert_eq!(parsed["height"], 40);
        assert_eq!(parsed["placement"], "prompt_boundary");
        // These were hardcoded `true`, which made them useless as signals and let
        // an ad rendered into a CI log or a pipe be billed as a real impression.
        // They must now reflect the ACTUAL terminal state. Under `cargo test`
        // stdout is captured, so this is false here — that is the point.
        assert_eq!(parsed["isTty"], terminal_is_interactive());
        assert_eq!(parsed["isInteractive"], terminal_is_interactive());
        assert_eq!(parsed["secondsSinceLastAd"], 30);
        assert_eq!(parsed["adsShownThisSession"], 2);
    }

    #[test]
    fn ci_environments_are_never_interactive() {
        let _environment = lock_process_environment();
        use std::io::IsTerminal;
        // Some CI runners allocate a PTY, so a TTY check alone is not enough:
        // an ad in a CI log is still an ad nobody saw.
        let previous = env::var("CI").ok();
        env::set_var("CI", "true");
        assert!(
            !terminal_is_interactive(),
            "CI=true must never be interactive"
        );
        env::set_var("CI", "false");
        // CI=false must not itself force non-interactive; fall through to the
        // real TTY check (false under captured test output).
        assert_eq!(
            terminal_is_interactive(),
            std::io::stdout().is_terminal() && std::io::stderr().is_terminal()
        );
        match previous {
            Some(value) => env::set_var("CI", value),
            None => env::remove_var("CI"),
        }
    }

    #[test]
    fn wrapped_command_label_is_bounded_for_api_payloads() {
        let long_arg = "é".repeat(200);
        let label = wrapped_command_label("codex", &[long_arg]);

        assert_eq!(label, "codex");
    }

    #[test]
    fn client_idle_uses_most_recent_attached_client_activity() {
        let idle = client_idle_from_activity_output("100\n120\n0\n", 150).unwrap();

        assert_eq!(idle, Duration::from_secs(30));
        assert_eq!(client_idle_from_activity_output("0\n\n", 150), None);
    }

    #[test]
    fn idle_fullscreen_delay_parser_rejects_invalid_values() {
        assert_eq!(parse_positive_seconds("5"), Some(5));
        assert_eq!(parse_positive_seconds(" 15 "), Some(15));
        assert_eq!(parse_positive_seconds("0"), None);
        assert_eq!(parse_positive_seconds("fast"), None);
    }

    #[test]
    fn missing_tmux_requires_an_explicit_install_command() {
        let message = tmux_missing_message();

        assert!(message.contains("sponsor-shell install-tmux"));
        assert!(message.contains("explicitly"));
        assert!(!message.contains("automatically"));
    }

    #[test]
    fn global_help_lists_every_management_command() {
        let help = help_lines().join("\n");

        for command in [
            "login",
            "link",
            "unlink",
            "configure",
            "status",
            "doctor",
            "install-tmux",
            "version",
        ] {
            assert!(help.contains(command), "help omitted {command}");
        }
    }

    #[test]
    fn tmux_ad_command_explicitly_passes_idle_fullscreen_delay() {
        let command = sponsor_ad_command(
            "/tmp/sponsor shell",
            "sponsor-shell-test",
            "75",
            "codex",
            Some("5"),
            true,
        );

        assert!(command.contains("SPONSOR_SHELL_IDLE_FULLSCREEN_SECONDS=5"));
        assert!(command.contains("SPONSOR_SHELL_TMUX_SESSION=sponsor-shell-test"));
        assert!(command.ends_with("'/tmp/sponsor shell' --sponsor-shell-ad-pane"));
    }

    #[test]
    fn tmux_ad_command_omits_idle_fullscreen_delay_without_env_override() {
        let command = sponsor_ad_command(
            "/tmp/sponsor-shell",
            "sponsor-shell-test",
            "75",
            "codex",
            None,
            true,
        );

        assert!(!command.contains("SPONSOR_SHELL_IDLE_FULLSCREEN_SECONDS"));
    }

    #[test]
    fn interactivity_verdict_is_passed_into_the_ad_pane() {
        // The pane cannot work this out for itself: tmux always gives it a PTY,
        // and it inherits the tmux SERVER's environment, so CI is invisible
        // there whenever a server was already running. The outer process must
        // hand the answer down explicitly.
        let interactive = sponsor_ad_command("/tmp/s", "sess", "75", "zsh", None, true);
        let headless = sponsor_ad_command("/tmp/s", "sess", "75", "zsh", None, false);

        assert!(interactive.contains("SPONSOR_SHELL_INTERACTIVE=1"));
        assert!(headless.contains("SPONSOR_SHELL_INTERACTIVE=0"));
    }

    #[test]
    fn ad_pane_obeys_the_outer_verdict_over_its_own_pty() {
        let _environment = lock_process_environment();
        // Inside a real pane stdout IS a terminal, so a local check would always
        // say "interactive". The injected verdict has to win.
        let previous = env::var(SPONSOR_INTERACTIVE_ENV).ok();
        env::set_var(SPONSOR_INTERACTIVE_ENV, "0");
        assert!(!terminal_is_interactive(), "pane must honour a 0 verdict");
        env::set_var(SPONSOR_INTERACTIVE_ENV, "1");
        assert!(terminal_is_interactive(), "pane must honour a 1 verdict");
        match previous {
            Some(value) => env::set_var(SPONSOR_INTERACTIVE_ENV, value),
            None => env::remove_var(SPONSOR_INTERACTIVE_ENV),
        }
    }

    #[test]
    fn ci_markers_beyond_plain_ci_are_detected() {
        let _environment = lock_process_environment();
        let previous = env::var("GITHUB_ACTIONS").ok();
        env::set_var("GITHUB_ACTIONS", "true");
        assert!(running_in_ci(), "GITHUB_ACTIONS alone must count as CI");
        match previous {
            Some(value) => env::set_var("GITHUB_ACTIONS", value),
            None => env::remove_var("GITHUB_ACTIONS"),
        }
    }

    #[test]
    fn fullscreen_centering_preserves_ascii_row_alignment() {
        let art = vec!["#####".to_string(), "  #".to_string(), "  #".to_string()];
        let centered = center_block(&art, 11);

        assert_eq!(centered, vec!["   #####", "     #", "     #"]);
    }

    #[test]
    fn fullscreen_renders_injected_art_full_bleed() {
        let creative = LocalAdCreative {
            enabled: Some(true),
            sponsor: Some("ART CO".to_string()),
            url: Some("art.example".to_string()),
            headline: Some("Headline".to_string()),
            subheadline: Some("Sub".to_string()),
            cta: Some("Go".to_string()),
            disclosure: Some("Disclosure".to_string()),
            idle_fullscreen_seconds: Some(7),
            ascii_art: Some("  ___  ART  ___\n /custom canvas/".to_string()),
            ascii_art_small: None,
            route: Some(vec!["sketch".to_string(), "ship".to_string()]),
            links: Some(vec!["art.example".to_string()]),
            stats: Some(vec!["canvas".to_string(), "terminal".to_string()]),
        }
        .into_ad_creative();
        let lines = ad_lines(&creative, 160, 50, 7);
        let text = lines.join("\n");

        // The expanded idle ad fills the whole canvas...
        assert_eq!(lines.len(), 50);
        // ...and the injected art renders inside it.
        assert!(text.contains("/custom canvas/"));
        // ...and the generated web copy is rendered with it.
        assert!(text.contains("ART CO / Sponsored terminal time / art.example"));
        assert!(text.contains("Headline"));
        assert!(text.contains("Sub"));
        assert!(text.contains("> Go"));
        assert!(text.contains("[Disclosure]"));
        assert!(text.contains("pipeline: sketch -> ship"));
        assert!(text.contains("inventory: canvas / terminal"));
        assert!(text.contains("links: art.example"));
        // ...and none of the old stock widgets are painted over it.
        assert!(!text.contains("what is happening"));
        assert!(!text.contains("SPONSORED BY"));
        assert!(!text.contains("]---["));
    }

    #[test]
    fn fullscreen_poster_layout_uses_lower_half() {
        let creative = default_ad_creative();
        let lines = ad_lines(&creative, 160, 50, 0);
        let last_content_row = lines
            .iter()
            .rposition(|line| !line.trim_matches(['|', '+', '-', ' ']).is_empty())
            .unwrap();

        assert!(last_content_row > 35);
    }

    #[test]
    fn links_are_wrapped_as_terminal_hyperlinks() {
        let creative = default_ad_creative();
        let linked = linkified_line("Visit https://railway.app/templates now", &creative, None);

        assert!(linked.contains("\x1b]8;;https://railway.app/templates\x1b\\"));
        assert!(!linked.contains("\x1b[4mrailway.app/templates\x1b[24m"));
        assert!(linked.contains("railway.app/templates"));
        assert!(linked.ends_with("\x1b]8;;\x1b\\ now"));
    }

    #[test]
    fn links_are_underlined_at_hovered_column() {
        let creative = default_ad_creative();
        let line = "Visit https://railway.app/templates now";
        let hovered_col = line.find("https://railway.app/templates").unwrap() + 4;
        let linked = linkified_line(line, &creative, Some(hovered_col));

        assert!(linked.contains("\x1b[4mhttps://railway.app/templates\x1b[24m"));
    }

    #[test]
    fn fullscreen_gate_only_accepts_clicks() {
        assert!(is_mouse_click(MouseEventKind::Down(
            event::MouseButton::Left
        )));
        assert!(!is_mouse_click(MouseEventKind::Up(
            event::MouseButton::Left
        )));
        assert!(!is_mouse_click(MouseEventKind::Moved));
        assert!(!is_mouse_click(MouseEventKind::ScrollDown));
    }

    #[test]
    fn control_c_forwards_press_and_repeat_but_not_release_events() {
        let control_c =
            |kind| KeyEvent::new_with_kind(KeyCode::Char('c'), KeyModifiers::CONTROL, kind);

        assert_eq!(
            key_to_tmux_send_key(control_c(KeyEventKind::Press)),
            Some("C-c")
        );
        assert_eq!(
            key_to_tmux_send_key(control_c(KeyEventKind::Repeat)),
            Some("C-c")
        );
        assert_eq!(key_to_tmux_send_key(control_c(KeyEventKind::Release)), None);
    }

    #[test]
    fn fullscreen_links_win_over_click_gate() {
        let creative = default_ad_creative();
        let line = "| links: https://railway.app https://railway.app/templates";

        assert_eq!(
            link_at_column(
                line,
                &creative,
                line.find("https://railway.app/templates").unwrap() + 4
            ),
            Some("https://railway.app/templates".to_string())
        );
        assert_eq!(
            link_at_column(
                line,
                &creative,
                line.find("https://railway.app").unwrap() + 4,
            ),
            Some("https://railway.app".to_string())
        );
        assert_eq!(link_at_column(line, &creative, 2), None);
    }

    #[test]
    fn undeclared_urls_embedded_in_copy_are_not_clickable() {
        let creative = default_ad_creative();
        let line = "Visit https://trusted.example@evil.example";

        assert_eq!(link_at_column(line, &creative, 12), None);
        assert_eq!(linkified_line(line, &creative, None), line);
    }

    #[test]
    fn rendered_fullscreen_links_are_click_targets() {
        let creative = LocalAdCreative {
            enabled: Some(true),
            sponsor: None,
            url: None,
            headline: None,
            subheadline: None,
            cta: None,
            disclosure: None,
            idle_fullscreen_seconds: None,
            ascii_art: Some("visit https://railway.app/templates for more".to_string()),
            ascii_art_small: None,
            route: None,
            links: None,
            stats: None,
        }
        .into_ad_creative();
        let layout = Layout::new(160, 50);
        let lines = ad_lines(&creative, layout.cols, layout.rows, 0);
        let (row, col) = lines
            .iter()
            .enumerate()
            .find_map(|(row, line)| {
                line.find("https://railway.app/templates")
                    .map(|col| (row as u16, (col + 5) as u16))
            })
            .expect("fullscreen ad should render the link inside the injected art");

        assert_eq!(
            ad_link_at(&creative, layout, 0, row, col),
            Some("https://railway.app/templates".to_string())
        );
    }

    #[test]
    fn local_ad_creative_maps_web_submission_into_terminal_creative() {
        let creative = LocalAdCreative {
            enabled: Some(true),
            sponsor: Some(" ASCII CO ".to_string()),
            url: Some("ascii.example/new".to_string()),
            headline: Some("Ship text ads from a web form".to_string()),
            subheadline: Some("Preview locally, then inject into sponsor-shell.".to_string()),
            cta: Some("Start at ascii.example/new".to_string()),
            disclosure: Some("Local sponsored terminal time".to_string()),
            idle_fullscreen_seconds: Some(9),
            ascii_art: Some("  /\\_/\\\\  \n ( o.o ) \n  > ^ <  ".to_string()),
            ascii_art_small: Some(" =^.^= ".to_string()),
            route: Some(vec!["create".to_string(), "preview".to_string()]),
            links: Some(vec![
                "ascii.example".to_string(),
                "ascii.example/new".to_string(),
            ]),
            stats: Some(vec!["terminal".to_string(), "local".to_string()]),
        }
        .into_ad_creative();

        assert_eq!(creative.id, "local-web");
        assert_eq!(creative.sponsor, "ASCII CO");
        assert_eq!(creative.url, "ascii.example/new");
        assert_eq!(creative.headline, "Ship text ads from a web form");
        assert_eq!(creative.cta, "Start at ascii.example/new");
        assert_eq!(creative.idle_fullscreen_seconds, 9);
        assert_eq!(
            creative.logos.large,
            vec![
                "  /\\_/\\\\".to_string(),
                " ( o.o )".to_string(),
                "  > ^ <".to_string(),
            ]
        );
        // The dedicated small-screen art is kept separately for narrow panes.
        assert_eq!(creative.logos.small, vec![" =^.^=".to_string()]);
        assert_eq!(creative.route, vec!["create", "preview"]);
        assert_eq!(creative.links, vec!["ascii.example", "ascii.example/new"]);
        assert_eq!(creative.stats, vec!["terminal", "local"]);
    }

    #[test]
    fn disabled_local_ad_creative_uses_inactive_terminal_slot() {
        let creative = LocalAdCreative {
            enabled: Some(false),
            sponsor: Some("SHOULD NOT RENDER".to_string()),
            url: Some("example.test".to_string()),
            headline: Some("hidden".to_string()),
            subheadline: None,
            cta: None,
            disclosure: None,
            idle_fullscreen_seconds: None,
            ascii_art: None,
            ascii_art_small: None,
            route: None,
            links: None,
            stats: None,
        }
        .into_ad_creative();

        assert_eq!(creative.id, "local-disabled");
        assert_eq!(creative.sponsor, "SPONSOR SLOT");
        assert!(!creative.headline.contains("hidden"));
        assert_eq!(creative.links, vec!["localhost:3000"]);
    }
}
