use crate::domain::{FocusTarget, ScreenContext};
use crate::platform::macos;

pub trait FocusResolver {
    fn current_focus_target(&self) -> Result<FocusTarget, String>;
    fn current_screen_context(&self) -> Result<ScreenContext, String>;
}

#[derive(Debug, Default)]
pub struct DemoFocusResolver;

impl FocusResolver for DemoFocusResolver {
    fn current_focus_target(&self) -> Result<FocusTarget, String> {
        Ok(FocusTarget {
            app_name: "Notes".into(),
            element_role: "AXTextArea".into(),
        })
    }

    fn current_screen_context(&self) -> Result<ScreenContext, String> {
        Ok(ScreenContext {
            app_name: "Notes".into(),
            window_title: "Meeting prep".into(),
            focused_role: "AXTextArea".into(),
            focused_value_preview: "Action items for launch, customer feedback, latency fixes"
                .into(),
            visible_text: "Action items for launch, customer feedback, latency fixes".into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MacOsFocusResolver {
    max_chars: usize,
}

impl MacOsFocusResolver {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    fn snapshot(&self) -> Result<macos::FocusSnapshot, String> {
        macos::read_focus_snapshot(self.max_chars)
    }
}

impl FocusResolver for MacOsFocusResolver {
    fn current_focus_target(&self) -> Result<FocusTarget, String> {
        let snapshot = self.snapshot()?;
        Ok(FocusTarget {
            app_name: snapshot.app_name,
            element_role: snapshot.focused_role,
        })
    }

    fn current_screen_context(&self) -> Result<ScreenContext, String> {
        let snapshot = self.snapshot()?;
        Ok(ScreenContext {
            app_name: snapshot.app_name,
            window_title: snapshot.window_title,
            focused_role: snapshot.focused_role,
            focused_value_preview: snapshot.focused_value_preview,
            visible_text: snapshot.visible_text,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MacOsAccessibilityNotes;

impl MacOsAccessibilityNotes {
    pub fn key_steps() -> [&'static str; 5] {
        [
            "Ask for Accessibility permission at startup and fail soft when denied.",
            "Read the system-wide focused application and focused UI element first.",
            "Prefer AXValue or AXSelectedTextRange from the focused element before walking the whole tree.",
            "Bound traversal depth and total extracted characters to keep latency predictable.",
            "Cache the last successful focus target so paste can still happen if AX context refresh lags.",
        ]
    }
}
