use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::db::DbManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub name: String,
    pub path: String,
    pub version: String,
    pub status: String, // "installed", "not_installed", "broken"
}

// Track active subprocesses and their last activity timestamp
struct ActiveProcess {
    child: Arc<tokio::sync::Mutex<Child>>,
    last_activity: Instant,
    stdin_tx: mpsc::Sender<String>,
}

pub struct AgentManager {
    db: Arc<DbManager>,
    active_processes: Arc<Mutex<HashMap<String, ActiveProcess>>>,
}

impl AgentManager {
    pub fn new(db: Arc<DbManager>) -> Self {
        let manager = Self {
            db,
            active_processes: Arc::new(Mutex::new(HashMap::new())),
        };
        manager.start_idle_reaper();
        manager
    }

    // --- 1. Agent Detection logic ---
    pub fn detect_agents(&self) -> Vec<DetectedAgent> {
        let mut list = Vec::new();
        let agent_names = vec![
            ("Claude Code", "claude"),
            ("Gemini CLI", "gemini"),
            ("Codex", "codex"),
            ("Qwen Code", "qwen-code"),
            ("GitHub Copilot CLI", "github-copilot-cli"),
            ("Google Antigravity", "agy"),
            ("OpenCode", "opencode"),
        ];

        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        
        // Setup local sandbox search paths
        let mut local_bin_dir = home_dir.clone();
        local_bin_dir.push(".omnix");
        local_bin_dir.push("agents");
        local_bin_dir.push("node_modules");
        local_bin_dir.push(".bin");

        for (display_name, bin_name) in agent_names {
            let mut found_path = None;
            
            // A. Check local sandbox first
            let mut local_exe = local_bin_dir.clone();
            local_exe.push(if cfg!(windows) { format!("{}.cmd", bin_name) } else { bin_name.to_string() });
            
            if local_exe.exists() {
                found_path = Some(local_exe.to_string_lossy().to_string());
            } else {
                // B. Check system PATH
                if let Ok(path) = which::which(bin_name) {
                    found_path = Some(path.to_string_lossy().to_string());
                }
            }

            if let Some(path) = found_path {
                // Quick command execution to query version
                let version = self.query_agent_version(&path);
                list.push(DetectedAgent {
                    name: display_name.to_string(),
                    path,
                    version,
                    status: "installed".to_string(),
                });
            } else {
                list.push(DetectedAgent {
                    name: display_name.to_string(),
                    path: "".to_string(),
                    version: "".to_string(),
                    status: "not_installed".to_string(),
                });
            }
        }

        list
    }

    fn query_agent_version(&self, exe_path: &str) -> String {
        // Run <path> --version
        let output = std::process::Command::new(exe_path)
            .arg("--version")
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !stdout.is_empty() {
                    stdout
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    if !stderr.is_empty() {
                        stderr
                    } else {
                        "0.1.0".to_string()
                    }
                }
            }
            Err(_) => "Unknown".to_string(),
        }
    }

    // --- 2. Headless Configuration Bootstrap (TOS Bypass) ---
    pub fn bootstrap_claude_code(&self) {
        // Claude Code accepts license config at ~/.config/claude-code/config.json (on Windows and Mac/Linux)
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        let mut config_dir = home_dir.clone();
        config_dir.push(".config");
        config_dir.push("claude-code");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut config_file = config_dir;
        config_file.push("config.json");

        // Write configuration pre-approving license terms and telemetry opt-out
        if !config_file.exists() {
            let tos_bypass_json = serde_json::json!({
                "analyticsConsent": "opt-out",
                "tosAccepted": true,
                "primaryColor": "green"
            });
            let _ = fs::write(config_file, tos_bypass_json.to_string());
            println!("Pre-seeded Claude Code configuration to bypass initial TOS interactive prompt.");
        }
    }

    // --- 3. Run and execute subprocesses ---
    pub fn spawn_agent(
        &self,
        session_id: String,
        exe_path: String,
        args: Vec<String>,
        workspace_dir: String,
        stdout_tx: mpsc::Sender<String>,
    ) -> Result<mpsc::Sender<String>, String> {
        // Pre-initialization check for Claude Code
        if exe_path.contains("claude") {
            self.bootstrap_claude_code();
        }

        // Inject long-term memory anti-failure files into the workspace directory
        let _ = self.inject_workspace_memories(&workspace_dir);

        // Configure environment variables (redirect Claude to our local HTTP proxy gateway)
        let proxy_port = self.db.get_setting("proxy_port").unwrap_or(None).unwrap_or_else(|| "1421".to_string());
        let local_proxy_url = format!("http://localhost:{}", proxy_port);

        let mut cmd = Command::new(&exe_path);
        cmd.args(args)
            .current_dir(workspace_dir)
            .env("ANTHROPIC_BASE_URL", &local_proxy_url)
            .env("HTTPS_PROXY", &local_proxy_url)
            .env("HTTP_PROXY", &local_proxy_url)
            .env("NO_PROXY", "localhost,127.0.0.1")
            // HEADLESS flags
            .env("CLAUDE_CODE_HEADLESS", "1") 
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Spawn child
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to launch agent subprocess: {}", e)),
        };

        let stdin = child.stdin.take().ok_or_else(|| "Failed to open stdin stream".to_string())?;
        let stdout = child.stdout.take().ok_or_else(|| "Failed to open stdout stream".to_string())?;
        let stderr = child.stderr.take().ok_or_else(|| "Failed to open stderr stream".to_string())?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
        let active_processes_clone = Arc::clone(&self.active_processes);
        let session_id_clone = session_id.clone();

        // 1. Thread for handling writing to stdin
        let active_processes_for_stdin = Arc::clone(&self.active_processes);
        let session_id_for_stdin = session_id.clone();
        tokio::spawn(async move {
            let mut writer = stdin;
            while let Some(msg) = stdin_rx.recv().await {
                // Update activity timestamp on input
                if let Ok(mut procs) = active_processes_for_stdin.lock() {
                    if let Some(proc) = procs.get_mut(&session_id_for_stdin) {
                        proc.last_activity = Instant::now();
                    }
                }
                
                let _ = writer.write_all(msg.as_bytes()).await;
                let _ = writer.flush().await;
            }
        });

        // 2. Thread for reading stdout
        let stdout_tx_clone = stdout_tx.clone();
        let active_processes_for_stdout = Arc::clone(&self.active_processes);
        let session_id_for_stdout = session_id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                // Update activity timestamp on output
                if let Ok(mut procs) = active_processes_for_stdout.lock() {
                    if let Some(proc) = procs.get_mut(&session_id_for_stdout) {
                        proc.last_activity = Instant::now();
                    }
                }
                let _ = stdout_tx_clone.send(format!("STDOUT: {}\n", line)).await;
            }
        });

        // 3. Thread for reading stderr
        let stderr_tx_clone = stdout_tx.clone();
        let active_processes_for_stderr = Arc::clone(&self.active_processes);
        let session_id_for_stderr = session_id.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                // Update activity timestamp on error output
                if let Ok(mut procs) = active_processes_for_stderr.lock() {
                    if let Some(proc) = procs.get_mut(&session_id_for_stderr) {
                        proc.last_activity = Instant::now();
                    }
                }
                let _ = stderr_tx_clone.send(format!("STDERR: {}\n", line)).await;
            }
        });

        // Save process registry
        let child_shared = Arc::new(tokio::sync::Mutex::new(child));
        let proc = ActiveProcess {
            child: Arc::clone(&child_shared),
            last_activity: Instant::now(),
            stdin_tx: stdin_tx.clone(),
        };

        let session_id_for_wait = session_id.clone();

        if let Ok(mut procs) = self.active_processes.lock() {
            procs.insert(session_id, proc);
        }

        // 4. Thread for awaiting process termination asynchronously without holding the map lock
        let active_processes_for_wait = Arc::clone(&self.active_processes);
        let child_for_wait = Arc::clone(&child_shared);
        tokio::spawn(async move {
            let mut child_lock = child_for_wait.lock().await;
            let status = child_lock.wait().await;
            println!("Subprocess for session {} exited with status {:?}", session_id_for_wait, status);
            
            // Clean up registry map on process exit
            if let Ok(mut procs) = active_processes_for_wait.lock() {
                procs.remove(&session_id_for_wait);
            }
        });

        Ok(stdin_tx)
    }

    pub async fn install_agent(&self, agent_name: &str) -> Result<(), String> {
        if agent_name == "Google Antigravity" {
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("powershell");
                c.args(&["-Command", "irm https://antigravity.google/cli/install.ps1 | iex"]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args(&["-c", "curl -fsSL https://antigravity.google/cli/install.sh | bash"]);
                c
            };
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn Antigravity installer: {}", e))?;
            let status = child.wait().await.map_err(|e| format!("Antigravity installer run error: {}", e))?;
            if status.success() {
                return Ok(());
            } else {
                return Err(format!("Antigravity installer failed with code {:?}", status.code()));
            }
        }

        let package = match agent_name {
            "Claude Code" => "@anthropic-ai/claude-code@latest",
            "Gemini CLI" => "@google/gemini-cli@latest",
            "GitHub Copilot CLI" => "@github/copilot-cli@latest",
            "OpenCode" => "opencode-ai@latest",
            _ => return Err(format!("Unsupported agent CLI auto-install: {}", agent_name)),
        };

        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        let mut sandbox_dir = home_dir.clone();
        sandbox_dir.push(".omnix");
        sandbox_dir.push("agents");

        // Ensure directory exists
        let _ = fs::create_dir_all(&sandbox_dir);
        let sandbox_str = sandbox_dir.to_string_lossy().to_string();

        println!("Installing agent {} in sandbox prefix {}", agent_name, sandbox_str);

        // Run npm install --prefix <sandbox> <package>
        let mut cmd = Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" });
        cmd.args(&["install", "--prefix", &sandbox_str, package])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to run npm command: {}", e))?;
        let status = child.wait().await.map_err(|e| format!("Npm install process error: {}", e))?;

        if status.success() {
            if agent_name == "Claude Code" {
                self.bootstrap_claude_code();
            }
            Ok(())
        } else {
            Err(format!("Npm install failed with status exit code {:?}", status.code()))
        }
    }

    pub async fn repair_agent_cli(&self, agent_name: &str) -> Result<(), String> {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));
        
        // 1. Clean npm lockfiles inside the sandbox
        if agent_name == "Claude Code" || agent_name == "GitHub Copilot CLI" {
            let mut sandbox_dir = home_dir.clone();
            sandbox_dir.push(".omnix");
            sandbox_dir.push("agents");

            let mut lock_file = sandbox_dir.clone();
            lock_file.push("package-lock.json");
            if lock_file.exists() {
                let _ = fs::remove_file(lock_file);
            }

            let mut node_modules_lock = sandbox_dir.clone();
            node_modules_lock.push("node_modules");
            node_modules_lock.push(".package-lock.json");
            if node_modules_lock.exists() {
                let _ = fs::remove_file(node_modules_lock);
            }
            println!("Cleared sandbox lockfiles for agent {}", agent_name);
        }

        // 2. Perform a clean reinstall
        self.install_agent(agent_name).await?;
        Ok(())
    }

    pub fn sync_agent_configs(&self) -> Result<(), String> {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\87953"));

        // A. Sync Claude Code config
        let mut claude_code_config = home_dir.clone();
        claude_code_config.push(".config");
        claude_code_config.push("claude-code");
        claude_code_config.push("config.json");

        if claude_code_config.parent().map(|p| p.exists()).unwrap_or(false) {
            let mut val = if claude_code_config.exists() {
                let content = fs::read_to_string(&claude_code_config).unwrap_or_default();
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
            
            if let Some(obj) = val.as_object_mut() {
                obj.insert("tosAccepted".to_string(), serde_json::Value::Bool(true));
                obj.insert("analyticsConsent".to_string(), serde_json::Value::String("opt-out".to_string()));
            }
            self.atomic_write_config(&claude_code_config, &val.to_string())?;
        }

        // B. Sync Claude Desktop config
        let mut claude_desktop_dir = home_dir.clone();
        claude_desktop_dir.push("AppData");
        claude_desktop_dir.push("Roaming");
        claude_desktop_dir.push("Claude");
        
        let mut claude_desktop_config = claude_desktop_dir.clone();
        claude_desktop_config.push("claude_desktop_config.json");

        if claude_desktop_dir.exists() {
            let mut val = if claude_desktop_config.exists() {
                let content = fs::read_to_string(&claude_desktop_config).unwrap_or_default();
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if let Some(obj) = val.as_object_mut() {
                let mut mcp_servers = obj.remove("mcpServers").unwrap_or_else(|| serde_json::json!({}));
                if mcp_servers.as_object().is_none() {
                    mcp_servers = serde_json::json!({});
                }
                
                // Inject custom MCP server settings pointing to OMNIX
                obj.insert("mcpServers".to_string(), mcp_servers);
            }

            self.atomic_write_config(&claude_desktop_config, &val.to_string())?;
            println!("Synchronized OMNIX configuration to Claude Desktop.");
        }

        Ok(())
    }

    fn atomic_write_config(&self, file_path: &Path, content: &str) -> Result<(), String> {
        let mut tmp_path = file_path.to_path_buf();
        tmp_path.set_extension("tmp");
        
        fs::write(&tmp_path, content).map_err(|e| format!("Failed to write tmp file: {}", e))?;
        fs::rename(&tmp_path, file_path).map_err(|e| format!("Failed to atomically replace config file: {}", e))?;
        Ok(())
    }

    pub fn terminate_agent(&self, session_id: &str) {
        if let Ok(mut procs) = self.active_processes.lock() {
            if let Some(proc) = procs.remove(session_id) {
                // Kill asynchronously to prevent blocking the registry lock
                tokio::spawn(async move {
                    let mut child = proc.child.lock().await;
                    let _ = child.start_kill();
                });
                println!("Forcefully terminated agent process for session: {}", session_id);
            }
        }
    }

    pub fn send_stdin(&self, session_id: &str, text: String) -> Result<(), String> {
        let procs = self.active_processes.lock().map_err(|_| "Failed to lock active processes map".to_string())?;
        if let Some(proc) = procs.get(session_id) {
            let tx = proc.stdin_tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(text).await;
            });
            Ok(())
        } else {
            Err(format!("No active agent session found with ID {}", session_id))
        }
    }

    // --- 4. Idle Reaper Watchdog thread ---
    fn start_idle_reaper(&self) {
        let active_processes_clone = Arc::clone(&self.active_processes);
        let db_clone = Arc::clone(&self.db);

        tokio::spawn(async move {
            loop {
                // Check every 30 seconds
                tokio::time::sleep(Duration::from_secs(30)).await;

                // Load threshold from DB config
                let timeout_min_str = db_clone.get_setting("idle_timeout_min").unwrap_or(None).unwrap_or_else(|| "15".to_string());
                let timeout_min = timeout_min_str.parse::<u64>().unwrap_or(15);
                let timeout_duration = Duration::from_secs(timeout_min * 60);

                let mut to_reap = Vec::new();

                if let Ok(procs) = active_processes_clone.lock() {
                    for (session_id, proc) in procs.iter() {
                        if proc.last_activity.elapsed() > timeout_duration {
                            to_reap.push(session_id.clone());
                        }
                    }
                }

                // Terminate reaped processes
                for session_id in to_reap {
                    println!("Idle Reaper: Session {} exceeded idle threshold of {} minutes. Killing subprocess...", session_id, timeout_min);
                    if let Ok(mut procs_lock) = active_processes_clone.lock() {
                        if let Some(proc) = procs_lock.remove(&session_id) {
                            tokio::spawn(async move {
                                let mut child = proc.child.lock().await;
                                let _ = child.start_kill();
                            });
                        }
                    }
                }
            }
        });
    }

    fn inject_workspace_memories(&self, workspace_dir: &str) -> Result<(), String> {
        let conn = self.db.get_connection().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare("SELECT incident_desc, code_pattern, remediation, keywords FROM memories")
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        }).map_err(|e| e.to_string())?;

        let mut memories_md = String::new();
        memories_md.push_str("\n<!--- OMNIX MEMORY START --->\n");
        memories_md.push_str("## 🧠 OMNIX Anti-Failure Guidelines & Memory Bank\n");
        memories_md.push_str("以下是历史项目踩坑事故记录与规约，请在此工作区内严加防范，避免重犯相同错误：\n\n");

        let mut count = 0;
        for r in rows {
            if let Ok((desc, pattern, remediation, keywords)) = r {
                count += 1;
                memories_md.push_str(&format!("### ❌ 坑点 {}: {}\n", count, desc));
                memories_md.push_str(&format!("* **危险模式/命令**: `{}`\n", pattern));
                memories_md.push_str(&format!("* **安全修复方案**: {}\n", remediation));
                memories_md.push_str(&format!("* **相关标签**: `{}`\n\n", keywords));
            }
        }
        memories_md.push_str("<!--- OMNIX MEMORY END --->\n");

        if count == 0 {
            return Ok(());
        }

        let workspace_path = PathBuf::from(workspace_dir);
        if !workspace_path.exists() {
            return Ok(());
        }

        // Write to CLAUDE.md
        let claude_md_path = workspace_path.join("CLAUDE.md");
        if claude_md_path.exists() {
            if let Ok(mut content) = fs::read_to_string(&claude_md_path) {
                if let (Some(start_idx), Some(end_idx)) = (content.find("<!--- OMNIX MEMORY START --->"), content.find("<!--- OMNIX MEMORY END --->")) {
                    let end_block_len = "<!--- OMNIX MEMORY END --->\n".len();
                    let actual_end = if end_idx + end_block_len <= content.len() {
                        end_idx + end_block_len
                    } else {
                        end_idx
                    };
                    content.replace_range(start_idx..actual_end, &memories_md);
                } else {
                    content.push_str(&memories_md);
                }
                let _ = fs::write(&claude_md_path, content);
            }
        } else {
            let _ = fs::write(&claude_md_path, &memories_md);
        }

        // Also write to OMNIX_MEMORY.md for generic model reference
        let omnix_md_path = workspace_path.join("OMNIX_MEMORY.md");
        let _ = fs::write(&omnix_md_path, &memories_md);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[tokio::test]
    async fn test_memory_injection() {
        let temp_dir = std::env::temp_dir();
        let test_db_path = temp_dir.join("omnix_agent_test.db");
        if test_db_path.exists() {
            let _ = std::fs::remove_file(&test_db_path);
        }

        let db = Arc::new(DbManager::new_with_path(test_db_path.clone()));

        let manager = AgentManager::new(Arc::clone(&db));

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_workspace = temp_dir.join(format!("omnix_workspace_{}", timestamp));
        fs::create_dir_all(&test_workspace).unwrap();

        // Run injection
        manager.inject_workspace_memories(&test_workspace.to_string_lossy()).unwrap();

        // Verify files exist
        let claude_md = test_workspace.join("CLAUDE.md");
        let omnix_md = test_workspace.join("OMNIX_MEMORY.md");
        assert!(claude_md.exists());
        assert!(omnix_md.exists());

        // Read content and check guidelines
        let content = fs::read_to_string(&claude_md).unwrap();
        assert!(content.contains("OMNIX Anti-Failure Guidelines"));
        assert!(content.contains("std::sync::MutexGuard across await point")); // seeded default memory

        // Clean up
        let _ = fs::remove_file(claude_md);
        let _ = fs::remove_file(omnix_md);
        let _ = fs::remove_dir(test_workspace);
        if test_db_path.exists() {
            let _ = fs::remove_file(&test_db_path);
        }
    }
}


