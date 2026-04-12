#[cfg(target_os = "macos")]
#[allow(non_camel_case_types, unsafe_op_in_unsafe_fn)]
mod imp {
    use std::ffi::CString;
    use std::mem;
    use std::os::raw::{c_char, c_void};
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, LazyLock, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::domain::HotkeyEvent;

    type Boolean = u8;
    type CFIndex = isize;
    type CFTypeID = usize;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFBooleanRef = *const c_void;
    type CFRunLoopRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFAllocatorRef = *const c_void;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;
    type pid_t = i32;
    type ObjcId = *mut c_void;
    type Sel = *mut c_void;
    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CGEventMask = u64;
    type CGKeyCode = u16;

    type CGEventFlags = u64;
    type CGEventType = u32;
    type CGEventField = u32;
    type AXError = i32;
    type AXValueType = u32;

    const AX_ERROR_SUCCESS: AXError = 0;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    const K_AX_VALUE_TYPE_CFRANGE: AXValueType = 4;

    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
    const K_CG_EVENT_KEY_UP: CGEventType = 11;
    const K_CG_EVENT_FLAGS_CHANGED: CGEventType = 12;
    const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFF_FFFE;
    const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: CGEventType = 0xFFFF_FFFF;

    const K_CG_KEYBOARD_EVENT_KEYCODE: CGEventField = 9;
    const K_CG_EVENT_FLAG_MASK_COMMAND: CGEventFlags = 0x0010_0000;

    const K_VK_COMMAND: CGKeyCode = 0x37;
    const K_VK_RIGHT_COMMAND: CGKeyCode = 0x36;
    const K_VK_SHIFT: CGKeyCode = 0x38;
    const K_VK_F18: CGKeyCode = 0x4F;
    const K_VK_ANSI_V: CGKeyCode = 0x09;
    const K_VK_LEFT_ARROW: CGKeyCode = 0x7B;
    const K_CG_EVENT_FLAG_MASK_SHIFT: CGEventFlags = 0x0002_0000;

    #[link(name = "CoreFoundation", kind = "framework")]
    #[link(name = "ApplicationServices", kind = "framework")]
    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "objc")]
    unsafe extern "C" {
        fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
        fn CFStringGetTypeID() -> CFTypeID;
        fn CFDictionaryCreate(
            allocator: CFAllocatorRef,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        fn CFStringCreateWithCString(
            allocator: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: CFIndex,
            encoding: u32,
        ) -> Boolean;
        fn CFArrayGetCount(the_array: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(the_array: CFTypeRef, idx: CFIndex) -> *const c_void;

        static kCFBooleanTrue: CFBooleanRef;
        static kCFRunLoopCommonModes: CFStringRef;
        static kAXTrustedCheckOptionPrompt: CFStringRef;

        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: pid_t) -> AXUIElementRef;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut pid_t) -> AXError;
        fn AXUIElementSetMessagingTimeout(
            element: AXUIElementRef,
            timeout_in_seconds: f32,
        ) -> AXError;
        fn AXUIElementCopyAttributeNames(
            element: AXUIElementRef,
            names: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementCopyParameterizedAttributeValue(
            element: AXUIElementRef,
            parameterized_attribute: CFStringRef,
            parameter: CFTypeRef,
            result: *mut CFTypeRef,
        ) -> AXError;
        fn AXValueGetTypeID() -> CFTypeID;
        fn AXValueGetType(value: AXValueRef) -> AXValueType;

        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
        fn CGPreflightPostEventAccess() -> bool;
        fn CGRequestPostEventAccess() -> bool;
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: CGEventMask,
            callback: extern "C" fn(
                proxy: CGEventTapProxy,
                event_type: CGEventType,
                event: CGEventRef,
                user_info: *mut c_void,
            ) -> CGEventRef,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            virtual_key: CGKeyCode,
            key_down: bool,
        ) -> CGEventRef;
        fn CGEventGetFlags(event: CGEventRef) -> CGEventFlags;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
        fn CGEventPost(tap: u32, event: CGEventRef);
        fn CGEventSetFlags(event: CGEventRef, flags: CGEventFlags);
        fn CGEventSourceFlagsState(state_id: u32) -> CGEventFlags;
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: CFIndex,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopAddSource(
            run_loop: CFRunLoopRef,
            source: CFRunLoopSourceRef,
            mode: CFStringRef,
        );
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopRun();

        fn objc_getClass(name: *const c_char) -> ObjcId;
        fn sel_registerName(name: *const c_char) -> Sel;
        static NSPasteboardTypeString: ObjcId;
        fn objc_msgSend();
    }

    #[derive(Debug, Clone)]
    pub struct PermissionSnapshot {
        pub accessibility: bool,
        pub listen_events: bool,
        pub post_events: bool,
    }

    #[derive(Debug, Clone)]
    pub struct FocusSnapshot {
        pub app_name: String,
        pub window_title: String,
        pub focused_role: String,
        pub focused_value_preview: String,
        pub visible_text: String,
    }

    #[derive(Debug, Clone)]
    pub struct AxDebugReport {
        pub frontmost_app_name: Option<String>,
        pub frontmost_pid: Option<pid_t>,
        pub system_focused_application_error: String,
        pub system_focused_element_error: String,
        pub app_focused_window_error: Option<String>,
        pub app_focused_element_error: Option<String>,
        pub system_attributes: Vec<String>,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum TriggerKey {
        F18,
        RightCommand,
    }

    impl TriggerKey {
        pub fn from_name(value: &str) -> Result<Self, String> {
            match value.to_ascii_lowercase().as_str() {
                "f18" => Ok(Self::F18),
                "right_command" | "right-command" | "rightcmd" => Ok(Self::RightCommand),
                other => Err(format!(
                    "unsupported hold key '{other}'. use 'f18' or 'right_command'"
                )),
            }
        }

        fn keycode(self) -> CGKeyCode {
            match self {
                Self::F18 => K_VK_F18,
                Self::RightCommand => K_VK_RIGHT_COMMAND,
            }
        }

        fn matching_event(self, event_type: CGEventType, event: CGEventRef) -> Option<HotkeyEvent> {
            let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) }
                as CGKeyCode;
            if keycode != self.keycode() {
                return None;
            }

            match self {
                Self::F18 => match event_type {
                    K_CG_EVENT_KEY_DOWN => Some(HotkeyEvent::Pressed),
                    K_CG_EVENT_KEY_UP => Some(HotkeyEvent::Released),
                    _ => None,
                },
                Self::RightCommand => {
                    if event_type != K_CG_EVENT_FLAGS_CHANGED {
                        return None;
                    }

                    let flags = unsafe { CGEventGetFlags(event) };
                    if flags & K_CG_EVENT_FLAG_MASK_COMMAND != 0 {
                        Some(HotkeyEvent::Pressed)
                    } else {
                        Some(HotkeyEvent::Released)
                    }
                }
            }
        }
    }

    struct OwnedCf {
        ptr: CFTypeRef,
    }

    impl OwnedCf {
        fn new(ptr: CFTypeRef) -> Option<Self> {
            if ptr.is_null() {
                None
            } else {
                Some(Self { ptr })
            }
        }

        fn as_ptr(&self) -> CFTypeRef {
            self.ptr
        }
    }

    impl Clone for OwnedCf {
        fn clone(&self) -> Self {
            unsafe {
                let retained = CFRetain(self.ptr);
                Self { ptr: retained }
            }
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                unsafe {
                    CFRelease(self.ptr);
                }
            }
        }
    }

    static HOTKEY_CODE: AtomicUsize = AtomicUsize::new(K_VK_F18 as usize);
    static HOTKEY_KIND: AtomicUsize = AtomicUsize::new(0);
    static HOTKEY_SUPPRESS: AtomicBool = AtomicBool::new(true);
    static EVENT_TAP_PTR: AtomicUsize = AtomicUsize::new(0);
    static HOTKEY_SENDER: LazyLock<Mutex<Option<mpsc::Sender<HotkeyEvent>>>> =
        LazyLock::new(|| Mutex::new(None));

    pub fn permission_snapshot(prompt: bool) -> PermissionSnapshot {
        PermissionSnapshot {
            accessibility: accessibility_granted(prompt),
            listen_events: event_listen_granted(prompt),
            post_events: event_post_granted(prompt),
        }
    }

    pub fn ensure_accessibility(prompt: bool) -> Result<(), String> {
        if accessibility_granted(prompt) {
            Ok(())
        } else {
            Err("Accessibility permission is required. Grant it in System Settings > Privacy & Security > Accessibility, then rerun the command.".into())
        }
    }

    pub fn ensure_listen_access(prompt: bool) -> Result<(), String> {
        if event_listen_granted(prompt) {
            Ok(())
        } else {
            Err("Input Monitoring permission is required for global key capture. Grant it in System Settings > Privacy & Security > Input Monitoring, then rerun the command.".into())
        }
    }

    pub fn ensure_post_access(prompt: bool) -> Result<(), String> {
        if event_post_granted(prompt) {
            Ok(())
        } else {
            Err("macOS denied synthetic key events. Grant access in System Settings > Privacy & Security > Accessibility or the relevant automation prompt, then rerun the command.".into())
        }
    }

    pub fn read_focus_snapshot(max_chars: usize) -> Result<FocusSnapshot, String> {
        ensure_accessibility(false)?;

        let system = OwnedCf::new(unsafe { AXUIElementCreateSystemWide() } as CFTypeRef)
            .ok_or_else(|| "failed to create system-wide accessibility element".to_string())?;
        let _ = unsafe { AXUIElementSetMessagingTimeout(system.as_ptr() as AXUIElementRef, 1.5) };

        let frontmost = frontmost_application_info();
        let system_focused_app =
            copy_ax_element_attribute_with_error(system.as_ptr(), "AXFocusedApplication")?;
        let system_focused_element =
            copy_ax_element_attribute_with_error(system.as_ptr(), "AXFocusedUIElement")?;

        let fallback_app = frontmost
            .as_ref()
            .and_then(|info| unsafe { OwnedCf::new(AXUIElementCreateApplication(info.pid) as CFTypeRef) });
        if let Some(app) = fallback_app.as_ref() {
            let _ = unsafe { AXUIElementSetMessagingTimeout(app.as_ptr() as AXUIElementRef, 1.5) };
        }

        let focused_app = system_focused_app.value.clone().or_else(|| fallback_app.clone());
        let focused_element = system_focused_element.value.clone().or_else(|| {
            fallback_app.as_ref().and_then(|app| {
                copy_ax_element_attribute_with_error(app.as_ptr(), "AXFocusedUIElement")
                    .ok()
                    .and_then(|lookup| lookup.value)
            })
        });

        if focused_app.is_none() && focused_element.is_none() {
            let debug = debug_report_for(system.as_ptr(), frontmost.as_ref(), fallback_app.as_ref())?;
            return Err(format!(
                "no focused accessibility element found. frontmost_app={:?} frontmost_pid={:?} system_focused_app={} system_focused_element={} app_focused_window={:?} app_focused_element={:?}. Run `cargo run -- ax-debug` and send me the output.",
                debug.frontmost_app_name,
                debug.frontmost_pid,
                debug.system_focused_application_error,
                debug.system_focused_element_error,
                debug.app_focused_window_error,
                debug.app_focused_element_error
            ));
        }

        let pid_hint = focused_element
            .as_ref()
            .and_then(|element| pid_for_ax_element(element.as_ptr()).ok())
            .or_else(|| {
                focused_app
                    .as_ref()
                    .and_then(|app| pid_for_ax_element(app.as_ptr()).ok())
            })
            .map(|pid| format!("pid:{pid}"));

        let app_name = first_non_empty(&[
            focused_app
                .as_ref()
                .and_then(|app| string_attribute(app.as_ptr(), "AXTitle").ok().flatten()),
            frontmost.as_ref().and_then(|info| info.name.clone()),
            pid_hint,
        ])
        .unwrap_or_else(|| "Unknown".into());

        let focused_role = focused_element
            .as_ref()
            .and_then(|element| string_attribute(element.as_ptr(), "AXRole").ok().flatten())
            .unwrap_or_else(|| "Unknown".into());

        let window_title = window_title_for(focused_element.as_ref(), focused_app.as_ref())?
            .unwrap_or_default();

        let focused_value_preview = if let Some(element) = focused_element.as_ref() {
            focus_preview_for(element, &window_title, max_chars)?.unwrap_or_default()
        } else {
            String::new()
        };

        // Gather "nearby" visible text to feed Whisper as a vocabulary hint.
        // Strategy: start at the focused element (or the focused window when
        // no element is available, e.g. Electron/webview apps), then climb
        // the AX tree one parent at a time until we've gathered enough text
        // or run out of ancestors. Only text-role elements (static text,
        // text areas, text fields) contribute — this filters out button
        // labels and other UI chrome while still picking up chat bubbles,
        // document body, and form fields.
        let visible_text = extract_nearby_text(
            focused_element.as_ref(),
            focused_app.as_ref(),
        );

        Ok(FocusSnapshot {
            app_name,
            window_title,
            focused_role,
            focused_value_preview,
            visible_text,
        })
    }

    pub fn ax_debug_report() -> Result<AxDebugReport, String> {
        ensure_accessibility(false)?;
        let system = OwnedCf::new(unsafe { AXUIElementCreateSystemWide() } as CFTypeRef)
            .ok_or_else(|| "failed to create system-wide accessibility element".to_string())?;
        let _ = unsafe { AXUIElementSetMessagingTimeout(system.as_ptr() as AXUIElementRef, 1.5) };

        let frontmost = frontmost_application_info();
        let fallback_app = frontmost
            .as_ref()
            .and_then(|info| unsafe { OwnedCf::new(AXUIElementCreateApplication(info.pid) as CFTypeRef) });
        if let Some(app) = fallback_app.as_ref() {
            let _ = unsafe { AXUIElementSetMessagingTimeout(app.as_ptr() as AXUIElementRef, 1.5) };
        }

        debug_report_for(system.as_ptr(), frontmost.as_ref(), fallback_app.as_ref())
    }

    pub fn paste_text_via_clipboard(text: &str) -> Result<(), String> {
        ensure_post_access(false)?;
        ensure_accessibility(false)?;

        // Wait for all modifier keys (especially Command) to be released before
        // injecting the synthetic ⌘V.  When using Right Command as the trigger
        // key the OS may still consider it "held" at the moment this function
        // runs, which causes the synthetic keystroke to be swallowed or
        // misinterpreted.
        wait_for_modifiers_released(500);

        // Save the user's prior clipboard so we can restore it after pasting.
        // Some apps copy non-string content (images, files) we can't easily
        // round-trip; in that case `prior` is None and we just leave whatever
        // we wrote on the pasteboard.
        let prior = unsafe {
            let pool = autorelease_pool_new()?;
            let snapshot = read_pasteboard_string();
            let write_result = write_to_pasteboard(text);
            autorelease_pool_drain(pool);
            write_result?;
            snapshot
        };

        // Give the pasteboard a moment to settle.
        thread::sleep(Duration::from_millis(60));

        let cg_result = unsafe { post_command_v_via_cg() };
        if let Err(cg_error) = cg_result {
            return Err(cg_error);
        }

        // Restore the prior clipboard contents after the paste settles.
        if let Some(prior) = prior {
            thread::sleep(Duration::from_millis(120));
            unsafe {
                if let Ok(pool) = autorelease_pool_new() {
                    let _ = write_to_pasteboard(&prior);
                    autorelease_pool_drain(pool);
                }
            }
        }

        Ok(())
    }

    /// Replace the last `prev_char_count` characters at the current focus by
    /// selecting them with Shift+Left and pasting `new_text`. Used to upgrade
    /// a freshly pasted draft to its refined version without any visible
    /// flicker beyond the replacement itself.
    pub fn replace_last_chars(prev_char_count: usize, new_text: &str) -> Result<(), String> {
        if prev_char_count == 0 {
            return paste_text_via_clipboard(new_text);
        }

        ensure_post_access(false)?;
        ensure_accessibility(false)?;
        wait_for_modifiers_released(200);

        unsafe { post_shift_left_n(prev_char_count) }?;
        // Tiny pause so the selection is visible to the foreground app before
        // we drop the keystroke that replaces it.
        thread::sleep(Duration::from_millis(15));

        paste_text_via_clipboard(new_text)
    }

    /// Return the PID of the currently frontmost application, if one is
    /// reachable via NSWorkspace.
    pub fn frontmost_pid() -> Option<i32> {
        frontmost_application_info().map(|info| info.pid)
    }

    /// Bring the application with `pid` back to the foreground. Used when the
    /// user's focus drifted between record and paste — we re-activate the
    /// original target before injecting the synthetic ⌘V.
    pub fn activate_pid(pid: i32) -> Result<(), String> {
        unsafe {
            let pool = autorelease_pool_new()?;
            let class = get_class("NSRunningApplication")?;
            let app = msg_send_id_pid(class, sel("runningApplicationWithProcessIdentifier:")?, pid);
            if app.is_null() {
                autorelease_pool_drain(pool);
                return Err(format!("no running application with pid {pid}"));
            }
            // NSApplicationActivateIgnoringOtherApps = 1 << 1
            let activated = msg_send_bool_uint(app, sel("activateWithOptions:")?, 1 << 1);
            autorelease_pool_drain(pool);
            if activated {
                // Give the WindowServer a beat to actually move focus.
                thread::sleep(Duration::from_millis(60));
                Ok(())
            } else {
                Err(format!("activateWithOptions: returned false for pid {pid}"))
            }
        }
    }

    /// Spin-waits up to `timeout_ms` for all modifier flags to be cleared.
    fn wait_for_modifiers_released(timeout_ms: u64) {
        // Flags we care about: Shift, Control, Option, Command.
        const ALL_MODIFIER_MASK: CGEventFlags =
            0x0002_0000  // Shift
            | 0x0004_0000  // Control
            | 0x0008_0000  // Option
            | 0x0010_0000; // Command

        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let flags = unsafe { current_modifier_flags() };
            if flags & ALL_MODIFIER_MASK == 0 {
                break;
            }
            if std::time::Instant::now() >= deadline {
                eprintln!(
                    "[paste] warning: modifier keys still held after {}ms (flags=0x{:X}), proceeding anyway",
                    timeout_ms, flags
                );
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Read the current global modifier flags from Core Graphics.
    unsafe fn current_modifier_flags() -> CGEventFlags {
        // CGEventSourceFlagsState with kCGEventSourceStateCombinedSessionState (0)
        CGEventSourceFlagsState(0)
    }

    pub fn monitor_trigger_key(trigger: TriggerKey, suppress_trigger: bool) -> Result<(), String> {
        ensure_listen_access(false)?;

        HOTKEY_CODE.store(trigger.keycode() as usize, Ordering::Relaxed);
        HOTKEY_KIND.store(
            match trigger {
                TriggerKey::F18 => 0,
                TriggerKey::RightCommand => 1,
            },
            Ordering::Relaxed,
        );
        HOTKEY_SUPPRESS.store(suppress_trigger, Ordering::Relaxed);

        let mask = (1u64 << K_CG_EVENT_KEY_DOWN)
            | (1u64 << K_CG_EVENT_KEY_UP)
            | (1u64 << K_CG_EVENT_FLAGS_CHANGED);

        let tap = unsafe {
            CGEventTapCreate(
                K_CG_SESSION_EVENT_TAP,
                K_CG_HEAD_INSERT_EVENT_TAP,
                K_CG_EVENT_TAP_OPTION_DEFAULT,
                mask,
                hotkey_tap_callback,
                ptr::null_mut(),
            )
        };

        if tap.is_null() {
            return Err("failed to create global event tap. macOS usually returns null when Input Monitoring permission is missing or the app is running in a restricted context.".into());
        }

        EVENT_TAP_PTR.store(tap as usize, Ordering::Relaxed);

        let source = OwnedCf::new(unsafe {
            CFMachPortCreateRunLoopSource(ptr::null(), tap, 0) as CFTypeRef
        })
        .ok_or_else(|| "failed to create run-loop source for event tap".to_string())?;

        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source.as_ptr() as CFRunLoopSourceRef, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            println!(
                "Monitoring {:?}. Press Ctrl+C to stop.",
                trigger
            );
            CFRunLoopRun();
        }

        Ok(())
    }

    pub fn spawn_hotkey_stream(
        trigger: TriggerKey,
        suppress_trigger: bool,
    ) -> Result<mpsc::Receiver<HotkeyEvent>, String> {
        let (sender, receiver) = mpsc::channel();
        {
            let mut slot = HOTKEY_SENDER
                .lock()
                .map_err(|_| "failed to install hotkey sender".to_string())?;
            *slot = Some(sender);
        }

        thread::spawn(move || {
            if let Err(error) = monitor_trigger_key(trigger, suppress_trigger) {
                eprintln!("hotkey stream failed: {error}");
            }
        });

        Ok(receiver)
    }

    extern "C" fn hotkey_tap_callback(
        _proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: CGEventRef,
        _user_info: *mut c_void,
    ) -> CGEventRef {
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            let tap_ptr = EVENT_TAP_PTR.load(Ordering::Relaxed) as CFMachPortRef;
            if !tap_ptr.is_null() {
                unsafe {
                    CGEventTapEnable(tap_ptr, true);
                }
            }
            return event;
        }

        let trigger = match HOTKEY_KIND.load(Ordering::Relaxed) {
            0 => TriggerKey::F18,
            _ => TriggerKey::RightCommand,
        };

        if let Some(hotkey_event) = trigger.matching_event(event_type, event) {
            let mut emitted = false;
            if let Ok(slot) = HOTKEY_SENDER.lock() {
                if let Some(sender) = slot.as_ref() {
                    let _ = sender.send(hotkey_event.clone());
                    emitted = true;
                }
            }
            if !emitted {
                println!("[hotkey] {:?}", hotkey_event);
            }
            if HOTKEY_SUPPRESS.load(Ordering::Relaxed) {
                return ptr::null_mut();
            }
        }

        event
    }

    fn accessibility_granted(prompt: bool) -> bool {
        unsafe {
            if prompt {
                let options = accessibility_prompt_options();
                AXIsProcessTrustedWithOptions(options.as_ptr() as CFDictionaryRef) != 0
            } else {
                AXIsProcessTrustedWithOptions(ptr::null()) != 0
            }
        }
    }

    fn event_listen_granted(prompt: bool) -> bool {
        unsafe {
            if prompt {
                CGRequestListenEventAccess()
            } else {
                CGPreflightListenEventAccess()
            }
        }
    }

    fn event_post_granted(prompt: bool) -> bool {
        unsafe {
            if prompt {
                CGRequestPostEventAccess()
            } else {
                CGPreflightPostEventAccess()
            }
        }
    }

    fn accessibility_prompt_options() -> OwnedCf {
        let keys = [unsafe { kAXTrustedCheckOptionPrompt } as *const c_void];
        let values = [unsafe { kCFBooleanTrue } as *const c_void];
        let dict = unsafe {
            CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                ptr::null(),
                ptr::null(),
            )
        };
        OwnedCf::new(dict as CFTypeRef).expect("CFDictionaryCreate returned null")
    }

    fn copy_ax_element_attribute(
        element: CFTypeRef,
        attribute_name: &str,
    ) -> Result<Option<OwnedCf>, String> {
        Ok(copy_ax_element_attribute_with_error(element, attribute_name)?.value)
    }

    fn copy_ax_element_attribute_with_error(
        element: CFTypeRef,
        attribute_name: &str,
    ) -> Result<AttributeLookup, String> {
        let attribute = CfString::new(attribute_name)?;
        let mut out: CFTypeRef = ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(
                element as AXUIElementRef,
                attribute.as_ptr(),
                &mut out,
            )
        };

        Ok(AttributeLookup {
            value: if error == AX_ERROR_SUCCESS {
                OwnedCf::new(out)
            } else {
                None
            },
            error,
        })
    }

    fn string_attribute(element: CFTypeRef, attribute_name: &str) -> Result<Option<String>, String> {
        let value = copy_ax_element_attribute(element, attribute_name)?;
        Ok(value.and_then(|item| cf_type_to_string(item.as_ptr())))
    }

    fn parameterized_string_attribute(
        element: CFTypeRef,
        attribute_name: &str,
        parameter: &OwnedCf,
    ) -> Result<Option<String>, String> {
        let attribute = CfString::new(attribute_name)?;
        let mut out: CFTypeRef = ptr::null();
        let error = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                element as AXUIElementRef,
                attribute.as_ptr(),
                parameter.as_ptr(),
                &mut out,
            )
        };

        if error != AX_ERROR_SUCCESS {
            return Ok(None);
        }

        Ok(OwnedCf::new(out).and_then(|item| cf_type_to_string(item.as_ptr())))
    }

    fn window_title_for(
        focused_element: Option<&OwnedCf>,
        focused_app: Option<&OwnedCf>,
    ) -> Result<Option<String>, String> {
        if let Some(focused_element) = focused_element {
            if let Some(window) = copy_ax_element_attribute(focused_element.as_ptr(), "AXWindow")? {
                if let Some(title) = string_attribute(window.as_ptr(), "AXTitle")? {
                    return Ok(Some(title));
                }
            }
        }

        if let Some(focused_app) = focused_app {
            if let Some(window) = copy_ax_element_attribute(focused_app.as_ptr(), "AXFocusedWindow")? {
                if let Some(title) = string_attribute(window.as_ptr(), "AXTitle")? {
                    return Ok(Some(title));
                }
            }
        }

        Ok(None)
    }

    fn focus_preview_for(
        focused_element: &OwnedCf,
        _window_title: &str,
        max_chars: usize,
    ) -> Result<Option<String>, String> {
        let selected_text = string_attribute(focused_element.as_ptr(), "AXSelectedText")?;
        if let Some(text) = selected_text.filter(|value| !value.trim().is_empty()) {
            return Ok(Some(normalize_and_truncate(&text, max_chars)));
        }

        if let Some(range) = copy_ax_element_attribute(focused_element.as_ptr(), "AXVisibleCharacterRange")? {
            if unsafe { CFGetTypeID(range.as_ptr()) } == unsafe { AXValueGetTypeID() }
                && unsafe { AXValueGetType(range.as_ptr() as AXValueRef) } == K_AX_VALUE_TYPE_CFRANGE
            {
                if let Some(text) = parameterized_string_attribute(
                    focused_element.as_ptr(),
                    "AXStringForRange",
                    &range,
                )? {
                    if !text.trim().is_empty() {
                        return Ok(Some(normalize_and_truncate(&text, max_chars)));
                    }
                }
            }
        }

        if let Some(value) = string_attribute(focused_element.as_ptr(), "AXValue")? {
            if !value.trim().is_empty() {
                return Ok(Some(normalize_and_truncate(&value, max_chars)));
            }
        }

        // AXTitle and window_title are element labels, not user content — skip them.
        // If the text field is empty, return None so the caller knows there's nothing.
        Ok(None)
    }

    /// Gather visible text from the user's focused window so Whisper gets a
    /// vocabulary hint covering everything currently on screen in the active
    /// app (contact names in a sidebar, recent chat messages, document body,
    /// etc.).
    ///
    /// We deliberately anchor on the *window* rather than the focused
    /// element: in chat and document apps the content the user cares about
    /// lives in sibling subtrees, not in the parent chain of the text input.
    /// We fall back to the focused element (walking up to its window) when
    /// the app doesn't expose a focused window directly.
    fn extract_nearby_text(
        focused_element: Option<&OwnedCf>,
        focused_app: Option<&OwnedCf>,
    ) -> String {
        const MAX_CHARS: usize = 1200;

        let window = focused_app
            .and_then(|app| {
                copy_ax_element_attribute(app.as_ptr(), "AXFocusedWindow")
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                focused_element.and_then(|element| {
                    copy_ax_element_attribute(element.as_ptr(), "AXWindow")
                        .ok()
                        .flatten()
                })
            });

        // Prefer anchoring on the focused element's immediate panel (the direct
        // child of the window that contains the focused element) rather than the
        // full window root. In split-panel apps like WhatsApp or Slack the
        // sidebar lives in a sibling panel — starting from the window root
        // fills the budget with sidebar content before reaching the open
        // conversation. Walking up from the focused element and stopping one
        // level below the window gives us just the relevant panel.
        let anchor = focused_element
            .zip(window.as_ref())
            .and_then(|(element, win)| panel_ancestor_of(element, win))
            .or(window);

        let Some(anchor) = anchor else {
            return String::new();
        };
        collect_static_text(anchor.as_ptr(), MAX_CHARS)
    }

    /// Walk up the AX parent chain from `element` and return the tightest
    /// ancestor that represents just the "content panel" containing the focused
    /// element — not siblings like a sidebar or navigation panel.
    ///
    /// Two stopping conditions (whichever comes first):
    ///   1. The parent is AXWindow / AXApplication — return current (direct
    ///      child of window).
    ///   2. The parent has ≥ 2 children that are layout containers (AXGroup,
    ///      AXScrollArea, AXSplitGroup, AXTabGroup) — this is a split layout.
    ///      Return `current` so we anchor on just one branch, not both panels.
    fn panel_ancestor_of(element: &OwnedCf, _window: &OwnedCf) -> Option<OwnedCf> {
        const MAX_CLIMB: usize = 20;
        let mut current = element.clone();

        for _ in 0..MAX_CLIMB {
            let parent = copy_ax_element_attribute(current.as_ptr(), "AXParent")
                .ok()
                .flatten()?;

            let parent_role = string_attribute(parent.as_ptr(), "AXRole")
                .ok()
                .flatten()
                .unwrap_or_default();

            if parent_role == "AXWindow" || parent_role == "AXApplication" {
                return Some(current);
            }

            // Detect split-panel layout: parent has 2+ layout-container children.
            // `current` is one branch of that split — use it as the anchor so
            // we don't collect text from sibling panels (sidebar, nav, etc.).
            if let Ok(Some(children)) = copy_ax_element_attribute(parent.as_ptr(), "AXChildren") {
                let count = unsafe { CFArrayGetCount(children.as_ptr()) };
                let mut panel_siblings: usize = 0;
                for i in 0..count {
                    let child = unsafe { CFArrayGetValueAtIndex(children.as_ptr(), i) };
                    if child.is_null() { continue; }
                    let role = string_attribute(child as CFTypeRef, "AXRole")
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    if matches!(role.as_str(),
                        "AXGroup" | "AXScrollArea" | "AXSplitGroup" | "AXTabGroup"
                    ) {
                        panel_siblings += 1;
                    }
                }
                if panel_siblings >= 2 {
                    return Some(current);
                }
            }

            current = parent;
        }

        None
    }

    /// Recursively walk the AX subtree rooted at `root` and collect readable
    /// text from any element that isn't UI chrome, up to `max_chars` total.
    ///
    /// We use a *skip list* rather than a keep list because different apps
    /// expose their content under different roles: chat bubbles might be
    /// `AXStaticText`, `AXGroup`, `AXCell`, or `AXRow`. Excluding the obvious
    /// chrome (buttons, menus, toolbars, images) removes the noise while
    /// letting real content through regardless of role. Skipped subtrees are
    /// not descended into, which preserves the element budget for content.
    fn collect_static_text(root: CFTypeRef, max_chars: usize) -> String {
        const MAX_DEPTH: usize = 25;
        const MAX_ELEMENTS: usize = 2500;

        let mut buf = String::new();
        let mut visited: usize = 0;

        fn is_chrome_role(role: Option<&str>) -> bool {
            matches!(
                role,
                Some("AXButton")
                    | Some("AXMenuBar")
                    | Some("AXMenuBarItem")
                    | Some("AXMenu")
                    | Some("AXMenuItem")
                    | Some("AXCheckBox")
                    | Some("AXRadioButton")
                    | Some("AXPopUpButton")
                    | Some("AXToolbar")
                    | Some("AXImage")
                    | Some("AXSlider")
                    | Some("AXIncrementor")
                    | Some("AXDisclosureTriangle")
                    | Some("AXScrollBar")
                    | Some("AXTabGroup")
                    | Some("AXColorWell")
                    | Some("AXProgressIndicator")
            )
        }

        fn strip_fmt_marks(s: &str) -> String {
            s.chars().filter(|c| !matches!(c,
                '\u{200b}' | '\u{200c}' | '\u{200d}'
                | '\u{200e}' | '\u{200f}'
                | '\u{202a}' | '\u{202b}' | '\u{202c}'
                | '\u{202d}' | '\u{202e}' | '\u{202f}'
                | '\u{feff}'
            )).collect()
        }

        fn element_text(element: CFTypeRef) -> Option<String> {
            for attr in &["AXValue", "AXTitle", "AXDescription"] {
                if let Ok(Some(text)) = string_attribute(element, attr) {
                    let clean = strip_fmt_marks(&text);
                    let trimmed = clean.trim().to_string();
                    if trimmed.chars().count() >= 2 {
                        return Some(trimmed);
                    }
                }
            }
            None
        }

        fn push_snippet(buf: &mut String, text: &str, max_chars: usize) {
            let trimmed = text.trim();
            if trimmed.chars().count() < 2 {
                return;
            }
            // Skip near-duplicates of the tail (parents that echo children,
            // or consecutive identical labels like timestamps).
            if buf.ends_with(trimmed) {
                return;
            }
            if !buf.is_empty() {
                buf.push('\n');
            }
            let remaining = max_chars.saturating_sub(buf.chars().count());
            let snippet: String = trimmed.chars().take(remaining).collect();
            buf.push_str(&snippet);
        }

        fn walk(
            element: CFTypeRef,
            buf: &mut String,
            visited: &mut usize,
            max_chars: usize,
            depth: usize,
        ) {
            if depth > MAX_DEPTH || *visited >= MAX_ELEMENTS || buf.chars().count() >= max_chars {
                return;
            }
            *visited += 1;

            let role = string_attribute(element, "AXRole").ok().flatten();
            if is_chrome_role(role.as_deref()) {
                // Skip the subtree entirely — button labels, menu items, and
                // toolbar contents are UI chrome we don't want in the prompt.
                return;
            }

            if let Some(text) = element_text(element) {
                push_snippet(buf, &text, max_chars);
            }

            if let Ok(Some(children)) = copy_ax_element_attribute(element, "AXChildren") {
                let count = unsafe { CFArrayGetCount(children.as_ptr()) };
                for idx in 0..count {
                    if *visited >= MAX_ELEMENTS || buf.chars().count() >= max_chars {
                        break;
                    }
                    let child = unsafe { CFArrayGetValueAtIndex(children.as_ptr(), idx) };
                    if !child.is_null() {
                        walk(child as CFTypeRef, buf, visited, max_chars, depth + 1);
                    }
                }
            }
        }

        walk(root, &mut buf, &mut visited, max_chars, 0);
        buf
    }

    fn pid_for_ax_element(element: CFTypeRef) -> Result<pid_t, String> {
        let mut pid: pid_t = 0;
        let error = unsafe { AXUIElementGetPid(element as AXUIElementRef, &mut pid) };
        if error == AX_ERROR_SUCCESS {
            Ok(pid)
        } else {
            Err("failed to read AX process id".into())
        }
    }

    fn first_non_empty(values: &[Option<String>]) -> Option<String> {
        values
            .iter()
            .flatten()
            .find(|value| !value.trim().is_empty())
            .cloned()
    }

    fn frontmost_application_info() -> Option<FrontmostApplicationInfo> {
        unsafe {
            let pool = autorelease_pool_new().ok()?;
            let workspace_class = get_class("NSWorkspace").ok()?;
            let workspace = msg_send_id(workspace_class, sel("sharedWorkspace").ok()?);
            if workspace.is_null() {
                autorelease_pool_drain(pool);
                return None;
            }

            let app = msg_send_id(workspace, sel("frontmostApplication").ok()?);
            if app.is_null() {
                autorelease_pool_drain(pool);
                return None;
            }

            let pid = msg_send_i32(app, sel("processIdentifier").ok()?);
            let name = msg_send_id(app, sel("localizedName").ok()?);
            let result = if name.is_null() {
                None
            } else {
                cf_string_to_rust(name as CFStringRef).map(|name| FrontmostApplicationInfo {
                    name: Some(name),
                    pid,
                })
            };
            autorelease_pool_drain(pool);
            result
        }
    }

    fn ax_error_name(error: AXError) -> &'static str {
        match error {
            0 => "success",
            -25200 => "failure",
            -25201 => "illegal_argument",
            -25202 => "invalid_ui_element",
            -25203 => "invalid_observer",
            -25204 => "cannot_complete",
            -25205 => "attribute_unsupported",
            -25206 => "action_unsupported",
            -25207 => "notification_unsupported",
            -25208 => "not_implemented",
            -25209 => "notification_already_registered",
            -25210 => "notification_not_registered",
            -25211 => "api_disabled",
            -25212 => "no_value",
            -25213 => "parameterized_attribute_unsupported",
            -25214 => "not_enough_precision",
            _ => "unknown",
        }
    }

    fn debug_report_for(
        system: CFTypeRef,
        frontmost: Option<&FrontmostApplicationInfo>,
        fallback_app: Option<&OwnedCf>,
    ) -> Result<AxDebugReport, String> {
        let system_focused_app =
            copy_ax_element_attribute_with_error(system, "AXFocusedApplication")?;
        let system_focused_element =
            copy_ax_element_attribute_with_error(system, "AXFocusedUIElement")?;
        let app_focused_window = fallback_app
            .map(|app| copy_ax_element_attribute_with_error(app.as_ptr(), "AXFocusedWindow"))
            .transpose()?;
        let app_focused_element = fallback_app
            .map(|app| copy_ax_element_attribute_with_error(app.as_ptr(), "AXFocusedUIElement"))
            .transpose()?;

        Ok(AxDebugReport {
            frontmost_app_name: frontmost.and_then(|info| info.name.clone()),
            frontmost_pid: frontmost.map(|info| info.pid),
            system_focused_application_error: format!(
                "{} ({})",
                ax_error_name(system_focused_app.error),
                system_focused_app.error
            ),
            system_focused_element_error: format!(
                "{} ({})",
                ax_error_name(system_focused_element.error),
                system_focused_element.error
            ),
            app_focused_window_error: app_focused_window.map(|lookup| {
                format!("{} ({})", ax_error_name(lookup.error), lookup.error)
            }),
            app_focused_element_error: app_focused_element.map(|lookup| {
                format!("{} ({})", ax_error_name(lookup.error), lookup.error)
            }),
            system_attributes: attribute_names(system)?,
        })
    }

    fn attribute_names(element: CFTypeRef) -> Result<Vec<String>, String> {
        let mut out: CFTypeRef = ptr::null();
        let error = unsafe { AXUIElementCopyAttributeNames(element as AXUIElementRef, &mut out) };
        if error != AX_ERROR_SUCCESS {
            return Ok(vec![format!("error:{}({})", ax_error_name(error), error)]);
        }

        let array = OwnedCf::new(out).ok_or_else(|| "attribute name array was null".to_string())?;
        let count = unsafe { CFArrayGetCount(array.as_ptr()) };
        let mut names = Vec::new();
        for idx in 0..count {
            let item = unsafe { CFArrayGetValueAtIndex(array.as_ptr(), idx) };
            if !item.is_null() {
                if let Some(name) = cf_type_to_string(item as CFTypeRef) {
                    names.push(name);
                }
            }
        }

        Ok(names)
    }



    fn normalize_and_truncate(value: &str, max_chars: usize) -> String {
        // Strip Unicode bidirectional/directional formatting marks that apps
        // like WhatsApp embed in their AX strings (LRM, RLM, LRE, RLE, etc.).
        let stripped: String = value.chars().filter(|c| !matches!(c,
            '\u{200b}'  // zero-width space
            | '\u{200c}'  // zero-width non-joiner
            | '\u{200d}'  // zero-width joiner
            | '\u{200e}'  // left-to-right mark
            | '\u{200f}'  // right-to-left mark
            | '\u{202a}'  // left-to-right embedding
            | '\u{202b}'  // right-to-left embedding
            | '\u{202c}'  // pop directional formatting
            | '\u{202d}'  // left-to-right override
            | '\u{202e}'  // right-to-left override
            | '\u{202f}'  // narrow no-break space (used in timestamps)
            | '\u{feff}'  // BOM / zero-width no-break space
        )).collect();
        let normalized = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
        normalized.chars().take(max_chars).collect()
    }

    fn cf_type_to_string(value: CFTypeRef) -> Option<String> {
        let type_id = unsafe { CFGetTypeID(value) };
        if type_id != unsafe { CFStringGetTypeID() } {
            return None;
        }

        unsafe { cf_string_to_rust(value as CFStringRef) }
    }

    unsafe fn cf_string_to_rust(value: CFStringRef) -> Option<String> {
        let length = CFStringGetLength(value);
        let capacity = CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) + 1;
        let mut buffer = vec![0i8; capacity as usize];
        let ok = CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            capacity,
            K_CF_STRING_ENCODING_UTF8,
        );
        if ok == 0 {
            return None;
        }

        let bytes = buffer
            .iter()
            .take_while(|value| **value != 0)
            .map(|value| *value as u8)
            .collect::<Vec<_>>();
        String::from_utf8(bytes).ok()
    }

    struct CfString {
        inner: OwnedCf,
    }

    impl CfString {
        fn new(value: &str) -> Result<Self, String> {
            let c_value = CString::new(value)
                .map_err(|_| format!("string contains interior NUL byte: {value:?}"))?;
            let cf = unsafe {
                CFStringCreateWithCString(ptr::null(), c_value.as_ptr(), K_CF_STRING_ENCODING_UTF8)
            };
            let inner = OwnedCf::new(cf as CFTypeRef)
                .ok_or_else(|| format!("failed to create CFString for {value:?}"))?;
            Ok(Self { inner })
        }

        fn as_ptr(&self) -> CFStringRef {
            self.inner.as_ptr() as CFStringRef
        }
    }

    unsafe fn autorelease_pool_new() -> Result<ObjcId, String> {
        let class = get_class("NSAutoreleasePool")?;
        let pool = msg_send_id(class, sel("new")?);
        if pool.is_null() {
            Err("failed to create NSAutoreleasePool".into())
        } else {
            Ok(pool)
        }
    }

    unsafe fn autorelease_pool_drain(pool: ObjcId) {
        if let Ok(drain_sel) = sel("drain") {
            msg_send_void(pool, drain_sel);
        }
    }

    /// Read the current string content of the general pasteboard. Returns
    /// `None` when the pasteboard holds non-string content (images, files).
    unsafe fn read_pasteboard_string() -> Option<String> {
        let pool = autorelease_pool_new().ok()?;
        let pasteboard_class = get_class("NSPasteboard").ok()?;
        let pasteboard = msg_send_id(pasteboard_class, sel("generalPasteboard").ok()?);
        if pasteboard.is_null() {
            autorelease_pool_drain(pool);
            return None;
        }
        let ns_str = msg_send_id_id(
            pasteboard,
            sel("stringForType:").ok()?,
            NSPasteboardTypeString,
        );
        let result = if ns_str.is_null() {
            None
        } else {
            cf_string_to_rust(ns_str as CFStringRef)
        };
        autorelease_pool_drain(pool);
        result
    }

    /// Inject Shift+Left `count` times to select the last `count` characters.
    unsafe fn post_shift_left_n(count: usize) -> Result<(), String> {
        // Hold Shift down.
        let shift_down = create_key_event(K_VK_SHIFT, true, K_CG_EVENT_FLAG_MASK_SHIFT)?;
        CGEventPost(K_CG_HID_EVENT_TAP, shift_down.as_ptr() as CGEventRef);
        thread::sleep(Duration::from_millis(8));

        // Inject Left-arrow key events while Shift is held. Batch in chunks so
        // we don't spam CG with thousands of events for very long pastes — but
        // in practice text pasted here is at most a few hundred chars.
        for _ in 0..count {
            let left_down = create_key_event(K_VK_LEFT_ARROW, true, K_CG_EVENT_FLAG_MASK_SHIFT)?;
            let left_up = create_key_event(K_VK_LEFT_ARROW, false, K_CG_EVENT_FLAG_MASK_SHIFT)?;
            CGEventPost(K_CG_HID_EVENT_TAP, left_down.as_ptr() as CGEventRef);
            CGEventPost(K_CG_HID_EVENT_TAP, left_up.as_ptr() as CGEventRef);
        }

        // Release Shift.
        let shift_up = create_key_event(K_VK_SHIFT, false, 0)?;
        CGEventPost(K_CG_HID_EVENT_TAP, shift_up.as_ptr() as CGEventRef);
        thread::sleep(Duration::from_millis(8));

        Ok(())
    }

    unsafe fn write_to_pasteboard(text: &str) -> Result<(), String> {
        let pasteboard_class = get_class("NSPasteboard")?;
        let pasteboard = msg_send_id(pasteboard_class, sel("generalPasteboard")?);
        if pasteboard.is_null() {
            return Err("failed to access NSPasteboard.generalPasteboard".into());
        }

        let string = nsstring(text)?;
        msg_send_isize(pasteboard, sel("clearContents")?);
        let success = msg_send_bool_id_id(
            pasteboard,
            sel("setString:forType:")?,
            string,
            NSPasteboardTypeString,
        );
        msg_send_void(string, sel("release")?);

        if success {
            Ok(())
        } else {
            Err("NSPasteboard.setString:forType: returned false".into())
        }
    }

    unsafe fn post_command_v_via_cg() -> Result<(), String> {
        let cmd_down = create_key_event(K_VK_COMMAND, true, K_CG_EVENT_FLAG_MASK_COMMAND)?;
        let v_down = create_key_event(K_VK_ANSI_V, true, K_CG_EVENT_FLAG_MASK_COMMAND)?;
        let v_up = create_key_event(K_VK_ANSI_V, false, K_CG_EVENT_FLAG_MASK_COMMAND)?;
        let cmd_up = create_key_event(K_VK_COMMAND, false, 0)?;

        for event in [&cmd_down, &v_down, &v_up, &cmd_up] {
            CGEventPost(K_CG_HID_EVENT_TAP, event.as_ptr() as CGEventRef);
            thread::sleep(Duration::from_millis(8));
        }

        Ok(())
    }

    unsafe fn create_key_event(
        keycode: CGKeyCode,
        key_down: bool,
        flags: CGEventFlags,
    ) -> Result<OwnedCf, String> {
        let event = CGEventCreateKeyboardEvent(ptr::null(), keycode, key_down);
        let event = OwnedCf::new(event as CFTypeRef)
            .ok_or_else(|| format!("failed to create keyboard event for keycode {keycode}"))?;
        CGEventSetFlags(event.as_ptr() as CGEventRef, flags);
        Ok(event)
    }

    unsafe fn nsstring(value: &str) -> Result<ObjcId, String> {
        let class = get_class("NSString")?;
        let alloc = msg_send_id(class, sel("alloc")?);
        if alloc.is_null() {
            return Err("NSString alloc returned null".into());
        }

        let c_value =
            CString::new(value).map_err(|_| "paste text contains interior NUL byte".to_string())?;
        let string = msg_send_id_cstr(alloc, sel("initWithUTF8String:")?, c_value.as_ptr());
        if string.is_null() {
            Err("NSString initWithUTF8String: returned null".into())
        } else {
            Ok(string)
        }
    }

    unsafe fn get_class(name: &str) -> Result<ObjcId, String> {
        let c_name = CString::new(name).map_err(|_| format!("invalid ObjC class name {name:?}"))?;
        let class = objc_getClass(c_name.as_ptr());
        if class.is_null() {
            Err(format!("Objective-C class not found: {name}"))
        } else {
            Ok(class)
        }
    }

    fn sel(name: &str) -> Result<Sel, String> {
        let c_name =
            CString::new(name).map_err(|_| format!("invalid selector name {name:?}"))?;
        let selector = unsafe { sel_registerName(c_name.as_ptr()) };
        if selector.is_null() {
            Err(format!("selector lookup failed: {name}"))
        } else {
            Ok(selector)
        }
    }

    unsafe fn msg_send_id(obj: ObjcId, selector: Sel) -> ObjcId {
        let send: unsafe extern "C" fn(ObjcId, Sel) -> ObjcId = mem::transmute(objc_msgSend as *const ());
        send(obj, selector)
    }

    unsafe fn msg_send_void(obj: ObjcId, selector: Sel) {
        let send: unsafe extern "C" fn(ObjcId, Sel) = mem::transmute(objc_msgSend as *const ());
        send(obj, selector)
    }

    unsafe fn msg_send_isize(obj: ObjcId, selector: Sel) -> isize {
        let send: unsafe extern "C" fn(ObjcId, Sel) -> isize =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector)
    }

    unsafe fn msg_send_i32(obj: ObjcId, selector: Sel) -> i32 {
        let send: unsafe extern "C" fn(ObjcId, Sel) -> i32 =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector)
    }

    unsafe fn msg_send_id_cstr(obj: ObjcId, selector: Sel, arg: *const c_char) -> ObjcId {
        let send: unsafe extern "C" fn(ObjcId, Sel, *const c_char) -> ObjcId =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector, arg)
    }

    unsafe fn msg_send_bool_id_id(obj: ObjcId, selector: Sel, arg1: ObjcId, arg2: ObjcId) -> bool {
        let send: unsafe extern "C" fn(ObjcId, Sel, ObjcId, ObjcId) -> bool =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector, arg1, arg2)
    }

    unsafe fn msg_send_id_id(obj: ObjcId, selector: Sel, arg: ObjcId) -> ObjcId {
        let send: unsafe extern "C" fn(ObjcId, Sel, ObjcId) -> ObjcId =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector, arg)
    }

    unsafe fn msg_send_id_pid(obj: ObjcId, selector: Sel, pid: i32) -> ObjcId {
        let send: unsafe extern "C" fn(ObjcId, Sel, i32) -> ObjcId =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector, pid)
    }

    unsafe fn msg_send_bool_uint(obj: ObjcId, selector: Sel, options: usize) -> bool {
        let send: unsafe extern "C" fn(ObjcId, Sel, usize) -> bool =
            mem::transmute(objc_msgSend as *const ());
        send(obj, selector, options)
    }

    struct AttributeLookup {
        value: Option<OwnedCf>,
        error: AXError,
    }

    struct FrontmostApplicationInfo {
        name: Option<String>,
        pid: pid_t,
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;

#[cfg(not(target_os = "macos"))]
mod imp {
    use crate::domain::HotkeyEvent;
    use std::sync::mpsc;

    #[derive(Debug, Clone)]
    pub struct PermissionSnapshot {
        pub accessibility: bool,
        pub listen_events: bool,
        pub post_events: bool,
    }

    #[derive(Debug, Clone)]
    pub struct FocusSnapshot {
        pub app_name: String,
        pub window_title: String,
        pub focused_role: String,
        pub focused_value_preview: String,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum TriggerKey {
        F18,
        RightCommand,
    }

    impl TriggerKey {
        pub fn from_name(_value: &str) -> Result<Self, String> {
            Err("macOS-only feature".into())
        }
    }

    pub fn permission_snapshot(_prompt: bool) -> PermissionSnapshot {
        PermissionSnapshot {
            accessibility: false,
            listen_events: false,
            post_events: false,
        }
    }

    pub fn ensure_accessibility(_prompt: bool) -> Result<(), String> {
        Err("Accessibility support is only implemented for macOS".into())
    }

    pub fn ensure_listen_access(_prompt: bool) -> Result<(), String> {
        Err("Global key capture is only implemented for macOS".into())
    }

    pub fn ensure_post_access(_prompt: bool) -> Result<(), String> {
        Err("Synthetic paste is only implemented for macOS".into())
    }

    pub fn read_focus_snapshot(_max_chars: usize) -> Result<FocusSnapshot, String> {
        Err("Focus inspection is only implemented for macOS".into())
    }

    pub fn paste_text_via_clipboard(_text: &str) -> Result<(), String> {
        Err("Paste injection is only implemented for macOS".into())
    }

    pub fn replace_last_chars(_prev_char_count: usize, _new_text: &str) -> Result<(), String> {
        Err("replace_last_chars is only implemented for macOS".into())
    }

    pub fn frontmost_pid() -> Option<i32> {
        None
    }

    pub fn activate_pid(_pid: i32) -> Result<(), String> {
        Err("activate_pid is only implemented for macOS".into())
    }

    pub fn monitor_trigger_key(_trigger: TriggerKey, _suppress_trigger: bool) -> Result<(), String> {
        let _ = HotkeyEvent::Pressed;
        Err("Hotkey monitoring is only implemented for macOS".into())
    }

    pub fn spawn_hotkey_stream(
        _trigger: TriggerKey,
        _suppress_trigger: bool,
    ) -> Result<mpsc::Receiver<HotkeyEvent>, String> {
        Err("Hotkey monitoring is only implemented for macOS".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;
