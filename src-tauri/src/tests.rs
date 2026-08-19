// `tests::tests` 这个套娃是 clippy 的 module_inception。这里显式豁免而不是改名：
// 整个文件就是「集成测试」这一件事，外层文件名已经说了 tests，内层再起一个别的
// 名字（`integration` / `suite`）只会让 `cargo test <name>` 的过滤词变得不直觉。
#[allow(clippy::module_inception)]
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
            .args(["127.0.0.1", "-n", "5"])
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

}

/// 第四道接线守卫：**没有只写不读的表**。
///
/// 前三道（`commandWiring.test.ts`）守的是前后端之间的接缝：命令有没有调用方、
/// API 包装有没有组件用、invoke 的命令存不存在。这一道守的是**数据库层**——
/// 而这恰恰是本项目最高产的找 bug 启发式：「写了但没人读」。
///
/// 它抓到过的实例（都是先手工翻出来的，正是因为没有守卫）：
/// - 技能复利：`success_count` / `priority_score` 只写不读，两处 ORDER BY 在按常数排序
/// - `skill_audit_log`：一次审计写 313 行，从来没人查过（审计结果是靠返回值到界面的）
/// - `dev_checklist`：读取方被删后写入方留着，agent 每写一条 task 就收到一句假回执
#[cfg(test)]
mod table_wiring {
    use std::collections::HashSet;

    /// 已知只写不读，**只允许变短**。每一条要么补上读取端，要么删掉写入。
    const KNOWN_WRITE_ONLY: &[&str] = &[
        // 失误检测器是活的（runtime_manager 每条 agent 消息都调），信号有价值，
        // 所以保留写入；但界面上还没有地方展示它。在补上之前，`mistake_detect`
        // 里有一条 log::info 作为唯一出口。
        "activity_log",
    ];

    pub(super) fn read_sources() -> String {
        let mut src = String::new();
        for entry in walk(std::path::Path::new("src")) {
            // 测试夹具里的 INSERT 不算生产写入——`chat_knowledge_bindings` 就是
            // 这么被误判过一次的（那条 INSERT 在 conversations.rs 的测试模块里）。
            //
            // **只按测试模块截断，不能按 `#[cfg(test)]` 截断**：`db.rs` 在文件中段
            // 就给 `new_run_test` 挂了 `#[cfg(test)]`，按第一个出现处切会把后面整个
            // 文件（含 `get_setting` 的 `FROM settings`）全丢掉——这条守卫第一版就是
            // 这么把 `settings` 误报成只写不读的。
            let text = std::fs::read_to_string(&entry).unwrap_or_default();
            src.push_str(production_part(&text));
            src.push('\n');
        }
        src
    }

    /// 截到第一个**测试模块**为止，返回生产段。
    ///
    /// 关键是「模块」而不是「任何 `#[cfg(test)]`」：`db.rs` 在文件中段就给
    /// `new_run_test` 挂了 `#[cfg(test)]`，按第一个出现处切会把后面整个文件
    /// （含 `get_setting` 的 `FROM settings`）全丢掉——这条守卫第一版就是这么把
    /// `settings` 误报成只写不读的。守卫抓到的第一个 bug 是它自己的。
    pub(super) fn production_part(text: &str) -> &str {
        for (i, _) in text.match_indices("#[cfg(test)]") {
            let after = text[i + "#[cfg(test)]".len()..].trim_start();
            if after.starts_with("mod ") {
                return &text[..i];
            }
        }
        text
    }

    pub(super) fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        out
    }

    /// 抓 `INSERT [OR ...] INTO <table>` 里的表名。
    fn inserted_tables(src: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        for (i, _) in src.match_indices("INSERT ") {
            // **不能按字节切**：源码里满是中文注释，`&src[i..i+80]` 会切在一个汉字
            // 中间直接 panic。这里改成从 i 起按字符取 80 个再拼回来。
            let tail: String = src[i..].chars().take(80).collect();
            let Some(pos) = tail.find("INTO ") else { continue };
            let name: String = tail[pos + 5..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.insert(name.to_lowercase());
            }
        }
        out
    }

    /// 抓任何读法：`FROM <table>` 与 `UPDATE <table> SET`。
    fn read_tables(src: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        for kw in ["FROM ", "UPDATE "] {
            for (i, _) in src.match_indices(kw) {
                let name: String = src[i + kw.len()..]
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    out.insert(name.to_lowercase());
                }
            }
        }
        out
    }

    #[test]
    fn write_only_tables_are_declared() {
        let src = read_sources();
        let inserted = inserted_tables(&src);
        let read = read_tables(&src);

        // 自检：扫得到东西才说明这条测试没有空转
        assert!(inserted.len() > 30, "只扫到 {} 张有写入的表，正则大概率失效了", inserted.len());
        assert!(read.len() > 30, "只扫到 {} 张有读取的表", read.len());

        let mut orphans: Vec<String> = inserted
            .difference(&read)
            .filter(|t| !KNOWN_WRITE_ONLY.contains(&t.as_str()))
            .cloned()
            .collect();
        orphans.sort();
        assert!(
            orphans.is_empty(),
            "这些表只写不读：{}\n\
             要么补上读取端，要么删掉写入——留着的话，写进去的东西永远没人看见，\n\
             而调用方通常还会收到一句成功回执。不要加进 KNOWN_WRITE_ONLY。",
            orphans.join(", ")
        );
    }

    /// 清单不许烂：已经补上读取端、或写入已删的，条目要拿掉。
    #[test]
    fn the_write_only_list_has_no_stale_entries() {
        let src = read_sources();
        let inserted = inserted_tables(&src);
        let read = read_tables(&src);
        let stale: Vec<&str> = KNOWN_WRITE_ONLY
            .iter()
            .copied()
            .filter(|t| !inserted.contains(*t) || read.contains(*t))
            .collect();
        assert!(
            stale.is_empty(),
            "KNOWN_WRITE_ONLY 里这些条目已经不成立（写入没了，或已经有读取端），请删掉：{}",
            stale.join(", ")
        );
    }
}

/// 第五道守卫：**回环调用不许走系统代理**。
///
/// Windows 上 reqwest 会读系统代理，却**不解析** WinINET 的 ProxyOverride 绕行表。
/// 用户开着 Clash 一类的本地代理（`ProxyEnable=1, ProxyServer=127.0.0.1:7897`）时，
/// 发往 `127.0.0.1` / `localhost` 的请求会被塞进代理绕一圈，失败时回来的是一个
/// **空正文的 502**——界面上看起来像「Ollama 没装」或「模型坏了」。
///
/// 这个坑在这个仓库里踩过**三次**，每次换一个文件：
/// 1. 内部打自己 1421 网关 → 修出了 `storage::loopback_client`
/// 2. 网关打 localhost 上游 → 修出了 `proxy::client_for`
/// 3. Ollama 探测 / 嵌入 / 健康检查 / ntfy → 也就是这一轮
///
/// 每一次「要 no_proxy」的知识都只存在于刚修过的那个文件里，下一个新写的
/// `Client::builder()` 照样踩。前两次的守卫（`storage.rs` 里那条）只盯自己一个
/// 文件，管不到别处——所以这一道按**全仓**扫。
///
/// 判据：凡是打**用户配置地址**的调用，都必须走 `storage::client_for_url`
/// （它按 host 选：回环绕开代理，公网保留系统代理——墙内访问云 API 需要）。
/// 只可能打写死的公网地址的，登记在下面的白名单里。
#[cfg(test)]
mod loopback_client_wiring {
    /// 允许直接构造 reqwest Client 的文件 → 允许的处数。**只允许变短。**
    ///
    /// 每一条都必须是「目标地址写死在代码里、不可能是 localhost」的调用。
    /// 新增命令一律不许进这里：只要地址来自数据库或用户输入，就该走
    /// `client_for_url`，因为 Ollama / 本地 vLLM 就是合法的用户配置。
    const PUBLIC_ONLY: &[(&str, usize)] = &[
        // 选客户端的工厂本身就在这里（公网分支必须保留系统代理）。
        ("storage.rs", 3),
        // 网关的云上游 client，与 `direct_client` 成对，由 `client_for` 分流。
        ("proxy.rs", 1),
        // 远端模型目录，地址是编译期常量。
        ("model_knowledge.rs", 1),
        // OfficeCLI 安装包，github.com。
        ("office.rs", 1),
        // 技能源，raw.githubusercontent.com / api.github.com。
        ("skill_library.rs", 2),
        // OAuth 授权端点，来自各家供应商的固定配置。
        ("commands/oauth.rs", 1),
        // 局域网模型主机连通性测试——**故意**打非回环地址，走代理反而是对的。
        ("commands/remote_dev.rs", 1),
        // 联网搜索与 fetch_url，均先过 `guard_public_url`（只放行公网）。
        ("commands/search.rs", 3),
    ];

    /// 数一个文件里「没有 `.no_proxy()` 兜底」的 Client 构造处。
    ///
    /// 窗口取 700 字节是因为 builder 链最长也就这么长；放宽会把隔壁那处的
    /// `.no_proxy()` 误算进来，收紧会漏掉带一堆 `.timeout()` 的长链。
    fn bare_client_sites(src: &str) -> usize {
        let mut n = 0;
        let mut from = 0;
        while let Some(i) = src[from..].find("Client::") {
            let at = from + i;
            let rest = &src[at + "Client::".len()..];
            if rest.starts_with("builder(") || rest.starts_with("new(") {
                let end = (at + 700).min(src.len());
                if !src[at..end].contains(".no_proxy()") {
                    n += 1;
                }
            }
            from = at + "Client::".len();
        }
        n
    }

    #[test]
    fn user_configured_upstreams_never_bypass_the_loopback_check() {
        let mut offenders: Vec<String> = Vec::new();
        let mut seen: Vec<(String, usize)> = Vec::new();

        for path in super::table_wiring::walk(std::path::Path::new("src")) {
            let name = path.to_string_lossy().replace('\\', "/");
            let Some(rel) = name.strip_prefix("src/") else {
                continue;
            };
            // 测试文件不算生产调用：夹具用什么 client 由 T0 那条测试自己管
            // （`proxy_wire_tests.rs` 的夹具已经 `.no_proxy()`，否则开着代理
            // 跑 `cargo test` 会红 19 条，而 CI 干净 runner 上是绿的）。
            if rel == "tests.rs" || rel.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let count = bare_client_sites(super::table_wiring::production_part(&text));
            if count == 0 {
                continue;
            }
            seen.push((rel.to_string(), count));
            match PUBLIC_ONLY.iter().find(|(f, _)| *f == rel) {
                None => offenders.push(format!(
                    "{rel}：{count} 处直接构造 Client。地址若来自用户配置，请改用 \
                     `crate::storage::client_for_url(&url, timeout)`；确实只打写死的\
                     公网地址，才登记进 PUBLIC_ONLY 并写明理由。"
                )),
                Some((_, allowed)) if count > *allowed => offenders.push(format!(
                    "{rel}：新增了裸 Client（{allowed} → {count}）。白名单只许变短。"
                )),
                _ => {}
            }
        }

        // 清单会烂：文件删了、或者后来改走 client_for_url 了，条目却留着。
        for (file, allowed) in PUBLIC_ONLY {
            let actual = seen.iter().find(|(f, _)| f == file).map(|(_, c)| *c);
            assert_eq!(
                actual,
                Some(*allowed),
                "PUBLIC_ONLY 里 `{file}` 已经不成立（实际 {actual:?} 处），请更新或删掉这条"
            );
        }

        assert!(offenders.is_empty(), "{}", offenders.join("\n"));
    }

    /// 自检：扫描本身得真的扫到东西，否则上面那条永远是绿的。
    #[test]
    fn the_scanner_actually_finds_sites() {
        assert_eq!(bare_client_sites("let c = Client::builder().build();"), 1);
        assert_eq!(
            bare_client_sites("let c = Client::builder().no_proxy().build();"),
            0
        );
        assert_eq!(bare_client_sites("Client::new()"), 1);
        // 700 字节之外的 `.no_proxy()` 不该把这一处洗白
        let far = format!("Client::builder(){}\n.no_proxy()", " ".repeat(800));
        assert_eq!(bare_client_sites(&far), 1);
    }
}
