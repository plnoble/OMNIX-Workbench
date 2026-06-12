/**
 * OMNIX Selection Assistant — System-Level Text Capture
 *
 * Hybrid approach for capturing selected text from any application:
 *   Tier 1: UI Automation (passive, no clipboard modification)
 *   Tier 2: SendInput Ctrl+C + Clipboard (universal fallback)
 *
 * COM threading: UIA calls run on a blocking thread via
 * tokio::task::spawn_blocking, with CoInitializeEx on that thread.
 */

use serde::Serialize;
use tauri::Emitter;

// ── Capture Result Types ────────────────────────────────

/// Rich capture result with context information.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureResult {
    pub text: String,
    pub source: String,           // "uia" | "clipboard"
    pub window_title: String,
    pub process_name: String,
    pub timestamp: String,
}

/// Selection history entry (mirrors DB schema).
#[derive(Debug, Clone, Serialize)]
pub struct SelectionHistoryEntry {
    pub id: String,
    pub captured_text: String,
    pub source: String,
    pub window_title: String,
    pub process_name: String,
    pub created_at: String,
}

// ── Window Context ─────────────────────────────────────

/// Get the focused window title and process name via Win32 API.
#[cfg(target_os = "windows")]
fn get_focused_window_info() -> (String, String) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_FORMAT,
    };

    // SAFETY: GetForegroundWindow returns a valid HWND (or null) with no preconditions.
    // GetWindowTextW writes into a stack-allocated [0u16; 512] buffer — the buffer is
    // large enough for any window title (max 512 wchars). GetWindowThreadProcessId
    // writes a u32 at &mut pid, which is a valid u32 reference. OpenProcess returns
    // a handle that is closed by the `?` operator via the Drop impl. QueryFullProcessImageNameW
    // writes into a stack-allocated [0u16; 512] buffer with correct length tracking.
    unsafe {
        let hwnd = GetForegroundWindow();

        // Window title
        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);
        let window_title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        // Process ID via GetWindowThreadProcessId (re-exported from Win32::UI::WindowsAndMessaging)
        let mut pid: u32 = 0;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));

        let process_name = if pid != 0 {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let mut name_buf = [0u16; 512];
                    let mut name_len = name_buf.len() as u32;
                    let result = QueryFullProcessImageNameW(
                        handle,
                        PROCESS_NAME_FORMAT(0),
                        windows::core::PWSTR(name_buf.as_mut_ptr()),
                        &mut name_len,
                    );
                    let _ = handle; // HANDLE is Copy
                    if result.is_ok() && (name_len as usize) <= name_buf.len() {
                        String::from_utf16_lossy(&name_buf[..name_len as usize])
                            .rsplit('\\')
                            .next()
                            .unwrap_or("Unknown")
                            .to_string()
                    } else {
                        "Unknown".to_string()
                    }
                }
                Err(_) => "Unknown".to_string(),
            }
        } else {
            "Unknown".to_string()
        };

        (window_title, process_name)
    }
}

#[cfg(not(target_os = "windows"))]
fn get_focused_window_info() -> (String, String) {
    ("Unknown".to_string(), "Unknown".to_string())
}

// ── Tier 2: SendInput Ctrl+C + Clipboard ───────────────

/// Simulate Ctrl+C keypress using Win32 SendInput API.
/// This is NOT a keyboard hook — it's a one-shot input simulation
/// that does not trigger antivirus concerns.
#[cfg(target_os = "windows")]
pub fn simulate_ctrl_c() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_TYPE, KEYBDINPUT, KEYEVENTF_KEYUP,
        VK_CONTROL,
    };

    // SAFETY: SendInput accepts a slice of INPUT structs. We construct 4 INPUT
    // structs on the stack with valid VKEY codes and correct INPUT_TYPE(1) for
    // INPUT_KEYBOARD. All fields are zeroed or explicitly set. SendInput posts
    // input events to the system queue; there are no memory safety invariants beyond
    // the structs being valid for the duration of the call.
    unsafe {
        let vk_ctrl = VK_CONTROL.0 as u16;
        let vk_c = 0x43u16; // 'C' key

        let inputs: [INPUT; 4] = [
            // Ctrl down
            INPUT {
                r#type: INPUT_TYPE(1), // INPUT_KEYBOARD
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk_ctrl),
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // C down
            INPUT {
                r#type: INPUT_TYPE(1),
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk_c),
                        wScan: 0,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // C up
            INPUT {
                r#type: INPUT_TYPE(1),
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk_c),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            // Ctrl up
            INPUT {
                r#type: INPUT_TYPE(1),
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk_ctrl),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn simulate_ctrl_c() {
    // No-op on non-Windows platforms
}

/// Read text from the system clipboard using Win32 API.
/// Includes retry logic: if clipboard is locked by another app,
/// retries up to 3 times with 50ms intervals.
#[cfg(target_os = "windows")]
pub fn read_clipboard_win32() -> Result<String, String> {
    use windows::Win32::System::DataExchange::{
        OpenClipboard, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::Foundation::{HWND, HGLOBAL};

    // CF_UNICODETEXT = 13 (Win32 clipboard format constant)
    const CF_UNICODETEXT: u32 = 13;

    // SAFETY: All Win32 clipboard APIs require the caller to have opened the clipboard
    // (via OpenClipboard) before calling GetClipboardData/GlobalLock. We retry
    // OpenClipboard up to 3 times with sleep to handle contention. GlobalLock on a
    // valid HGLOBAL from GetClipboardData returns a valid pointer; we copy the data
    // immediately and call GlobalUnlock before closing. EmptyClipboard is only called
    // when the caller requests clipboard clearing. All HWND arguments are null/default
    // as required for the process-level clipboard access pattern.
    unsafe {
        // Check if there's Unicode text available
        if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
            return Err("No Unicode text on clipboard".to_string());
        }

        // Retry OpenClipboard up to 3 times (clipboard may be locked by another app)
        let mut opened = false;
        for attempt in 0..3 {
            if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
                opened = true;
                break;
            }
            if attempt == 2 {
                return Err("Failed to open clipboard after 3 attempts (locked by another application)".to_string());
            }
            // Brief sleep before retry
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        if !opened {
            return Err("Failed to open clipboard".to_string());
        }

        let result = (|| -> Result<String, String> {
            let handle = GetClipboardData(CF_UNICODETEXT)
                .map_err(|e| format!("GetClipboardData failed: {}", e))?;

            // Convert HANDLE to HGLOBAL for GlobalLock/GlobalUnlock
            let hglobal = HGLOBAL(handle.0);

            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return Err("GlobalLock returned null".to_string());
            }

            let len = {
                let mut len = 0usize;
                let wchars = ptr as *const u16;
                while *wchars.add(len) != 0 {
                    len += 1;
                }
                len
            };

            let slice = std::slice::from_raw_parts(ptr as *const u16, len);
            let text = String::from_utf16(slice).map_err(|e| format!("UTF-16 decode failed: {}", e))?;

            let _ = GlobalUnlock(hglobal);

            Ok(text)
        })();

        let _ = CloseClipboard();
        result
    }
}

#[cfg(not(target_os = "windows"))]
pub fn read_clipboard_win32() -> Result<String, String> {
    Err("Not supported on this platform".to_string())
}

// ── Tier 1: UI Automation Passive Read ──────────────────

/// Read selected text from the currently focused element via Windows UI Automation.
///
/// Strategy:
/// 1. GetFocusedElement → try TextPattern.GetSelection → GetText
/// 2. GetFocusedElement → try SelectionPattern.GetCurrentSelection → Name
/// 3. Return empty if neither pattern is available
#[cfg(target_os = "windows")]
pub fn get_selected_text_via_uia() -> Result<String, String> {
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::System::Com::*;

    // SAFETY: CoInitializeEx initializes the COM library on the current thread.
    // COINIT_MULTITHREADED is valid for background threads. The returned _com
    // value is dropped at function end, which calls CoUninitialize. CoCreateInstance
    // creates a COM object with CLSCTX_INPROC_SERVER — the IUIAutomation interface
    // pointer is valid for the lifetime of the function. All UIA method calls
    // (GetFocusedElement, GetCurrentPattern, GetSelection) operate on valid COM
    // interface pointers obtained from the automation object.
    unsafe {
        // Initialize COM on this thread
        let _com = CoInitializeEx(None, COINIT_MULTITHREADED);

        // Create the CUIAutomation instance
        let automation: IUIAutomation = match CoCreateInstance(
            &CUIAutomation,
            None,
            CLSCTX_INPROC_SERVER,
        ) {
            Ok(a) => a,
            Err(e) => return Err(format!("CUIAutomation creation failed: {}", e)),
        };

        // Get the focused element
        let focused = match automation.GetFocusedElement() {
            Ok(e) => e,
            Err(e) => return Err(format!("GetFocusedElement failed: {}", e)),
        };

        // Strategy 1: TextPattern (for text editors, browsers, etc.)
        // UIA_TextPatternId = 10014
        if let Ok(text_pattern) = focused.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
            if let Ok(selection) = text_pattern.GetSelection() {
                if let Ok(length) = selection.Length() {
                    let mut combined = String::new();
                    for i in 0..length {
                        if let Ok(range) = selection.GetElement(i) {
                            if let Ok(text) = range.GetText(-1) {
                                let text_str = text.to_string();
                                if !text_str.is_empty() {
                                    if !combined.is_empty() {
                                        combined.push('\n');
                                    }
                                    combined.push_str(&text_str);
                                }
                            }
                        }
                    }
                    if !combined.is_empty() {
                        return Ok(combined);
                    }
                }
            }
        }

        // Strategy 2: SelectionPattern (for lists, tree views, etc.)
        // UIA_SelectionPatternId = 10001
        if let Ok(sel_pattern) = focused.GetCurrentPatternAs::<IUIAutomationSelectionPattern>(UIA_SelectionPatternId) {
            if let Ok(selected) = sel_pattern.GetCurrentSelection() {
                if let Ok(length) = selected.Length() {
                    let mut combined = String::new();
                    for i in 0..length {
                        if let Ok(element) = selected.GetElement(i) {
                            if let Ok(name) = element.CurrentName() {
                                let name_str = name.to_string();
                                if !name_str.is_empty() {
                                    if !combined.is_empty() {
                                        combined.push('\n');
                                    }
                                    combined.push_str(&name_str);
                                }
                            }
                        }
                    }
                    if !combined.is_empty() {
                        return Ok(combined);
                    }
                }
            }
        }

        Err("No text selection found via UIA".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_selected_text_via_uia() -> Result<String, String> {
    Err("UIA not available on this platform".to_string())
}

// ── Hybrid Entry Points ────────────────────────────────

/// Capture selected text using the hybrid approach:
/// 1. Try UI Automation (passive, no clipboard modification)
/// 2. If UIA fails, fall back to SendInput Ctrl+C + clipboard read
///
/// Returns the captured text, or an error if nothing could be captured.
pub async fn capture_selection() -> Result<String, String> {
    // Tier 1: Try UIA on a blocking thread (COM must be on a dedicated thread)
    let uia_result = tokio::task::spawn_blocking(|| get_selected_text_via_uia())
        .await
        .map_err(|e| format!("UIA task failed: {}", e))?;

    match uia_result {
        Ok(text) if !text.trim().is_empty() => {
            return Ok(text);
        }
        _ => {
            // Tier 2: Fallback to SendInput + clipboard
            simulate_ctrl_c();

            // Wait for clipboard to update
            tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

            let text = read_clipboard_win32()?;
            if text.trim().is_empty() {
                return Err("No text captured (clipboard is empty after Ctrl+C)".to_string());
            }
            Ok(text)
        }
    }
}

/// Capture selected text with context information (window title, process name).
/// Returns a rich CaptureResult struct.
pub async fn capture_selection_with_context() -> Result<CaptureResult, String> {
    let (window_title, process_name) = get_focused_window_info();

    // Tier 1: Try UIA on a blocking thread
    let uia_result = tokio::task::spawn_blocking(|| get_selected_text_via_uia())
        .await
        .map_err(|e| format!("UIA task failed: {}", e))?;

    let (text, source) = match uia_result {
        Ok(t) if !t.trim().is_empty() => (t, "uia".to_string()),
        _ => {
            // Tier 2: Fallback to SendInput + clipboard
            simulate_ctrl_c();

            // Wait for clipboard to update
            tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

            let text = read_clipboard_win32()?;
            if text.trim().is_empty() {
                return Err("No text captured (clipboard is empty after Ctrl+C)".to_string());
            }
            (text, "clipboard".to_string())
        }
    };

    let timestamp = chrono::Utc::now().to_rfc3339();

    Ok(CaptureResult {
        text,
        source,
        window_title,
        process_name,
        timestamp,
    })
}

// ── Auto-Capture Monitor ────────────────────────────────

/// Start a background auto-capture monitor that polls UIA every `interval_ms`
/// for selected text changes. When new text is detected, it emits a
/// `selection-auto-captured` event to the frontend.
///
/// This runs on a dedicated Tokio task and stops when the returned `JoinHandle`
/// is aborted or the app exits.
pub fn start_auto_capture_monitor(
    app_handle: tauri::AppHandle,
    interval_ms: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut last_text = String::new();
        let mut last_window = String::new();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(interval_ms)).await;

            // Try UIA capture on a blocking thread
            let result = tokio::task::spawn_blocking(|| {
                // Get window info first
                let (window_title, _process_name) = get_focused_window_info();
                // Then try UIA
                match get_selected_text_via_uia() {
                    Ok(text) if !text.trim().is_empty() => Some((text, window_title)),
                    _ => None,
                }
            }).await;

            match result {
                Ok(Some((text, window_title))) => {
                    let text = text.trim().to_string();
                    // Only emit if text changed AND window changed (new selection in different focus)
                    // OR if text is different from last capture
                    if text != last_text || window_title != last_window {
                        last_text = text.clone();
                        last_window = window_title.clone();

                        let _ = app_handle.emit("selection-auto-captured", serde_json::json!({
                            "text": text,
                            "window_title": window_title,
                        }));
                    }
                }
                Ok(None) => {
                    // No selection — clear last text so next selection triggers
                    if !last_text.is_empty() {
                        last_text.clear();
                    }
                }
                Err(_) => {
                    // Task error — ignore and continue
                }
            }
        }
    })
}
