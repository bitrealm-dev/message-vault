//! Native file/folder pickers via `rfd`, run off the Slint UI thread so the
//! event loop is never blocked (Wayland compositors treat a blocked UI as hung).

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use slint::{ComponentHandle, SharedString, Weak};

use crate::AppWindow;
use crate::ContactsAdapter;
use crate::ExtractAdapter;
use crate::FormatAdapter;
use crate::ImportAdapter;
use crate::VaultAdapter;
use crate::VaultExportAdapter;

/// Ensures only one native picker is open at a time (Browse can fire again while
/// the dialog is still up because it runs off the UI thread).
static PICKER_OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
pub enum BrowseKind {
    File,
    Folder,
    FileOrFolder,
}

pub fn browse_kind_for_field(field_id: &str) -> BrowseKind {
    match field_id {
        "contacts.input"
        | "extract.contacts"
        | "extract.name_mapping"
        | "extract.whatsapp_wa"
        | "extract.whatsapp_db"
        | "extract.apple_contacts" => BrowseKind::File,
        "extract.input" => BrowseKind::FileOrFolder,
        "extract.db_path" => BrowseKind::FileOrFolder,
        "import.backup_path" => BrowseKind::Folder,
        "import.db_path" => BrowseKind::Folder,
        "import.archive_path" => BrowseKind::Folder,
        "import.attachment_root" => BrowseKind::Folder,
        "extract.whatsapp_backup" => BrowseKind::FileOrFolder,
        "vault_export.output" => BrowseKind::Folder,
        _ => BrowseKind::Folder,
    }
}

/// Spawn a background dialog, then apply the picked path on the UI thread.
pub fn pick_path(ui_weak: Weak<AppWindow>, field_id: String, kind: BrowseKind) {
    if PICKER_OPEN
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        let picked = pick_path_blocking(kind);
        PICKER_OPEN.store(false, Ordering::Release);
        let Some(path) = picked else {
            return;
        };
        let path = SharedString::from(path.display().to_string());
        let _ = ui_weak.upgrade_in_event_loop(move |ui| {
            apply_path(&ui, &field_id, path);
        });
    });
}

fn pick_path_blocking(kind: BrowseKind) -> Option<PathBuf> {
    if crate::wsl::is_wsl() {
        match pick_with_windows_dialog(kind) {
            Ok(picked) => return picked,
            Err(error) => eprintln!("Windows file picker failed; using Linux picker: {error}"),
        }
    }

    let dialog = rfd::FileDialog::new().set_title("Choose path");
    match kind {
        BrowseKind::File => dialog.pick_file(),
        BrowseKind::Folder => dialog.pick_folder(),
        BrowseKind::FileOrFolder => dialog
            .pick_folder()
            .or_else(|| rfd::FileDialog::new().set_title("Choose file").pick_file()),
    }
}

fn pick_with_windows_dialog(kind: BrowseKind) -> io::Result<Option<PathBuf>> {
    let script = match kind {
        BrowseKind::File => WINDOWS_FILE_PICKER,
        BrowseKind::Folder => WINDOWS_FOLDER_PICKER,
        BrowseKind::FileOrFolder => WINDOWS_FILE_OR_FOLDER_PICKER,
    };
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "PowerShell exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        return Ok(None);
    }
    windows_to_wsl_path(Path::new(&selected)).map(Some)
}

fn windows_to_wsl_path(path: &Path) -> io::Result<PathBuf> {
    let output = Command::new("wslpath").arg("-u").arg(path).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "wslpath exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let converted = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if converted.is_empty() {
        Err(io::Error::other("wslpath returned an empty path"))
    } else {
        Ok(PathBuf::from(converted))
    }
}

const WINDOWS_FILE_PICKER: &str = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Choose file'
$dialog.CheckFileExists = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Write($dialog.FileName)
}
"#;

const WINDOWS_FOLDER_PICKER: &str = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'Choose folder'
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Write($dialog.SelectedPath)
}
"#;

const WINDOWS_FILE_OR_FOLDER_PICKER: &str = r#"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$sentinel = '__MESSAGE_VAULT_IO_SELECT_THIS_FOLDER__'
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = 'Choose file or open a folder and select Choose'
$dialog.CheckFileExists = $false
$dialog.CheckPathExists = $true
$dialog.ValidateNames = $false
$dialog.FileName = $sentinel
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    $selected = $dialog.FileName
    if ([System.IO.Path]::GetFileName($selected) -eq $sentinel) {
        $selected = [System.IO.Path]::GetDirectoryName($selected)
    }
    [Console]::Write($selected)
}
"#;

fn apply_path(ui: &AppWindow, field_id: &str, path: SharedString) {
    match field_id {
        "contacts.input" => ui.global::<ContactsAdapter>().set_input(path),
        "extract.input" => ui.global::<ExtractAdapter>().set_input(path),
        "extract.output" => ui.global::<ExtractAdapter>().set_output(path),
        "extract.db_path" => ui.global::<ExtractAdapter>().set_db_path(path),
        "extract.contacts" => ui.global::<ExtractAdapter>().set_contacts(path),
        "extract.name_mapping" => ui.global::<ExtractAdapter>().set_name_mapping(path),
        "extract.whatsapp_backup" => ui.global::<ExtractAdapter>().set_whatsapp_backup(path),
        "extract.whatsapp_wa" => ui.global::<ExtractAdapter>().set_whatsapp_wa(path),
        "extract.whatsapp_media" => ui.global::<ExtractAdapter>().set_whatsapp_media(path),
        "extract.whatsapp_db" => ui.global::<ExtractAdapter>().set_whatsapp_db(path),
        "extract.apple_contacts" => ui.global::<ExtractAdapter>().set_apple_contacts(path),
        "extract.attachment_root" => ui.global::<ExtractAdapter>().set_attachment_root(path),
        "import.backup_path" | "import.db_path" => {
            ui.global::<ImportAdapter>().set_backup_path(path)
        }
        "import.archive_path" => ui.global::<ImportAdapter>().set_archive_path(path),
        "import.attachment_root" => ui.global::<ImportAdapter>().set_attachment_root(path),
        "format.input" => ui.global::<FormatAdapter>().set_input(path),
        "format.output" => ui.global::<FormatAdapter>().set_output(path),
        "vault.input" => ui.global::<VaultAdapter>().set_input(path),
        "vault_export.output" => ui.global::<VaultExportAdapter>().set_output(path),
        _ => {}
    }
}
