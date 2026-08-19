//! Configurable storage locations (R1 存储位置中心).
//!
//! Users can point heavy/growing directories (backups, exports, the central
//! skill store) anywhere — e.g. off the C: drive. Overrides live in the
//! `settings` table and are mirrored into a process-wide cache at startup so
//! stateless helpers (backup.rs etc.) can resolve directories without a DB
//! handle. Only the SQLite file itself is pinned to `~/.omnix` (its path is
//! resolved before this cache exists). 生成的图片/视频**可以**挪走：
//! `tauri.conf.json` 里的 assetProtocol scope 是静态白名单，但 Tauri 2 提供了
//! `app.asset_protocol_scope().allow_directory(dir, true)`，启动时按用户配置
//! 追加放行即可。此前这里的注释断言"media can't move at runtime"，是个没验证过
//! 的假设，白白挡住了最该挪出 C 盘的那个目录。
//! The agents install root is the existing `sandbox_dir` setting — it is read
//! from the DB directly by agent.rs, so it needs no cache entry here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::db::DbManager;

/// (setting key, default subdirectory under ~/.omnix, 中文标签)
pub const STORAGE_KEYS: &[(&str, &str, &str)] = &[
    ("storage_backups_dir", "backups", "备份目录"),
    ("storage_exports_dir", "exports", "导出目录"),
    ("storage_skills_dir", "skills", "技能中央库"),
    ("storage_notes_dir", "notes", "笔记目录"),
    ("storage_media_dir", "media", "创作产物"),
];

fn overrides() -> &'static RwLock<HashMap<String, String>> {
    static CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Load overrides from settings once at startup (lib.rs setup).
pub fn init_from_db(db: &DbManager) {
    let mut map = HashMap::new();
    for (key, _, _) in STORAGE_KEYS {
        if let Ok(Some(value)) = db.get_setting(key) {
            if !value.trim().is_empty() {
                map.insert((*key).to_string(), value);
            }
        }
    }
    if let Ok(mut cache) = overrides().write() {
        *cache = map;
    }
}

/// Update the cache after the user changes a location (empty = back to default).
pub fn set_override(key: &str, value: &str) {
    if let Ok(mut cache) = overrides().write() {
        if value.trim().is_empty() {
            cache.remove(key);
        } else {
            cache.insert(key.to_string(), value.to_string());
        }
    }
}

pub fn omnix_root() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".omnix")
}

pub fn default_dir(key: &str) -> PathBuf {
    let sub = STORAGE_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, s, _)| *s)
        .unwrap_or("misc");
    omnix_root().join(sub)
}

/// Resolve a storage dir: user override if set, else the ~/.omnix default.
pub fn dir_for(key: &str) -> PathBuf {
    if let Ok(cache) = overrides().read() {
        if let Some(v) = cache.get(key) {
            return PathBuf::from(v);
        }
    }
    default_dir(key)
}

pub fn backups_dir() -> PathBuf {
    dir_for("storage_backups_dir")
}

pub fn exports_dir() -> PathBuf {
    dir_for("storage_exports_dir")
}

pub fn skills_dir() -> PathBuf {
    dir_for("storage_skills_dir")
}

pub fn notes_dir() -> PathBuf {
    dir_for("storage_notes_dir")
}

pub fn media_dir() -> PathBuf {
    dir_for("storage_media_dir")
}

/// Recursively copy a directory tree (shared by backup/migration/skill ops).
pub(crate) fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 打 OMNIX 自己网关（`127.0.0.1:<proxy_port>`）用的 HTTP 客户端。
///
/// **必须 `.no_proxy()`。** reqwest 默认会读系统代理，而 Windows 上它不解析
/// WinINET 的 ProxyOverride 绕行表——用户开着 Clash 一类的本地代理时
/// （`ProxyEnable=1, ProxyServer=127.0.0.1:7897`），我们发给自己 1421 端口的
/// 请求会被塞进代理绕一圈，失败时代理回的是一个**空正文的 502 Bad Gateway**，
/// 于是「翻译接口错误 502 Bad Gateway (模型 X):」后面什么都没有，换什么模型都一样。
///
/// 注意：这只管**内部回环**调用。网关自己去打真正的上游（api.anthropic.com 等）
/// 那个 client 必须保留系统代理，否则墙内用户根本连不上。
pub fn loopback_client(timeout: std::time::Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// 目标 URL 是不是本机回环——**判定的单一来源**。
///
/// 网关（`proxy.rs`）和所有直连上游的命令都用这一个，别再各写一份：同一个坑
/// 已经踩过两次（先是内部 1421 调用，再是 Ollama 探测与嵌入），每次都是因为
/// 「知道要 no_proxy」的知识只存在于某一个文件里。
pub fn url_targets_loopback(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    let Ok(ip) = host.parse::<std::net::IpAddr>() else {
        return false;
    };
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| mapped.is_loopback())
        }
    }
}

/// 按目标 URL 选客户端：回环绕开系统代理，其余保留。
///
/// 凡是打**用户配置的 `api_address`** 的地方都该用它——Ollama 是种子平台，
/// 它的地址就是 `http://localhost:11434`，而同一段代码换个平台又要打公网。
/// 写死任何一边都会错一半：写死 `.no_proxy()` 墙内连不上云 API，不写回环被
/// 代理劫成空 502。
pub fn client_for_url(url: &str, timeout: std::time::Duration) -> reqwest::Client {
    if url_targets_loopback(url) {
        return loopback_client(timeout);
    }
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_and_empty_resets() {
        set_override("storage_exports_dir", "D:/somewhere/exports");
        assert_eq!(
            dir_for("storage_exports_dir"),
            PathBuf::from("D:/somewhere/exports")
        );
        set_override("storage_exports_dir", "  ");
        assert_eq!(dir_for("storage_exports_dir"), default_dir("storage_exports_dir"));
    }

    /// 笔记曾经写死在 `~/.omnix/notes`，只能待在 C 盘——而这个模块存在的理由
    /// 就是「让会长大的目录挪得走」，笔记只是漏了没接进来。
    #[test]
    fn notes_can_be_moved_off_the_home_drive() {
        assert!(
            STORAGE_KEYS.iter().any(|(k, _, _)| *k == "storage_notes_dir"),
            "笔记不在可配置存储项里，用户就没法把它挪出 C 盘",
        );
        set_override("storage_notes_dir", "D:/elsewhere/notes");
        assert_eq!(notes_dir(), PathBuf::from("D:/elsewhere/notes"));
        set_override("storage_notes_dir", "");
        assert_eq!(notes_dir(), omnix_root().join("notes"));
    }

    /// 内部回环请求绝不能走系统代理：用户开着本地代理（Clash 一类）时，
    /// 发给自己 1421 端口的请求会被塞进代理，失败时只回一个空正文 502。
    #[test]
    fn loopback_client_never_uses_the_system_proxy() {
        // reqwest 不暴露"有没有代理"的读取接口，所以直接盯构造处的字面量。
        // 切片必须**只到函数体结束**：第一版一路切到文件末尾，把这条测试自己的
        // 代码也算了进去（里面同样有 `.no_proxy()` 字面量），于是永远是绿的——
        // 反向控制立刻暴露了这一点。
        let source = include_str!("storage.rs");
        let after_signature = source
            .split("pub fn loopback_client")
            .nth(1)
            .expect("loopback_client 应当存在");
        let builder = after_signature
            .split_once("
}")
            .map(|(body, _)| body)
            .expect("函数体应当有结束花括号");
        assert!(
            builder.contains(".no_proxy()"),
            "loopback_client 丢了 .no_proxy()——本机回环请求会被系统代理劫走",
        );
    }
}
