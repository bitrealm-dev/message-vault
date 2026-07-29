//! Cross-platform GUI wrapper around the `vault-push` CLI.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Default)]
struct SavedPrefs {
    url: String,
    account: String,
    source_id: String,
    remember_token: bool,
    #[serde(default)]
    token: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthState {
    None,
    /// Token valid; account exists (or was not checked).
    Ok,
    /// Token valid but account UUID not in DB yet (import may create it).
    TokenOkAccountMissing,
    BadToken,
    Error,
}

impl AuthState {
    fn allows_import(self) -> bool {
        matches!(self, Self::Ok | Self::TokenOkAccountMissing)
    }
}

struct App {
    url: String,
    token: String,
    /// Resolved account UUID (used for push / prefs).
    account: String,
    /// Display name from auth/check (`username`), falls back to UUID.
    account_label: String,
    source_id: String,
    input_dir: String,
    mode_append: bool,
    continue_on_error: bool,
    force_repush: bool,
    remember_token: bool,
    log: String,
    status: String,
    current_file: String,
    auth_state: AuthState,
    auth_detail: String,
    source_detect_note: String,
    authenticating: bool,
    running: bool,
    last_report: Option<PathBuf>,
    last_log: Option<PathBuf>,
    rx: Option<Receiver<UiMsg>>,
}

enum UiMsg {
    Line(String),
    AuthResult {
        state: AuthState,
        detail: String,
        account_id: Option<String>,
        username: Option<String>,
    },
    Done {
        code: i32,
        report: PathBuf,
        log: PathBuf,
    },
}

impl Default for App {
    fn default() -> Self {
        let prefs = load_prefs();
        let source_id = if prefs.source_id.trim().is_empty() {
            "imessage".into()
        } else {
            prefs.source_id
        };
        Self {
            url: prefs.url,
            token: if prefs.remember_token {
                prefs.token
            } else {
                String::new()
            },
            account: prefs.account,
            account_label: String::new(),
            source_id,
            input_dir: String::new(),
            mode_append: true,
            continue_on_error: true,
            force_repush: false,
            remember_token: prefs.remember_token,
            log: String::new(),
            status: "Ready — Authenticate before importing.".into(),
            current_file: String::new(),
            auth_state: AuthState::None,
            auth_detail: String::new(),
            source_detect_note: String::new(),
            authenticating: false,
            running: false,
            last_report: None,
            last_log: None,
            rx: None,
        }
    }
}

fn prefs_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("message-vault")
        .join("vault-push-gui.json")
}

fn load_prefs() -> SavedPrefs {
    let path = prefs_path();
    fs_read_json(&path).unwrap_or_default()
}

fn save_prefs(app: &App) {
    let prefs = SavedPrefs {
        url: app.url.clone(),
        account: app.account.clone(),
        source_id: app.source_id.clone(),
        remember_token: app.remember_token,
        token: if app.remember_token {
            app.token.clone()
        } else {
            String::new()
        },
    };
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&prefs) {
        let _ = std::fs::write(path, text);
    }
}

fn fs_read_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn find_vault_push() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("vault-push");
            if sibling.is_file() {
                return sibling;
            }
            #[cfg(windows)]
            {
                let sibling = dir.join("vault-push.exe");
                if sibling.is_file() {
                    return sibling;
                }
            }
        }
    }
    PathBuf::from("vault-push")
}

fn sorted_source_ids() -> Vec<&'static str> {
    let mut ids: Vec<_> = csv_ingest::known_source_ids().into_iter().collect();
    ids.sort_unstable();
    ids
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .weak()
            .color(egui::Color32::from_gray(140)),
    );
}

fn parse_progress_filename(line: &str) -> Option<String> {
    let rest = line.strip_prefix("PROGRESS ")?;
    // "12/400 ok chat.json …" / fail / skip
    let mut parts = rest.splitn(3, ' ');
    let _frac = parts.next()?;
    let _status = parts.next()?;
    let tail = parts.next()?;
    let name = tail.split_whitespace().next().unwrap_or(tail);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn format_log_line(line: &str) -> String {
    if let Some(name) = parse_progress_filename(line) {
        format!("▶ {name}\n  {line}")
    } else {
        line.to_string()
    }
}

#[derive(Deserialize)]
struct AuthCheckBody {
    ok: bool,
    #[serde(default)]
    sources: Vec<String>,
    account_id: Option<String>,
    username: Option<String>,
    account_ok: Option<bool>,
    admin: Option<bool>,
}

/// Returns (state, detail, resolved_account_id, username).
fn run_auth_check(
    url: &str,
    token: &str,
    account: &str,
) -> (AuthState, String, Option<String>, Option<String>) {
    let base = url.trim().trim_end_matches('/');
    if base.is_empty() || token.trim().is_empty() {
        return (
            AuthState::Error,
            "Enter vault URL and API token first.".into(),
            None,
            None,
        );
    }

    let mut endpoint = format!("{base}/v1/auth/check");
    let account = account.trim();
    if !account.is_empty() {
        endpoint.push_str(&format!("?account={}", urlencoding_simple(account)));
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                AuthState::Error,
                format!("HTTP client error: {e}"),
                None,
                None,
            );
        }
    };

    let resp = match client
        .get(&endpoint)
        .header("Authorization", format!("Bearer {}", token.trim()))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return (
                AuthState::Error,
                format!("Request failed: {e}"),
                None,
                None,
            );
        }
    };

    let status = resp.status();
    if status.as_u16() == 401 {
        return (AuthState::BadToken, "Invalid token".into(), None, None);
    }
    if status.as_u16() == 403 {
        let body = resp.text().unwrap_or_default();
        return (
            AuthState::Error,
            format!("Forbidden: {body}"),
            None,
            None,
        );
    }
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return (
            AuthState::Error,
            format!("Auth check failed ({status}): {body}"),
            None,
            None,
        );
    }

    let body: AuthCheckBody = match resp.json() {
        Ok(b) => b,
        Err(e) => {
            return (
                AuthState::Error,
                format!("Bad response: {e}"),
                None,
                None,
            );
        }
    };
    if !body.ok {
        return (
            AuthState::Error,
            "Server returned ok=false".into(),
            None,
            None,
        );
    }

    let n = body.sources.len();
    if body.admin == Some(true) {
        return (
            AuthState::Error,
            "This is the server admin token. Use your personal Import API token from web Settings."
                .into(),
            None,
            None,
        );
    }

    let username = body.username.filter(|s| !s.is_empty());
    if let Some(id) = body.account_id.filter(|s| !s.is_empty()) {
        return (
            AuthState::Ok,
            format!("Connected — {n} sources"),
            Some(id),
            username,
        );
    }

    match body.account_ok {
        Some(false) => (
            AuthState::TokenOkAccountMissing,
            format!("Token OK, account not found — {n} sources"),
            None,
            username,
        ),
        _ => (
            AuthState::Error,
            "Token OK but no account_id returned. Use a user token from web Settings.".into(),
            None,
            None,
        ),
    }
}

/// Minimal query escaping for UUID / account ids (no full url crate in GUI).
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl App {
    fn invalidate_auth(&mut self) {
        self.account_label.clear();
        if self.auth_state != AuthState::None {
            self.auth_state = AuthState::None;
            self.auth_detail.clear();
            self.status = "Credentials changed — Authenticate again.".into();
        }
    }

    fn detect_source_from_folder(&mut self) {
        let dir = self.input_dir.trim();
        if dir.is_empty() {
            self.source_detect_note.clear();
            return;
        }
        let path = PathBuf::from(dir);
        match csv_ingest::detect_export_source(path.as_path()) {
            Ok(Some(id)) => {
                self.source_id = id.clone();
                let mut note = format!("Detected: {}", csv_ingest::source_display_label(&id));
                if let Ok(Some(csv_ver)) = csv_ingest::detect_export_tool_version(path.as_path()) {
                    if let Some(info) = csv_ingest::source_info(&id) {
                        if let Some(expected) = info.tool_version {
                            if csv_ver != expected {
                                note.push_str(&format!(
                                    " (CSV version {csv_ver}; catalog targets {expected})"
                                ));
                            }
                        }
                    }
                }
                self.source_detect_note = note;
            }
            Ok(None) => {
                self.source_detect_note =
                    "Could not detect source from CSV — choose manually.".into();
            }
            Err(e) => {
                self.source_detect_note = format!("Source detection failed: {e}");
            }
        }
    }

    fn start_authenticate(&mut self) {
        if self.authenticating || self.running {
            return;
        }
        if self.url.trim().is_empty() || self.token.trim().is_empty() {
            self.status = "Enter vault URL and API token.".into();
            self.auth_state = AuthState::Error;
            self.auth_detail = self.status.clone();
            return;
        }

        save_prefs(self);

        let url = self.url.clone();
        let token = self.token.clone();
        self.authenticating = true;
        self.status = "Authenticating…".into();
        self.auth_detail = "Checking…".into();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let (state, detail, account_id, username) = run_auth_check(&url, &token, "");
            let _ = tx.send(UiMsg::AuthResult {
                state,
                detail,
                account_id,
                username,
            });
        });
    }

    fn start_import(&mut self) {
        if self.running {
            return;
        }
        if !self.auth_state.allows_import() {
            self.status = "Authenticate successfully before importing.".into();
            return;
        }
        if self.url.trim().is_empty() || self.token.trim().is_empty() || self.input_dir.trim().is_empty()
        {
            self.status = "Fill URL, API token, and export folder.".into();
            return;
        }
        if self.source_id.trim().is_empty() {
            self.status = "Choose a source.".into();
            return;
        }

        save_prefs(self);

        let input = PathBuf::from(self.input_dir.trim());
        let report = input.join("vault-push-report.json");
        let log = input.join("vault-push.log");
        let bin = find_vault_push();

        let mut cmd = Command::new(&bin);
        cmd.arg("--input")
            .arg(&input)
            .arg("--url")
            .arg(self.url.trim())
            .arg("--token")
            .arg(self.token.trim())
            .arg("--source-id")
            .arg(self.source_id.trim())
            .arg("--mode")
            .arg(if self.mode_append { "append" } else { "replace" })
            .arg("--report")
            .arg(&report)
            .arg("--log")
            .arg(&log)
            .arg("--checkpoint")
            .arg(input.join("vault-push-done.json"));
        if self.continue_on_error {
            cmd.arg("--continue-on-error");
        }
        if self.force_repush {
            cmd.arg("--force-repush");
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        self.log.clear();
        self.current_file.clear();
        self.log.push_str(&format!("Running {} …\n", bin.display()));
        self.status = "Import running…".into();
        self.running = true;
        self.last_report = Some(report.clone());
        self.last_log = Some(log.clone());

        let (tx, rx): (Sender<UiMsg>, Receiver<UiMsg>) = mpsc::channel();
        self.rx = Some(rx);

        thread::spawn(move || {
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(UiMsg::Line(format!(
                        "failed to start vault-push ({bin:?}): {e}\n\
                         Put vault-push next to this app or on PATH."
                    )));
                    let _ = tx.send(UiMsg::Done {
                        code: 2,
                        report,
                        log,
                    });
                    return;
                }
            };

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let tx2 = tx.clone();
            let t_out = thread::spawn(move || {
                if let Some(out) = stdout {
                    for line in BufReader::new(out).lines().flatten() {
                        let _ = tx2.send(UiMsg::Line(line));
                    }
                }
            });
            let tx3 = tx.clone();
            let t_err = thread::spawn(move || {
                if let Some(err) = stderr {
                    for line in BufReader::new(err).lines().flatten() {
                        let _ = tx3.send(UiMsg::Line(line));
                    }
                }
            });
            let _ = t_out.join();
            let _ = t_err.join();
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(1);
            let _ = tx.send(UiMsg::Done {
                code,
                report,
                log,
            });
        });
    }

    fn poll_messages(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else {
            return;
        };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                UiMsg::Line(line) => {
                    if let Some(name) = parse_progress_filename(&line) {
                        self.current_file = name;
                    }
                    self.log.push_str(&format_log_line(&line));
                    self.log.push('\n');
                }
                UiMsg::AuthResult {
                    state,
                    detail,
                    account_id,
                    username,
                } => {
                    self.authenticating = false;
                    self.auth_state = state;
                    self.auth_detail = detail.clone();
                    self.status = detail;
                    if let Some(id) = account_id {
                        self.account = id;
                    }
                    self.account_label = username
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| self.account.clone());
                    if state.allows_import() {
                        save_prefs(self);
                    }
                    if !self.running {
                        self.rx = None;
                    }
                    break;
                }
                UiMsg::Done { code, report, log } => {
                    self.running = false;
                    self.rx = None;
                    self.last_report = Some(report);
                    self.last_log = Some(log);
                    self.status = if code == 0 {
                        "Import finished successfully.".into()
                    } else {
                        format!("Import finished with errors (exit {code}). See report.")
                    };
                    break;
                }
            }
        }
        if self.running || self.authenticating {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn auth_color(&self) -> egui::Color32 {
        match self.auth_state {
            AuthState::Ok => egui::Color32::from_rgb(40, 140, 70),
            AuthState::TokenOkAccountMissing => egui::Color32::from_rgb(180, 120, 30),
            AuthState::BadToken | AuthState::Error => egui::Color32::from_rgb(180, 50, 50),
            AuthState::None => egui::Color32::from_gray(120),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_messages(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Message Vault Import");
            ui.label(
                egui::RichText::new(
                    "Push an exporter folder to your vault server. Authenticate first, then start import.",
                )
                .weak(),
            );
            ui.add_space(10.0);

            // --- Vault connection ---
            ui.heading(egui::RichText::new("Vault connection").size(16.0));
            egui::Grid::new("conn")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Vault URL");
                    ui.vertical(|ui| {
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.url)
                                    .desired_width(480.0)
                                    .hint_text("http://127.0.0.1:8080"),
                            )
                            .changed()
                        {
                            self.invalidate_auth();
                        }
                        hint(ui, "Base URL of message-vault-rs serve (no trailing path).");
                    });
                    ui.end_row();

                    ui.label("API token");
                    ui.vertical(|ui| {
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.token)
                                    .password(true)
                                    .desired_width(480.0),
                            )
                            .changed()
                        {
                            self.invalidate_auth();
                        }
                        hint(
                            ui,
                            "From web Settings → Import API token (identifies your account). Not your website password.",
                        );
                    });
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                let auth_btn = ui.add_enabled(
                    !self.authenticating && !self.running,
                    egui::Button::new("Authenticate"),
                );
                if auth_btn.clicked() {
                    self.start_authenticate();
                }
                ui.checkbox(&mut self.remember_token, "Remember token");
                if !self.auth_detail.is_empty() {
                    ui.colored_label(self.auth_color(), &self.auth_detail);
                }
            });
            if self.auth_state == AuthState::Ok && !self.account_label.is_empty() {
                hint(ui, &format!("Account: {}", self.account_label));
            }

            ui.add_space(12.0);

            // --- What to import ---
            ui.heading(egui::RichText::new("What to import").size(16.0));
            egui::Grid::new("import")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Export folder");
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.input_dir)
                                        .desired_width(400.0),
                                )
                                .changed()
                            {
                                self.detect_source_from_folder();
                            }
                            if ui.button("Browse…").clicked() {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.input_dir = path.display().to_string();
                                    self.detect_source_from_folder();
                                }
                            }
                        });
                        hint(
                            ui,
                            "Folder that contains your exported conversations. Media files are found from paths stored on each message — you do not pick them separately.",
                        );
                    });
                    ui.end_row();

                    ui.label("Source");
                    ui.vertical(|ui| {
                        let ids = sorted_source_ids();
                        let selected_label = if self.source_id.is_empty() {
                            "Choose source…".to_string()
                        } else {
                            csv_ingest::source_display_label(&self.source_id)
                        };
                        egui::ComboBox::from_id_salt("source_combo")
                            .selected_text(selected_label)
                            .width(480.0)
                            .show_ui(ui, |ui| {
                                for id in ids {
                                    let label = csv_ingest::source_display_label(id);
                                    if ui
                                        .selectable_value(
                                            &mut self.source_id,
                                            id.to_string(),
                                            label,
                                        )
                                        .changed()
                                    {
                                        self.source_detect_note.clear();
                                    }
                                }
                            });
                        if !self.source_detect_note.is_empty() {
                            hint(ui, &self.source_detect_note.clone());
                        } else {
                            hint(ui, "Usually detected from the export; override if needed.");
                        }
                    });
                    ui.end_row();
                });

            egui::CollapsingHeader::new("Advanced")
                .default_open(false)
                .show(ui, |ui| {
                    ui.checkbox(&mut self.mode_append, "Append mode (resume-safe)");
                    hint(
                        ui,
                        "Keep existing vault data; skip chats already uploaded (recommended).",
                    );
                    if !self.mode_append {
                        ui.colored_label(
                            egui::Color32::from_rgb(180, 100, 40),
                            "Replace wipes this source on the first uploaded chat.",
                        );
                    }
                    ui.add_space(4.0);
                    ui.checkbox(&mut self.continue_on_error, "Continue on error");
                    hint(ui, "If one chat fails, keep uploading the rest.");
                    ui.add_space(4.0);
                    ui.checkbox(&mut self.force_repush, "Force re-upload all (ignore checkpoint)");
                    hint(
                        ui,
                        "Upload every chat again, even ones marked done locally.",
                    );
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let can_import = !self.running
                    && !self.authenticating
                    && self.auth_state.allows_import();
                let import = ui.add_enabled(can_import, egui::Button::new("Start import"));
                if import.clicked() {
                    self.start_import();
                }
                ui.label(&self.status);
            });
            if !self.auth_state.allows_import() && !self.running {
                hint(ui, "Start import stays disabled until Authenticate succeeds.");
            }

            ui.add_space(10.0);

            // --- Progress ---
            ui.heading(egui::RichText::new("Progress").size(16.0));
            if !self.current_file.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Current file:");
                    ui.label(
                        egui::RichText::new(&self.current_file)
                            .strong()
                            .monospace(),
                    );
                });
            }

            let avail = ui.available_height().max(360.0) - 36.0;
            egui::ScrollArea::vertical()
                .min_scrolled_height(avail.max(360.0))
                .max_height(avail.max(360.0))
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.log)
                            .desired_width(f32::INFINITY)
                            .desired_rows(18)
                            .font(egui::TextStyle::Monospace)
                            .interactive(false),
                    );
                });

            ui.horizontal(|ui| {
                if let Some(path) = &self.last_report {
                    if ui.button("Open report").clicked() {
                        let _ = open::that(path);
                    }
                    ui.label(path.display().to_string());
                }
                if let Some(path) = &self.last_log {
                    if ui.button("Open log").clicked() {
                        let _ = open::that(path);
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    eprintln!(
        "warning: vault-push-gui is deprecated. \
         Use Message Exporters → Vault tab instead \
         (cargo run -p message-exporters-gui)."
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("Message Vault Import (deprecated)"),
        ..Default::default()
    };
    eframe::run_native(
        "Message Vault Import (deprecated)",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}
