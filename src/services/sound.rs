/// Subtle audio cues for recording start/stop, similar to Wispr Flow.
///
/// Uses `afplay` with macOS system sounds — no extra dependencies, non-blocking
/// (each sound fires in its own thread and is silent on failure).
///
/// Sound choices:
///   start → Tink.aiff  — brief high "ding" (recording active)
///   stop  → Pop.aiff   — brief soft "pop"  (recording ended)
///
/// Volume 0.45 keeps them mild and non-intrusive.

const VOLUME: &str = "0.45";

pub fn play_start_sound() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("afplay")
            .args(["/System/Library/Sounds/Tink.aiff", "-v", VOLUME])
            .output();
    });
}

pub fn play_stop_sound() {
    std::thread::spawn(|| {
        let _ = std::process::Command::new("afplay")
            .args(["/System/Library/Sounds/Pop.aiff", "-v", VOLUME])
            .output();
    });
}
