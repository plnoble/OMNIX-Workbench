use super::*;
use crate::proc::NoWindow;
use std::process::Command;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn set_compare_windows_layout(
    app: AppHandle,
    layout: Vec<WindowLayout>,
) -> Result<(), String> {
    use tauri::Manager;

    let main_win = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    let main_logical_pos = main_win
        .outer_position()
        .map(|p| p.to_logical::<f64>(main_win.scale_factor().unwrap_or(1.0)))
        .map_err(|e| e.to_string())?;

    for item in layout {
        let target_x = main_logical_pos.x + item.x;
        let target_y = main_logical_pos.y + item.y;

        if let Some(win) = app.get_webview_window(&item.label) {
            win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                item.width,
                item.height,
            )))
            .ok();
            win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(
                target_x, target_y,
            )))
            .ok();
            win.show().ok();
        } else {
            let url_parsed = item
                .url
                .parse()
                .map_err(|e| format!("Invalid URL: {}", e))?;

            let mut builder = tauri::WebviewWindowBuilder::new(
                &app,
                &item.label,
                tauri::WebviewUrl::External(url_parsed),
            )
            .decorations(false)
            .skip_taskbar(true)
            .inner_size(item.width, item.height)
            .position(target_x, target_y);

            builder = builder.owner(&main_win).map_err(|e| e.to_string())?;

            let _win = builder
                .build()
                .map_err(|e| format!("Failed to create compare webview: {}", e))?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn hide_compare_windows(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    for (label, win) in app.webview_windows() {
        if label.starts_with("expert-") {
            win.hide().ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn close_compare_windows(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    for (label, win) in app.webview_windows() {
        if label.starts_with("expert-") {
            win.close().ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn eval_compare_window(app: AppHandle, label: String, js: String) -> Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window(&label) {
        win.eval(&js).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn focus_main_window(app_handle: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let mut target_win = app_handle.get_webview_window("main");
    if target_win.is_none() {
        for (label, win) in app_handle.webview_windows() {
            if label != "status-dock" {
                target_win = Some(win);
                break;
            }
        }
    }
    if let Some(win) = target_win {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn pick_directory() -> Result<Option<String>, String> {
    if !cfg!(target_os = "windows") {
        return Err("Folder picker is currently implemented for Windows only.".to_string());
    }

    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择 OMNIX 工作区文件夹'
$dialog.ShowNewFolderButton = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  Write-Output $dialog.SelectedPath
}
"#;

    let output = Command::new("powershell.exe")
        .no_window()
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|e| format!("Failed to open folder picker: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Folder picker failed without an error message.".to_string()
        } else {
            stderr
        });
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(selected))
    }
}

#[tauri::command]
pub fn pick_file() -> Result<Option<String>, String> {
    if !cfg!(target_os = "windows") {
        return Err("File picker is currently implemented for Windows only.".to_string());
    }
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = '选择要导入 OMNIX 知识库的文件'
$dialog.Filter = '支持的文档|*.md;*.txt;*.pdf;*.docx;*.rs;*.py;*.js;*.ts;*.tsx;*.jsx;*.json|所有文件|*.*'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  Write-Output $dialog.FileName
}
"#;
    let output = Command::new("powershell.exe")
        .no_window()
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| format!("Failed to open file picker: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!selected.is_empty()).then_some(selected))
}

#[tauri::command]
pub fn toggle_status_dock(app_handle: AppHandle, visible: bool) -> Result<(), String> {
    use tauri::Manager;
    if let Some(dock) = app_handle.get_webview_window("status-dock") {
        if visible {
            let _ = dock.show();
            let _ = dock.set_focus();
        } else {
            let _ = dock.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_quick_assistant(app_handle: AppHandle, visible: bool) -> Result<(), String> {
    use tauri::Manager;
    if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
        if visible {
            let _ = qa.show();
            let _ = qa.set_focus();
            // Notify the QA window to read clipboard and prepare
            let _ = app_handle.emit("qa-shown", ());
        } else {
            let _ = qa.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn show_quick_assistant_with_text(app_handle: AppHandle, text: String) -> Result<(), String> {
    use tauri::Manager;
    if let Some(qa) = app_handle.get_webview_window("quick-assistant") {
        let _ = qa.show();
        let _ = qa.set_focus();
        // Send the text to the QA window via event
        let _ = app_handle.emit("qa-preset-text", text);
    }
    Ok(())
}
