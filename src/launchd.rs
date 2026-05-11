/// macOS LaunchAgent management for auto-launch at login.
///
/// Writes a property list to `~/Library/LaunchAgents/com.openflow.dictation.plist`
/// so launchd starts the app on next login. The plist file is the source of
/// truth — checking whether it exists determines the current enabled state.
///
/// When the binary lives inside an .app bundle (e.g. /Applications/OpenFlow.app),
/// the LaunchAgent runs the app via `open -a AppName` so macOS treats it as a
/// proper app launch, preserving TCC identity. Otherwise the raw binary path is
/// used directly (Homebrew install).
///
/// During enable(), the quarantine xattr is stripped from the executable (and
/// the enclosing .app bundle if applicable) so macOS won't prompt the user to
/// approve the binary on every login.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const LABEL: &str = "com.openflow.dictation";
const PLIST_NAME: &str = "com.openflow.dictation.plist";
const APP_NAME: &str = "OpenFlow";

fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(PLIST_NAME),
    )
}

/// True when the LaunchAgent plist exists in ~/Library/LaunchAgents/.
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// Enable auto-launch at login.
///
/// Writes the LaunchAgent plist and loads it into the current GUI session via
/// `launchctl bootstrap`. Also removes the quarantine xattr from the executable
/// and enclosing .app bundle so macOS won't block the LaunchAgent on next login.
pub fn enable() -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("cannot resolve binary path: {e}"))?;

    // Strip quarantine so macOS won't prompt on every login.
    remove_quarantine(&current_exe);
    if let Some(app_root) = find_app_bundle_root(&current_exe) {
        remove_quarantine(&app_root);
    }

    let plist = plist_path().ok_or("HOME not set")?;
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create LaunchAgents dir: {e}"))?;
    }

    let content = plist_content(&current_exe);
    fs::write(&plist, &content).map_err(|e| format!("cannot write plist: {e}"))?;

    // Load into the current GUI session so it takes effect immediately.
    bootstrap_load(&plist)
}

/// Disable auto-launch at login.
///
/// Unloads the LaunchAgent from the current session and removes the plist.
pub fn disable() -> Result<(), String> {
    if let Some(plist) = plist_path()
        && plist.exists()
    {
        // Best-effort bootout — ignore failures (e.g. agent not loaded).
        let _ = bootout_unload();
        let _ = fs::remove_file(&plist);
    }
    Ok(())
}

/// Remove the `com.apple.quarantine` extended attribute from `path`.
///
/// macOS tacks this attribute onto files downloaded from the internet.
/// An unsigned binary with quarantine set will be blocked (or prompt) every
/// time launchd tries to run it at login. Stripping it solves that.
pub fn remove_quarantine(path: &Path) {
    let result = Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(path)
        .output();
    match result {
        Ok(o) if o.status.success() => {
            eprintln!("[launchd] removed quarantine from {}", path.display());
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // "-d: No such xattr" is fine — it means there was no quarantine.
            let trimmed = stderr.trim();
            if !trimmed.is_empty() && !trimmed.contains("No such xattr") {
                eprintln!("[launchd] xattr warning: {trimmed}");
            }
        }
        Err(e) => {
            eprintln!("[launchd] xattr failed: {e}");
        }
    }
}

// ─── app bundle helpers ────────────────────────────────────────────────────

/// If `exe_path` lives inside an .app bundle (e.g.
/// `/Applications/OpenFlow.app/Contents/MacOS/openflow`), return the path to
/// the `.app` directory itself. Otherwise return `None`.
pub fn find_app_bundle_root(exe_path: &Path) -> Option<PathBuf> {
    let mut current = exe_path.parent(); // MacOS/
    while let Some(dir) = current {
        if dir.extension().map_or(false, |ext| ext == "app")
            && dir.is_dir()
        {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// If `exe_path` lives inside an .app bundle whose parent directory is
/// /Applications (or a subdirectory thereof), return the app name so the plist
/// can use `open -a AppName`. Otherwise fall back to the raw binary path.
fn resolve_launch_command(exe_path: &Path) -> (String, Vec<String>) {
    if let Some(app_root) = find_app_bundle_root(exe_path) {
        // Check if the .app bundle is in /Applications (or subdir).
        if let Some(parent) = app_root.parent() {
            let parent_str = parent.to_string_lossy();
            if parent_str == "/Applications" || parent_str.starts_with("/Applications/") {
                return (
                    "open".to_string(),
                    vec![
                        "-a".to_string(),
                        app_root
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| APP_NAME.to_string()),
                    ],
                );
            }
        }
        // App bundle is in a non-standard location (e.g. target/release/).
        // Use `open <bundle-path>` so macOS still sees it as an app launch.
        return (
            "open".to_string(),
            vec![app_root.to_string_lossy().to_string()],
        );
    }

    // Standalone binary (Homebrew install).
    (
        exe_path.to_string_lossy().to_string(),
        vec!["live".to_string()],
    )
}

// ─── plist generation ──────────────────────────────────────────────────────

fn plist_content(exe_path: &Path) -> String {
    let (program, args) = resolve_launch_command(exe_path);

    let program_escaped = xml_escape(&program);
    let mut args_xml = String::new();
    for arg in &args {
        args_xml.push_str(&format!(
            "        <string>{}</string>\n",
            xml_escape(arg)
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
{args}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardOutPath</key>
    <string>/tmp/openflow.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/openflow.err</string>
</dict>
</plist>"#,
        label = LABEL,
        program = program_escaped,
        args = args_xml,
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── launchctl helpers ─────────────────────────────────────────────────────

/// Get the current user's UID by shelling out to `id -u`.
fn current_uid() -> Result<String, String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| format!("id -u failed: {e}"))?;
    if !output.status.success() {
        return Err("id -u returned non-zero".into());
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("invalid UTF-8 from id -u: {e}"))
}

fn bootstrap_load(plist: &Path) -> Result<(), String> {
    let uid = current_uid()?;
    let domain = format!("gui/{uid}");
    let output = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(plist)
        .output()
        .map_err(|e| format!("launchctl bootstrap failed: {e}"))?;

    if !output.status.success() {
        // Exit code 17 means "already loaded" — not an error for our purposes.
        if output.status.code() == Some(17) {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl bootstrap: {stderr}"));
    }
    Ok(())
}

fn bootout_unload() -> Result<(), String> {
    let uid = current_uid()?;
    let domain = format!("gui/{uid}/{LABEL}");
    let output = Command::new("launchctl")
        .args(["bootout", &domain])
        .output()
        .map_err(|e| format!("launchctl bootout failed: {e}"))?;

    if !output.status.success() {
        // Exit code 3 means "not found" — not an error.
        if output.status.code() == Some(3) {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl bootout: {stderr}"));
    }
    Ok(())
}
