use crate::agent::{AgentManager, DetectedAgent};
use crate::input_validation;
use crate::proc::NoWindow;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn detect_installed_agents(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<DetectedAgent>, String> {
    Ok(agent_manager.detect_agents())
}

#[tauri::command]
pub async fn install_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.install_agent(&agent_name).await
}

/// Checks each installed agent CLI's version against the latest published on
/// npm, so the UI can surface an "update available" badge. npm registry queries
/// run concurrently; a query failure (offline, private package) yields
/// `has_update: false` rather than a spurious prompt.
#[tauri::command]
pub async fn check_agent_updates(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<Vec<crate::agent::AgentUpdateInfo>, String> {
    let mut handles = Vec::new();
    for (name, version, package) in agents_worth_checking(agent_manager.detect_agents()) {
        handles.push(tokio::task::spawn_blocking(move || {
            let latest = npm_latest_version(package);
            update_info(name, version, package, latest)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(info) = handle.await {
            results.push(info);
        }
    }
    Ok(results)
}

/// 哪些 agent 值得去 npm 问一句版本：**装上了的**，且**确实由 npm 发布**。
///
/// 抽出来是为了能测——`detect_agents` 要摸真实文件系统，命令本身测不到。
fn agents_worth_checking(
    detected: Vec<DetectedAgent>,
) -> Vec<(String, String, &'static str)> {
    use crate::agent::npm_package_for_agent;
    detected
        .into_iter()
        .filter(|agent| agent.status == "installed")
        .filter_map(|agent| {
            npm_package_for_agent(&agent.name).map(|package| (agent.name, agent.version, package))
        })
        .collect()
}

/// 由「本地版本」和「npm 上最新版本」得出要不要提示更新。
///
/// `latest` 为 `None` 表示那次查询没成功（离线、私有包、超时）。这时必须给出
/// `has_update: false`——**查不到不等于有新版**，否则一断网整排 agent 都会挂上
/// 「有更新」的红点。
fn update_info(
    name: String,
    version: String,
    package: &'static str,
    latest: Option<String>,
) -> crate::agent::AgentUpdateInfo {
    use crate::agent::{extract_semver, semver_is_older, AgentUpdateInfo};
    let current = extract_semver(&version).unwrap_or(version);
    let has_update = latest
        .as_deref()
        .is_some_and(|latest| semver_is_older(&current, latest));
    AgentUpdateInfo {
        name,
        current,
        latest,
        has_update,
        package: Some(package.to_string()),
    }
}

/// Returns the latest published version of an npm package via `npm view`, or
/// `None` if the query fails (offline, not found, timeout).
fn npm_latest_version(package: &str) -> Option<String> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = std::process::Command::new(npm)
        .args(["view", package, "version"])
        .no_window()
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[tauri::command]
pub async fn uninstall_agent_cli(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.uninstall_agent(&agent_name).await
}

#[tauri::command]
pub async fn repair_installed_agent(
    agent_name: String,
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    input_validation::validate_name(&agent_name, "agent_name")?;
    agent_manager.repair_agent_cli(&agent_name).await
}

#[tauri::command]
pub fn sync_external_agent_configs(
    agent_manager: State<'_, Arc<AgentManager>>,
) -> Result<(), String> {
    agent_manager.sync_agent_configs()
}

// `get_active_agent_model` / `update_active_agent_model` 已删除：**全项目零调用方**，
// 只在 lib.rs 里注册着开在 IPC 上。而后者不是无害的死代码——它在找不到账号时会
// 凭空造一个 `is_active = 1` 的账号，api_key 取自 `settings.api_key`，那个键从
// 安装起就是空字符串、没有任何地方写过它。造出来的空 Key 账号会**挡住**
// `get_active_account_for_agent` 原本的兜底（改agent专属→任意活跃账号），于是
// 「给某个 agent 换个模型」能把这个 agent 的鉴权弄坏。顺带它还是唯一一条绕开
// `crypto::encrypt` 明文写 api_key 的路。
//
// 模型选择本来就走 `agent_accounts` 的正规保存路径（`save_agent_account_core`），
// 不需要这两条。

#[cfg(test)]
mod tests {
    use super::{agents_worth_checking, update_info};
    use crate::agent::DetectedAgent;

    fn agent(name: &str, version: &str, status: &str) -> DetectedAgent {
        DetectedAgent {
            name: name.to_string(),
            path: "/x".to_string(),
            version: version.to_string(),
            status: status.to_string(),
        }
    }

    /// 只问装上了的、且确实发在 npm 上的。
    ///
    /// 没装的去问一遍纯属浪费一次网络往返；不是 npm 分发的（本地构建、自带
    /// 二进制）根本没有「npm 上的最新版」这回事，问出来的结果没有意义。
    #[test]
    fn only_installed_npm_agents_get_queried() {
        let checked = agents_worth_checking(vec![
            agent("Claude Code", "1.0.0", "installed"),
            agent("Codex", "0.9.1", "not_installed"),
            agent("Gemini CLI", "0.3.0", "broken"),
            agent("某个本地 Agent", "1.0.0", "installed"), // 不在 npm 映射里
        ]);
        assert_eq!(
            checked
                .iter()
                .map(|(name, _, package)| (name.as_str(), *package))
                .collect::<Vec<_>>(),
            vec![("Claude Code", "@anthropic-ai/claude-code")]
        );
    }

    /// **查不到最新版 ≠ 有新版。**
    ///
    /// 离线、私有包、超时都会让 npm 查询失败。这时挂上「有更新」的红点是纯粹的
    /// 误报，而且是断一次网整排都亮——所以必须是 false。
    #[test]
    fn a_failed_npm_query_never_claims_an_update() {
        let info = update_info(
            "Claude Code".into(),
            "1.0.0".into(),
            "@anthropic-ai/claude-code",
            None,
        );
        assert!(!info.has_update);
        assert_eq!(info.latest, None);
        assert_eq!(info.current, "1.0.0");
    }

    /// 版本号从带杂音的 `--version` 输出里取，比较按 semver 而不是字符串。
    #[test]
    fn update_is_flagged_only_when_the_local_build_is_actually_older() {
        let cases = [
            ("codex-cli 0.9.1 (rust)", "0.10.0", true, "0.9.1"),
            ("1.0.0", "1.0.0", false, "1.0.0"),
            // 本地比线上新（自己构建的）不该提示回退
            ("1.2.0", "1.1.9", false, "1.2.0"),
            // 字符串比较会把 "0.9.1" 判成大于 "0.10.0"
            ("0.9.1", "0.10.0", true, "0.9.1"),
        ];
        for (raw, latest, expected, expected_current) in cases {
            let info = update_info(
                "Codex".into(),
                raw.into(),
                "@openai/codex",
                Some(latest.to_string()),
            );
            assert_eq!(info.has_update, expected, "{raw} vs {latest}");
            assert_eq!(info.current, expected_current, "{raw} 的版本号没提干净");
        }
    }
}
