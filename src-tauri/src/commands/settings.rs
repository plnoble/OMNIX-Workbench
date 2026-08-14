use tauri::State;
use std::sync::Arc;
use crate::db::DbManager;

/// `settings` 表里**不能经这两个通用命令进出**的键。
///
/// `get_app_setting` / `set_app_setting` 是任意键读写：它们直接开在 IPC 上，
/// 而 `settings` 表里躺着 `remote_token`——手机远程面板和局域网网关的那把令牌。
/// 也就是说 WebView 里任何一段脚本（或一次 XSS）都能
/// `invoke("get_app_setting", { key: "remote_token" })` 把它读走，再从局域网
/// 直连网关；写那一侧更糟，可以把令牌改成攻击者已知的值。
///
/// 用黑名单而不是白名单，是因为普通设置有几十个、还会继续加，白名单漏一个就是
/// 一个「设置保存了但读不回来」的怪 bug；而秘密类的键名是有限且好识别的。
/// 真正需要读写这些的地方都有各自的专用命令（`get_remote_access_info`、
/// `rotate_remote_token`…），它们不走这里。
fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("api_key")
        || k.contains("apikey")
        || k.contains("credential")
        || k.contains("private_key")
}

fn refuse(key: &str) -> String {
    format!("设置项「{key}」不能通过通用读写接口访问（它属于凭据类）")
}

#[tauri::command]
pub fn get_app_setting(
    key: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<Option<String>, String> {
    if is_secret_key(key) {
        return Err(refuse(key));
    }
    db.get_setting(key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_app_setting(
    key: &str,
    value: &str,
    db: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    if is_secret_key(key) {
        return Err(refuse(key));
    }
    db.set_setting(key, value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::is_secret_key;

    /// 远程令牌是这条闸的由来：它就躺在 `settings` 表里，而这两个命令是任意键
    /// 读写、直接开在 IPC 上。
    #[test]
    fn secret_keys_are_refused() {
        for key in [
            "remote_token",
            "REMOTE_TOKEN",
            "some_api_key",
            "openai_apikey",
            "db_password",
            "signing_secret",
            "aws_credential",
            "ssh_private_key",
        ] {
            assert!(is_secret_key(key), "{key} 应该被判为凭据类");
        }
    }

    /// 普通设置一个都不能被误伤——这里列的是前端实际在用的键。
    #[test]
    fn ordinary_settings_still_pass() {
        for key in [
            "theme_mode",
            "target_model",
            "auto_start",
            "start_to_tray",
            "gpu_acceleration",
            "idle_timeout_min",
            "remote_access_enabled",
            "onboarding_completed",
            "embedding_model",
            "default_model",
            "translate_model",
            "translate_prompt",
            "selection_assistant_shortcut",
            "selection_assistant_blacklist",
            "quick_assistant_width",
            "office_recent_files",
            "memory_gateway_recall",
            "skill_gateway_injection",
            "navigation_layout",
        ] {
            assert!(!is_secret_key(key), "{key} 是普通设置，不该被拦");
        }
    }
}
