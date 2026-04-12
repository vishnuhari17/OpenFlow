use crate::domain::FocusTarget;
use crate::platform::macos;

pub trait TextPaster {
    fn paste_text(&self, target: &FocusTarget, text: &str) -> Result<(), String>;
    /// Replace the most recently pasted text. `prev_char_count` is the number
    /// of Unicode scalar values that were pasted — the implementation selects
    /// them backwards and pastes `new_text` in their place.
    fn replace_recent_paste(
        &self,
        target: &FocusTarget,
        prev_char_count: usize,
        new_text: &str,
    ) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct DemoTextPaster;

impl TextPaster for DemoTextPaster {
    fn paste_text(&self, target: &FocusTarget, text: &str) -> Result<(), String> {
        println!(
            "[paste] target_app={} role={} text={}",
            target.app_name, target.element_role, text
        );
        Ok(())
    }

    fn replace_recent_paste(
        &self,
        target: &FocusTarget,
        _prev_char_count: usize,
        text: &str,
    ) -> Result<(), String> {
        println!(
            "[replace] target_app={} role={} text={}",
            target.app_name, target.element_role, text
        );
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MacOsTextPaster;

impl TextPaster for MacOsTextPaster {
    fn paste_text(&self, _target: &FocusTarget, text: &str) -> Result<(), String> {
        macos::paste_text_via_clipboard(text)
    }

    fn replace_recent_paste(
        &self,
        _target: &FocusTarget,
        prev_char_count: usize,
        new_text: &str,
    ) -> Result<(), String> {
        macos::replace_last_chars(prev_char_count, new_text)
    }
}
