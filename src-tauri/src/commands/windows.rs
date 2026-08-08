use crate::proc::NoWindow;
use std::process::Command;
use tauri::{AppHandle, Emitter};

// 「AI 专家比对」的 Web 网页模式已删除，随之删掉四个只服务于它的命令：
// set_compare_windows_layout / hide_compare_windows / close_compare_windows /
// eval_compare_window。它们的活儿是把 expert-* 子 webview 覆盖到 DOM 占位框
// 上、并往别人的网页里 eval 硬编码选择器脚本——没有调用方之后留着只是一条
// 能往任意站点注入 JS 的口子。

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

/// 系统「打开文件」对话框，一处实现。`title` / `filter` 直接落进 PowerShell
/// 脚本，所以两者都不接受外部输入——只由本文件里的调用方写死字面量。
///
/// 走系统对话框而不是 `<input type="file">`，是因为浏览器的 File 对象拿不到
/// 真实路径，而 CLI agent 需要的恰恰是路径（它自己有 Read 工具）。
fn open_file_dialog(title: &str, filter: Option<&str>, multiselect: bool) -> Result<Vec<String>, String> {
    if !cfg!(target_os = "windows") {
        return Err("File picker is currently implemented for Windows only.".to_string());
    }

    let filter_line = filter
        .map(|value| format!("$dialog.Filter = '{value}'\n"))
        .unwrap_or_default();
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Title = '{title}'
$dialog.Multiselect = ${multiselect}
$dialog.CheckFileExists = $true
{filter_line}if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{
  $dialog.FileNames | ForEach-Object {{ Write-Output $_ }}
}}
"#
    );

    let output = Command::new("powershell.exe")
        .no_window()
        .args([
            "-NoProfile",
            "-STA",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|error| format!("Failed to open file picker: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "File picker failed without an error message.".to_string()
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// 「对话」页的附件选择器：任意类型、可多选，返回绝对路径列表。
#[tauri::command]
pub fn pick_files() -> Result<Vec<String>, String> {
    open_file_dialog("选择要交给 Agent 的文件", None, true)
}

#[tauri::command]
pub fn pick_file() -> Result<Option<String>, String> {
    let picked = open_file_dialog(
        "选择要导入 OMNIX 知识库的文件",
        Some("支持的文档|*.md;*.txt;*.pdf;*.docx;*.pptx;*.xlsx;*.rs;*.py;*.js;*.ts;*.tsx;*.jsx;*.json|所有文件|*.*"),
        false,
    )?;
    Ok(picked.into_iter().next())
}

/// 悬浮状态坞是否随应用启动。默认关（"0"/未设）——用户显式开启才创建。
pub const STATUS_DOCK_SETTING: &str = "status_dock_enabled";

/// 构造悬浮状态坞窗口（幂等：已存在则复用）。抽成函数供启动条件创建 + 开关
/// 后按需创建复用，两处不再各写一份 builder。
pub fn spawn_status_dock(app: &AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if app.get_webview_window("status-dock").is_some() {
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        app,
        "status-dock",
        tauri::WebviewUrl::App("/?window=status-dock".into()),
    )
    .title("OMNIX Status Dock")
    .inner_size(200.0, 48.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .resizable(false)
    .skip_taskbar(true)
    .build()
    .map_err(|e| format!("创建状态坞失败: {e}"))?;
    Ok(())
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

/// 开关悬浮状态坞（持久化 + 立即生效）。开启时按需创建并显示；关闭时关掉窗口。
/// 默认关，所以不会开机自启，除非用户在这里打开过。
#[tauri::command]
pub fn set_status_dock_enabled(
    app_handle: AppHandle,
    db: tauri::State<'_, std::sync::Arc<crate::db::DbManager>>,
    enabled: bool,
) -> Result<(), String> {
    use tauri::Manager;
    db.set_setting(STATUS_DOCK_SETTING, if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())?;
    if enabled {
        spawn_status_dock(&app_handle)?;
        if let Some(dock) = app_handle.get_webview_window("status-dock") {
            let _ = dock.show();
        }
    } else if let Some(dock) = app_handle.get_webview_window("status-dock") {
        let _ = dock.close();
    }
    Ok(())
}

#[tauri::command]
pub fn get_status_dock_enabled(
    db: tauri::State<'_, std::sync::Arc<crate::db::DbManager>>,
) -> Result<bool, String> {
    Ok(db
        .get_setting(STATUS_DOCK_SETTING)
        .unwrap_or(None)
        .as_deref()
        == Some("1"))
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
