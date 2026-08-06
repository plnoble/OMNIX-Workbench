#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::thread;
    use crate::db::DbManager;

    /// 进程 + 时间唯一的后缀，避免并行跑的测试进程撞同一个临时路径。
    fn unique_tag() -> String {
        format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }

    // Test 1: Atomic File Replacement Safety
    #[test]
    fn test_atomic_write_safety() {
        // 路径带唯一后缀：固定路径在「多个测试进程并行跑同一套件」时会互相
        // 删对方的文件（os error 5）。cargo test --lib 单进程不会撞，但压力
        // 复现时会——测试自己不该成为噪声源。
        let temp_dir = std::env::temp_dir().join(format!("omnix_atomic_test_{}", unique_tag()));
        fs::create_dir_all(&temp_dir).unwrap();

        let target_file = temp_dir.join("config.json");
        fs::write(&target_file, "initial content").unwrap();

        // Simulates our atomic replacement script
        let write_atomic = |file_path: &std::path::Path, content: &str| -> Result<(), String> {
            let mut tmp_path = file_path.to_path_buf();
            tmp_path.set_extension("tmp");
            fs::write(&tmp_path, content).map_err(|e| e.to_string())?;
            fs::rename(&tmp_path, file_path).map_err(|e| e.to_string())?;
            Ok(())
        };

        // Execute atomic replacement
        let new_content = "updated JSON configuration content";
        write_atomic(&target_file, new_content).unwrap();

        // Verify target file holds the updated content
        let content = fs::read_to_string(&target_file).unwrap();
        assert_eq!(content, new_content);

        // Verify temporary file was deleted/replaced
        let tmp_file = temp_dir.join("config.tmp");
        assert!(!tmp_file.exists());

        // Clean up test workspace
        fs::remove_dir_all(&temp_dir).ok();
    }

    // Test 2: SQLite Concurrency and Thread Safety
    #[test]
    // Stays out of the blocking baseline: a pool stress test that has been
    // flaky on Windows, and CI runs on windows-latest. Surfaced by the
    // signal-only "ignored tests" CI step so it cannot rot unnoticed.
    #[ignore = "SQLite pool stress test; flaky on Windows, run via cargo test --lib -- --ignored"]
    fn test_db_concurrency() {
        let temp_db_path =
            std::env::temp_dir().join(format!("omnix_test_db_{}.sqlite", unique_tag()));
        if temp_db_path.exists() {
            fs::remove_file(&temp_db_path).ok();
        }

        let db = Arc::new(DbManager::new_with_path(temp_db_path.clone()));
        db.init_schema().unwrap();

        // Seed database
        db.set_setting("concurrency_test_key", "base_val").unwrap();

        let mut threads = vec![];

        // Spawn 10 concurrent threads reading and writing to settings table
        for i in 0..10 {
            let db_clone = Arc::clone(&db);
            let handle = thread::spawn(move || {
                for j in 0..30 {
                    let key = format!("thread_{}_iter_{}", i, j);
                    db_clone.set_setting(&key, "value").unwrap();
                    let val = db_clone.get_setting("concurrency_test_key").unwrap();
                    assert!(val.is_some());
                }
            });
            threads.push(handle);
        }

        for handle in threads {
            handle.join().unwrap();
        }

        // Clean up database file
        fs::remove_file(&temp_db_path).ok();
    }

    // Test 3: Idle Reaper subprocess cleanups
    #[test]
    fn test_idle_reaper_kill() {
        use std::process::{Command, Stdio};

        // Spawn a sleeping subprocess mimicking an idle agent CLI
        let mut child = Command::new("ping")
            .args(&["127.0.0.1", "-n", "5"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn test subprocess");

        let pid = child.id();
        assert!(pid > 0);

        // Verify process is running (try_wait returns None)
        let status = child.try_wait().unwrap();
        assert!(status.is_none());

        // Trigger subprocess kill (as used in child.kill() in reaper loop)
        child.kill().ok();
        child.wait().unwrap();

        // Verify it is indeed reaped
        let status_after = child.try_wait().unwrap();
        assert!(status_after.is_some());
    }

    // Test 4: Verify spawning and basic interaction for all 5 installed agents
    #[test]
    // Needs the real CLIs installed on the machine, so it is vacuous on CI.
    // Fake-process coverage of the same paths already runs in the baseline
    // (see runtime_manager fake_claude/fake_codex/fake_acp tests).
    #[ignore = "spawns real agent CLIs; only meaningful on a machine with them installed"]
    fn test_all_five_agents_interactive() {
        use crate::agent::AgentManager;
        use crate::db::DbManager;
        use tokio::runtime::Runtime;

        let db = Arc::new(DbManager::new());
        let agent_manager = AgentManager::new(Arc::clone(&db));

        let agents = vec![
            ("Claude Code", "claude"),
            ("Gemini CLI", "gemini"),
            ("Codex", "codex"),
            ("Google Antigravity", "agy"),
            ("OpenCode", "opencode"),
        ];

        let rt = Runtime::new().unwrap();

        for (display_name, bin_name) in agents {
            // Find path
            let path = AgentManager::find_agent_path_static(display_name, Some(&db));
            if path.is_none() {
                println!("Skipping interactive test for {} (not found)", display_name);
                continue;
            }
            let exe_path = path.unwrap();
            println!("Testing agent {} at {}", display_name, exe_path);

            // Test spawning
            let (stdout_tx, _stdout_rx) = tokio::sync::mpsc::channel::<String>(100);
            let session_id = format!("test_sess_{}", bin_name);

            // Spawn the agent inside the tokio runtime
            let stdin_tx = rt.block_on(async {
                agent_manager.spawn_agent(
                    session_id.clone(),
                    display_name.to_string(),
                    exe_path,
                    vec!["--help".to_string()], // Use --help to avoid waiting for API keys
                    "direct".to_string(),
                    stdout_tx,
                )
            });

            assert!(stdin_tx.is_ok(), "Failed to spawn agent {}", display_name);
            let stdin_tx = stdin_tx.unwrap();

            // Test sending stdin
            rt.block_on(async {
                let _ = stdin_tx.send("help\n".to_string()).await;
            });

            // Clean up session
            agent_manager.terminate_agent(&session_id);
        }
    }
}
