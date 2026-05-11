/// macOS LaunchAgent management for auto-launch at login.
///
/// Writes a property list to `~/Library/LaunchAgents/com.openflow.dictation.plist`
/// so launchd starts the app on next login. The plist file is the source of
/// truth — checking whether it exists determines the current enabled state.
use std::fs;
use std::path::PathBuf;

const LABEL: &str = "com.openflow.dictation";
const PLIST_NAME: &str = "com.openflow.dictation.plist";

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
/// `launchctl bootstrap`. The `-w` persistence flag is intentionally omitted so
/// that removing the plist is sufficient to disable the agent.
pub fn enable() -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("cannot resolve binary path: {e}"))?;

    let plist = plist_path().ok_or("HOME not set")?;
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("cannot create LaunchAgents dir: {e}"))?;
    }

    fs::write(&plist, plist_content(&current_exe.to_string_lossy()))
        .map_err(|e| format!("cannot write plist: {e}"))?;

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

// ─── plist generation ──────────────────────────────────────────────────────

fn plist_content(binary: &str) -> String {
    // Escaping is only a concern for XML special characters, and typical macOS
    // binary paths (alphanumeric, /, -, _, .) don't contain any.
    let binary_escaped = xml_escape(binary);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>live</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
	<dict>
		<key>Crashed</key>
		<true/>
	</dict>
    <key>ProcessType</key>
    <string>Interactive</string>
    <key>StandardOutPath</key>
    <string>/tmp/openflow.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/openflow.err</string>
</dict>
</plist>"#,
        label = LABEL,
        binary = binary_escaped,
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
    let output = std::process::Command::new("id")
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

fn bootstrap_load(plist: &std::path::Path) -> Result<(), String> {
    let uid = current_uid()?;
    let domain = format!("gui/{uid}");
    let output = std::process::Command::new("launchctl")
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
    let output = std::process::Command::new("launchctl")
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
