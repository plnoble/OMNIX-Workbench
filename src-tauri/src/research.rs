//! 深度研究：带证据的多跳检索。
//!
//! ## 为什么需要这一层
//!
//! OMNIX 早就有搜索（5 家）、抓网页（带 SSRF 笼）、分块检索。**缺的只有「迭代」**：
//! 搜一次拿结果就结束，没有「读完发现还差什么 → 再搜」。
//!
//! 这个模块**不做编排**。循环让 agent 自己跑（它本来就擅长），这里只提供三件事：
//! 开一次研究、记一条带证据的结论、把结论汇成带引文的报告。
//!
//! ## 引文为什么是结构性的而不是检查出来的
//!
//! 没有引文的多轮搜索只是贵几倍的普通搜索——这是 deep research 和「搜一下然后
//! 总结」的唯一实质区别。所以 `add_note` **拒绝**没有来源的结论，而报告是从
//! `research_notes` 拼出来的，不是让模型凭记忆复述。
//!
//! 于是「报告里每条结论都有出处」不需要事后校验：**没出处的结论根本进不了库**。

use crate::db::DbManager;
use rusqlite::params;

/// 来源等级。复用记忆库那套「分级可信度」的思路，不另造一套。
///
/// 排序即优先级：报告里同一个问题有冲突结论时，等级高的排前面。
pub const SOURCE_TIERS: &[&str] = &["official", "repo", "blog", "forum", "unknown"];

fn tier_rank(tier: &str) -> usize {
    SOURCE_TIERS
        .iter()
        .position(|t| *t == tier)
        .unwrap_or(SOURCE_TIERS.len() - 1)
}

fn normalize_tier(tier: &str) -> String {
    let t = tier.trim().to_ascii_lowercase();
    if SOURCE_TIERS.contains(&t.as_str()) {
        t
    } else {
        // 不认得的等级不报错，落成 unknown。研究过程中被一个拼错的枚举打断
        // 比排错序更糟——但也不能悄悄当成 official。
        "unknown".to_string()
    }
}

/// 开一次研究。
pub fn start(db: &DbManager, question: &str, workspace: &str) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("question 不能为空——写清楚你要查什么。".into());
    }
    let id = format!("rsr_{}", chrono::Utc::now().timestamp_micros());
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO research_runs (id, question, workspace_path) VALUES (?1, ?2, ?3)",
        params![id, question, workspace.trim()],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// 记一条**带证据**的结论。
///
/// 三个字段都必填，缺一不可：
/// - `claim` 没有就没有结论；
/// - `source_url` 没有就不是研究，是猜测；
/// - `snippet` 没有就没法核对——URL 会改会死，原文片段是留在本地的那份证据。
pub fn add_note(
    db: &DbManager,
    research_id: &str,
    claim: &str,
    source_url: &str,
    snippet: &str,
    tier: &str,
) -> Result<(), String> {
    let (claim, source_url, snippet) = (claim.trim(), source_url.trim(), snippet.trim());
    if claim.is_empty() {
        return Err("claim 不能为空。".into());
    }
    if source_url.is_empty() {
        return Err(
            "source_url 不能为空。没有出处的结论不是研究结果——要么找到出处，\
             要么别把它记成结论。"
                .into(),
        );
    }
    if !source_url.starts_with("http://") && !source_url.starts_with("https://") {
        return Err(format!("source_url 必须是 http(s) 地址，收到：{source_url}"));
    }
    if snippet.is_empty() {
        return Err(
            "snippet 不能为空——原文片段是留在本地的那份证据，URL 会改会死，它不会。"
                .into(),
        );
    }

    let conn = db.get_connection().map_err(|e| e.to_string())?;
    // 研究必须存在。不校验的话，一个拼错的 research_id 会让笔记落进一次不存在的
    // 研究里——外键会拦，但报错看不出原因。
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM research_runs WHERE id = ?1",
            params![research_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Err(format!(
            "找不到研究 {research_id}——先用 research_start 开一次。"
        ));
    }

    let id = format!("rsn_{}", chrono::Utc::now().timestamp_micros());
    conn.execute(
        "INSERT INTO research_notes (id, research_id, claim, source_url, snippet, tier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, research_id, claim, source_url, snippet, normalize_tier(tier)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 一条笔记（报告和测试都用它）。
#[derive(Debug, Clone)]
pub struct Note {
    pub claim: String,
    pub source_url: String,
    pub snippet: String,
    pub tier: String,
}

pub fn notes_of(db: &DbManager, research_id: &str) -> Result<Vec<Note>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT claim, source_url, snippet, tier FROM research_notes
             WHERE research_id = ?1 ORDER BY created_at ASC, rowid ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![research_id], |r| {
            Ok(Note {
                claim: r.get(0)?,
                source_url: r.get(1)?,
                snippet: r.get(2)?,
                tier: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.flatten().collect())
}

/// 出报告：从库里的笔记拼，**不让模型凭记忆复述**。
///
/// 结论按来源等级排（官方文档在前），每条挂 URL + 原文片段。同一个 URL 只在
/// 「来源」一节列一次。
pub fn report(db: &DbManager, research_id: &str) -> Result<String, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let question: String = conn
        .query_row(
            "SELECT question FROM research_runs WHERE id = ?1",
            params![research_id],
            |r| r.get(0),
        )
        .map_err(|_| format!("找不到研究 {research_id}。"))?;
    drop(conn);

    let mut notes = notes_of(db, research_id)?;
    if notes.is_empty() {
        // 空报告要说清楚是空的，不能给一份看起来完整的空壳。
        return Ok(format!(
            "# 研究：{question}\n\n**还没有任何带证据的结论。**\n\n\
             用 `research_note` 记下结论时必须同时给出 `source_url` 和 `snippet`——\
             没有出处的结论不会被收录，报告也就不会凭空生成内容。\n"
        ));
    }
    // 稳定排序：等级高的在前，同级保持记录顺序。
    notes.sort_by_key(|n| tier_rank(&n.tier));

    let mut out = format!("# 研究：{question}\n\n## 结论\n\n");
    let mut sources: Vec<(String, String)> = Vec::new();
    for (i, n) in notes.iter().enumerate() {
        let idx = match sources.iter().position(|(u, _)| *u == n.source_url) {
            Some(p) => p + 1,
            None => {
                sources.push((n.source_url.clone(), n.tier.clone()));
                sources.len()
            }
        };
        out.push_str(&format!("{}. {} [^{}]\n", i + 1, n.claim, idx));
        out.push_str(&format!("   > {}\n\n", n.snippet.replace('\n', " ")));
    }

    out.push_str("## 来源\n\n");
    for (i, (url, tier)) in sources.iter().enumerate() {
        out.push_str(&format!("[^{}]: {} （{}）\n", i + 1, url, tier_label(tier)));
    }
    Ok(out)
}

fn tier_label(tier: &str) -> &'static str {
    match tier {
        "official" => "官方文档",
        "repo" => "一手仓库",
        "blog" => "技术博客",
        "forum" => "论坛/问答",
        _ => "未标注等级",
    }
}

/// 收尾。
pub fn finish(db: &DbManager, research_id: &str) -> Result<(), String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE research_runs SET status = 'done', finished_at = CURRENT_TIMESTAMP
         WHERE id = ?1",
        params![research_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db(tag: &str) -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_research_{tag}_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        DbManager::new_with_path(path)
    }

    fn run(db: &DbManager) -> String {
        start(db, "reqwest 在 Windows 上认不认 ProxyOverride", "D:/proj").unwrap()
    }

    // ── 没出处的东西进不了库 ────────────────────────────────────────────

    /// **这条是整个设计的支点。** 没有引文的多轮搜索只是贵几倍的普通搜索。
    /// 所以拦在写入口，而不是出报告时再筛——筛的时候模型已经把没出处的话写进去了。
    #[test]
    fn a_claim_without_a_source_is_refused() {
        let db = test_db("nosrc");
        let id = run(&db);
        let err = add_note(&db, &id, "它不认 ProxyOverride", "", "片段", "official")
            .expect_err("没有出处的结论被收下了");
        // 断言那句**讲道理**的话，不是笼统的 "source_url"。
        //
        // 第一版断言的是 `err.contains("source_url")`——反向验证时把空值检查删掉，
        // 测试照样绿：空串也不以 http 开头，后面那道格式检查会接住它，而两条错误
        // 信息里都含 "source_url"。也就是说那条断言分不出「拦住了」和「拦对了」。
        //
        // 这里要的不只是拦住：格式错误的提示教不会模型规矩，这句才教得会。
        assert!(
            err.contains("没有出处的结论不是研究结果"),
            "拦是拦住了，但没告诉模型为什么不能这么干：{err}"
        );
    }

    /// 片段是留在本地的那份证据：URL 会改会死，片段不会。
    #[test]
    fn a_claim_without_a_snippet_is_refused() {
        let db = test_db("nosnip");
        let id = run(&db);
        assert!(add_note(&db, &id, "结论", "https://example.com", "", "repo").is_err());
    }

    #[test]
    fn a_non_http_source_is_refused() {
        let db = test_db("scheme");
        let id = run(&db);
        for bad in ["file:///etc/passwd", "内部资料", "ftp://x/y"] {
            assert!(
                add_note(&db, &id, "结论", bad, "片段", "repo").is_err(),
                "{bad} 被当成了合法出处"
            );
        }
    }

    /// 拼错 research_id 时要说人话，而不是把外键错误抛给用户。
    #[test]
    fn a_note_for_an_unknown_research_is_refused_with_a_readable_error() {
        let db = test_db("unknown");
        let err = add_note(&db, "rsr_typo", "结论", "https://example.com", "片段", "repo")
            .expect_err("落进了不存在的研究");
        assert!(err.contains("research_start"), "错误信息没告诉人怎么办：{err}");
    }

    /// 不认得的等级落成 unknown——**但绝不能当成 official**。
    /// 研究过程被一个拼错的枚举打断比排错序更糟，所以不报错；可要是悄悄升级成
    /// 最高等级，报告的排序就成了假的。
    #[test]
    fn an_unrecognised_tier_degrades_to_unknown_never_to_official() {
        let db = test_db("tier");
        let id = run(&db);
        add_note(&db, &id, "结论", "https://example.com", "片段", "AUTHORITATIVE").unwrap();
        let notes = notes_of(&db, &id).unwrap();
        assert_eq!(notes[0].tier, "unknown");
    }

    #[test]
    fn tiers_are_case_insensitive() {
        let db = test_db("case");
        let id = run(&db);
        add_note(&db, &id, "结论", "https://example.com", "片段", "  Official ").unwrap();
        assert_eq!(notes_of(&db, &id).unwrap()[0].tier, "official");
    }

    // ── 报告 ──────────────────────────────────────────────────────────

    /// **报告里的每条结论都必须挂得上出处。**
    ///
    /// 这不是靠事后检查保证的，是靠「没出处的结论进不了库」+「报告从库里拼」
    /// ——所以这条测的其实是：报告确实是从库里拼的，没有别的来源。
    #[test]
    fn every_claim_in_the_report_carries_a_citation() {
        let db = test_db("cite");
        let id = run(&db);
        add_note(&db, &id, "reqwest 读系统代理", "https://docs.rs/reqwest", "reads proxy", "official").unwrap();
        add_note(&db, &id, "但不解析 ProxyOverride", "https://github.com/seanmonstar/reqwest", "no override", "repo").unwrap();

        let md = report(&db, &id).unwrap();
        for claim in ["reqwest 读系统代理", "但不解析 ProxyOverride"] {
            let line = md
                .lines()
                .find(|l| l.contains(claim))
                .unwrap_or_else(|| panic!("报告里没有这条结论：{claim}"));
            assert!(line.contains("[^"), "结论没挂引文：{line}");
        }
        assert!(md.contains("https://docs.rs/reqwest"));
        assert!(md.contains("https://github.com/seanmonstar/reqwest"));
    }

    /// 官方文档排在论坛回答前面。
    #[test]
    fn higher_tier_conclusions_come_first() {
        let db = test_db("order");
        let id = run(&db);
        add_note(&db, &id, "论坛说法", "https://forum.example/1", "片段", "forum").unwrap();
        add_note(&db, &id, "官方说法", "https://docs.example/1", "片段", "official").unwrap();

        let md = report(&db, &id).unwrap();
        let official = md.find("官方说法").unwrap();
        let forum = md.find("论坛说法").unwrap();
        assert!(official < forum, "官方文档没排在论坛前面");
    }

    /// 同一个 URL 在「来源」一节只列一次。
    #[test]
    fn one_url_is_listed_once_even_if_several_claims_use_it() {
        let db = test_db("dedupe");
        let id = run(&db);
        for claim in ["结论一", "结论二"] {
            add_note(&db, &id, claim, "https://same.example/doc", "片段", "official").unwrap();
        }
        let md = report(&db, &id).unwrap();
        assert_eq!(md.matches("https://same.example/doc").count(), 1);
        // 两条结论都指向同一个脚注编号。
        assert_eq!(md.matches("[^1]").count(), 3, "两条正文 + 一条来源定义");
    }

    /// **空研究要说自己是空的**，不能给一份看起来完整的空壳报告。
    /// 「产出了报告」和「产出了有内容的报告」是两回事，用户只看得到前者。
    #[test]
    fn an_empty_research_says_so_instead_of_looking_finished() {
        let db = test_db("empty");
        let id = run(&db);
        let md = report(&db, &id).unwrap();
        assert!(md.contains("还没有任何带证据的结论"), "{md}");
        assert!(!md.contains("## 结论"), "空报告不该摆出结论小节");
    }

    #[test]
    fn a_report_for_an_unknown_research_is_an_error_not_an_empty_page() {
        let db = test_db("norun");
        assert!(report(&db, "rsr_nope").is_err());
    }

    /// 多跳：结论来自**不同的**来源，汇进同一份报告——这是迭代检索的产物形态。
    /// （循环本身由 agent 跑，这里验的是产物能承载多跳的结果。）
    #[test]
    fn conclusions_from_several_hops_land_in_one_report() {
        let db = test_db("hops");
        let id = run(&db);
        add_note(&db, &id, "第一跳：A 里提到了 B", "https://a.example", "见 B", "repo").unwrap();
        add_note(&db, &id, "第二跳：答案在 B 里", "https://b.example", "答案", "official").unwrap();

        let md = report(&db, &id).unwrap();
        assert!(md.contains("第一跳") && md.contains("第二跳"));
        assert_eq!(md.matches("[^").count(), 4, "两条正文引用 + 两条来源定义");
    }

    #[test]
    fn finishing_marks_the_run_done() {
        let db = test_db("finish");
        let id = run(&db);
        finish(&db, &id).unwrap();
        let status: String = db
            .get_connection()
            .unwrap()
            .query_row(
                "SELECT status FROM research_runs WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "done");
    }
}
