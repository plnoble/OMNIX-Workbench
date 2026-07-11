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
use tauri::{Emitter, Manager};

pub fn is_capture_blocked(blacklist: &str, process_name: &str, window_title: &str) -> bool {
    let entries: Vec<String> = serde_json::from_str(blacklist).unwrap_or_else(|_| {
        blacklist
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    });
    let process = process_name.to_lowercase();
    let title = window_title.to_lowercase();
    entries.iter().any(|entry| {
        let needle = entry.trim().to_lowercase();
        !needle.is_empty() && (process.contains(&needle) || title.contains(&needle))
    })
}

// ── Capture Result Types ────────────────────────────────

/// Rich capture result with context information.
#[derive(Debug, Clone, Serialize)]
pub struct CaptureResult {
    pub text: String,
    pub source: String, // "uia" | "clipboard"
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

/// Translation result — serializes to the `TranslateResponse` shape the frontend
/// reads (`translatedText` / `detectedLang` / `targetLang`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub translated_text: String,
    pub detected_lang: String,
    pub target_lang: String,
}

/// Translation history entry — fields match the frontend `TranslateHistoryEntry`.
#[derive(Debug, Clone, Serialize)]
pub struct TranslateHistoryEntry {
    pub id: String,
    pub source_text: String,
    pub target_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub created_at: String,
}

// ── Window Context ─────────────────────────────────────

/// Get the focused window title and process name via Win32 API.
#[cfg(target_os = "windows")]
fn get_focused_window_info() -> (String, String) {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

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
        SendInput, INPUT, INPUT_0, INPUT_TYPE, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL,
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
    use windows::Win32::Foundation::{HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

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
                return Err(
                    "Failed to open clipboard after 3 attempts (locked by another application)"
                        .to_string(),
                );
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
            let text =
                String::from_utf16(slice).map_err(|e| format!("UTF-16 decode failed: {}", e))?;

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
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Accessibility::*;

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
        let automation: IUIAutomation =
            match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
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
        if let Ok(text_pattern) =
            focused.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
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
        if let Ok(sel_pattern) =
            focused.GetCurrentPatternAs::<IUIAutomationSelectionPattern>(UIA_SelectionPatternId)
        {
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

/// Current mouse-cursor position in physical screen pixels, so the popup can
/// appear right next to the selection.
#[cfg(windows)]
fn get_cursor_position() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos writes into a valid POINT we own.
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        Some((point.x, point.y))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn get_cursor_position() -> Option<(i32, i32)> {
    None
}

/// Show the Quick Assistant popup at a screen point **without taking focus**
/// Stealing focus was what broke text selection and copy
/// and caused the flicker, so we never call `set_focus` here. Returns the popup's
/// screen rect (x, y, w, h) so the monitor can detect click-away.
fn show_popup_at(app_handle: &tauri::AppHandle, x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    let qa = app_handle.get_webview_window("quick-assistant")?;
    // Offset slightly down-right of the selection end.
    let px = x + 12;
    let py = y + 18;
    let _ = qa.set_position(tauri::PhysicalPosition::new(px, py));
    let _ = qa.show();
    // Intentionally NO set_focus() — see doc above.
    let (w, h) = qa
        .outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or((360, 220));
    Some((px, py, w, h))
}

/// Hide the popup (used when the user clicks away / deselects without acting).
fn hide_popup(app_handle: &tauri::AppHandle) {
    if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
        let _ = qa.hide();
    }
}

/// Is the left mouse button currently down? Read cheaply from the global key state
/// (no UIA, no clipboard) so the monitor can watch for selection-completed (button
/// release) without any side effects.
#[cfg(windows)]
fn left_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // VK_LBUTTON = 0x01; high-order bit set => the button is currently down.
    unsafe { (GetAsyncKeyState(0x01) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn left_button_down() -> bool {
    false
}

fn point_in_rect(p: (i32, i32), rect: (i32, i32, i32, i32), margin: i32) -> bool {
    let (x, y, w, h) = rect;
    p.0 >= x - margin && p.0 <= x + w + margin && p.1 >= y - margin && p.1 <= y + h + margin
}

/// Live popup geometry. The window is user-movable and user-resizable, so the
/// rect captured at show time goes stale — click-away decisions must use the
/// current bounds. Returns `None` when the window is missing or hidden.
fn current_popup_rect(app_handle: &tauri::AppHandle) -> Option<(i32, i32, i32, i32)> {
    let qa = app_handle.get_webview_window("quick-assistant")?;
    if !qa.is_visible().unwrap_or(false) {
        return None;
    }
    let pos = qa.outer_position().ok()?;
    let size = qa.outer_size().ok()?;
    Some((pos.x, pos.y, size.width as i32, size.height as i32))
}

/// Selection monitor.
///
/// Instead of continuously polling UIA (which made the popup flicker, follow the
/// cursor, and steal focus mid-selection — breaking copy), this watches the left
/// mouse button and acts only on **button release** (a completed selection). On
/// mouse-up it does a *single* UIA read; if there's a non-empty selection it shows
/// the popup **once**, **positioned once**, **without taking focus**. A mouse-down
/// outside the popup dismisses it. The button state is read cheaply via
/// GetAsyncKeyState — no UIA runs during a drag, so there's no flicker and the
/// clipboard is never touched.
///
/// Runs on a dedicated Tokio task; stops when the returned `JoinHandle` is aborted.
pub fn start_auto_capture_monitor(
    app_handle: tauri::AppHandle,
    poll_ms: u64,
) -> tokio::task::JoinHandle<()> {
    let poll_ms = poll_ms.clamp(30, 200);
    tokio::spawn(async move {
        let mut was_down = false;
        let mut popup_shown = false;
        let mut popup_rect = (0i32, 0i32, 0i32, 0i32);

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(poll_ms)).await;
            let down = left_button_down();
            let cursor = get_cursor_position().unwrap_or((0, 0));

            if popup_shown {
                // Refresh the live bounds every tick: the popup is draggable and
                // resizable, so the rect captured at show time goes stale and a
                // click inside the real window would otherwise dismiss it. This
                // also syncs state when the frontend hid the popup itself (Esc).
                match current_popup_rect(&app_handle) {
                    Some(rect) => popup_rect = rect,
                    None => popup_shown = false,
                }
            }

            if down && !was_down {
                // Mouse down: if a popup is open and the click is outside it, the
                // user is clicking away → dismiss.
                if popup_shown && !point_in_rect(cursor, popup_rect, 8) {
                    hide_popup(&app_handle);
                    popup_shown = false;
                }
            } else if !down && was_down {
                // Mouse up: a selection may have just completed. Let the app settle,
                // then do a SINGLE UIA read (never Ctrl+C — the clipboard stays yours).
                tokio::time::sleep(tokio::time::Duration::from_millis(90)).await;
                let up = get_cursor_position().unwrap_or(cursor);
                let cap = tokio::task::spawn_blocking(|| {
                    let (window_title, process_name) = get_focused_window_info();
                    match get_selected_text_via_uia() {
                        Ok(text) if !text.trim().is_empty() => {
                            Some((text.trim().to_string(), window_title, process_name))
                        }
                        _ => None,
                    }
                })
                .await
                .ok()
                .flatten();

                if let Some((text, window_title, process_name)) = cap {
                    // Never react to a selection inside our own popup.
                    let is_omnix = process_name.to_lowercase().contains("omnix")
                        || window_title.contains("Quick Assistant");
                    let blocked = app_handle
                        .try_state::<std::sync::Arc<crate::db::DbManager>>()
                        .and_then(|db| {
                            db.get_setting("selection_assistant_blacklist").ok().flatten()
                        })
                        .is_some_and(|bl| is_capture_blocked(&bl, &process_name, &window_title));
                    if !is_omnix && !blocked {
                        if let Some(rect) = show_popup_at(&app_handle, up.0, up.1) {
                            popup_rect = rect;
                            popup_shown = true;
                            let _ = app_handle.emit(
                                "selection-auto-captured",
                                serde_json::json!({
                                    "text": text,
                                    "window_title": window_title,
                                    "process_name": process_name,
                                }),
                            );
                        }
                    }
                }
            }
            was_down = down;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::is_capture_blocked;

    #[test]
    fn selection_blacklist_matches_process_or_window_case_insensitively() {
        let blacklist = r#"["Bitwarden","Windows Security"]"#;

        assert!(is_capture_blocked(blacklist, "Bitwarden.exe", "Vault"));
        assert!(is_capture_blocked(
            blacklist,
            "explorer.exe",
            "WINDOWS SECURITY"
        ));
        assert!(!is_capture_blocked(
            blacklist,
            "Code.exe",
            "OMNIX Workbench"
        ));
    }
}
