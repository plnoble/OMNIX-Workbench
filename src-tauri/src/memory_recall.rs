//! 记忆自动召回（借鉴 jcode：相关记忆自动浮现进上下文）。
//!
//! 设计取舍：`memories` 表按 `keywords`（逗号标签）+ `incident_desc` 存，**没有**
//! embedding 列。给它硬接一套向量管线（生成/迁移/依赖 embedding 模型）是「加机器」，
//! 违背「纯接线」原则，也会让离线/未配 embedding 模型的用户用不了。所以这里按
//! memories 本来的**词法模型**做召回——和 `skill_library::match_skills_for_message`
//! 同一套打分思路，经 `proxy::inject_official_skills` 同一条链路注入 system。
//!
//! 注入是**克制**的：默认关（用户显式开启才生效），最多 3 条，只在真有词命中时才注，
//! 绝不喧宾夺主。防火墙侧仍视记忆内容为「写入时为真」的背景，不是可执行指令。
//!
//! ## S1 记忆固化：让「这条教训没用」这件事真的产生后果
//!
//! 表里早就有两个字段记着记忆的成色，但召回这一端一个都没看：
//!
//! - `status`：`consolidate_memories` 把近似重复标成 `merged`，可这里的查询
//!   没有 WHERE，合并掉的那条照样被召回注入——**去重的结果被唯一的消费方丢掉了**。
//! - `repeated_count`：同一个错误在这条教训在册期间又犯了几次
//!   （见 `project_protocol::bump_repeated_lessons`）。记忆中心显示成「失效 ×N」
//!   徽章，但注入时它和一条从没失效过的记忆权重完全相同。
//!
//! 这里补的就是这两根线。**不写库**——退场是打分的结果，不是新状态：
//! 记忆库没有「恢复」入口，写死一个 `status='ineffective'` 就是没有回头路的单向门。
//! 纯算权重则天然可逆：教训被改写或被合并加权后自己就回来了。

use crate::db::DbManager;

#[derive(Debug, Clone)]
pub struct MemoryMatch {
    pub incident_desc: String,
    pub code_pattern: String,
    pub remediation: String,
    pub score: f32,
}

/// 可信度权重：这条教训在册期间，同一个错误又犯了几次。
///
/// `confidence / (confidence + repeated_count)`——没失效过是 1.0，
/// 失效次数越多越低；被合并加权过（confidence 更高）的记忆能扛住更多次失效，
/// 因为那说明这个坑确实常见，问题更可能出在教训写得不够，而不是它没用。
pub fn veracity(confidence: f64, repeated_count: i64) -> f32 {
    let confidence = confidence.max(0.01);
    let repeats = repeated_count.max(0) as f64;
    (confidence / (confidence + repeats)) as f32
}

/// T1 证据权重：结论对得上网关记录的真实工具调用（`acted`）的记忆，
/// 比只在会话里被声称过（`claimed`）的更可信。
///
/// 借鉴 GenericAgent 的「无行动，不记忆」。OMNIX 不像 GA 那样把未验证的直接
/// 拒之门外——蒸馏前面还有一道人工审核闸，一刀切会把「人看过觉得对、只是没有
/// 机器记录」的经验也误伤掉。所以这里是**降权**不是丢弃：让有一手证据的排前面。
fn evidence_weight(verified: &str) -> f32 {
    if verified == "acted" {
        1.0
    } else {
        0.7
    }
}

/// T2：当不了触发词的泛化词。
///
/// 借鉴 GenericAgent 的触发词判定：
///
/// > 假设用户说出这个词，你能否想到去查对应 SOP？能→直觉(不需要)；不能→反直觉(必须留)
///
/// 「错误」「问题」「配置」这类词满足不了它——用户说「配置」时想不到该查哪条经验，
/// 因为**每条**经验都能沾上。它们不是把记忆钩出来，是让所有记忆一起浮上来，
/// 于是那 3 个注入名额被噪声占满，真正相关的那条反而挤不进去。
///
/// 所以泛化词命中只给零头分，不当作真命中。不是删掉它们——蒸馏出的关键词里
/// 混一两个泛化词很常见，整条丢掉会连同好词一起丢。
const GENERIC_KEYWORDS: &[&str] = &[
    "错误", "问题", "配置", "代码", "功能", "开发", "修复", "优化", "使用", "实现",
    "文件", "数据", "接口", "服务", "系统", "项目", "测试", "工具",
    "error", "issue", "config", "code", "bug", "fix", "test", "data", "file", "api",
];

fn is_generic(keyword: &str) -> bool {
    GENERIC_KEYWORDS.contains(&keyword)
}

/// 低于这条线就不再注入。
///
/// 等价于「失效次数超过 confidence 的三倍」：一条全新记忆失效 4 次即退场，
/// 一条被合并过两轮（confidence 2.0）的要 7 次。退场只是不再占那 3 个注入名额，
/// 记忆本身仍在库里、仍带着「失效 ×N」徽章——留给用户改写，而不是替他删掉。
const VERACITY_FLOOR: f32 = 0.25;

/// 词法打分：用户消息 vs 记忆的 关键词 / 现象描述 / 危险模式。
/// 关键词命中权重最高（它就是为召回而设的标签），其次现象与模式。
/// 最后乘 [`veracity`] × [`evidence_weight`]——一直没拦住的、以及没有一手
/// 动作证据的，都应当往后排。
pub fn match_memories_for_message(db: &DbManager, message: &str, limit: usize) -> Vec<MemoryMatch> {
    let Ok(conn) = db.get_connection() else {
        return Vec::new();
    };
    let mut stmt = match conn.prepare(
        // 与 `consolidate_core` / `bump_repeated_lessons` 用同一个「还在生效」谓词，
        // 免得三处对「active」的理解各走各的。
        "SELECT incident_desc, code_pattern, remediation, keywords, confidence, repeated_count,
                COALESCE(verified, 'claimed')
         FROM memories
         WHERE status = 'active' OR status IS NULL OR status = ''",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows: Vec<(String, String, String, String, f64, i64, String)> = match stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get::<_, Option<f64>>(4)?.unwrap_or(1.0),
            r.get::<_, Option<i64>>(5)?.unwrap_or(0),
            r.get::<_, String>(6)?,
        ))
    }) {
        Ok(r) => r.flatten().collect(),
        Err(_) => return Vec::new(),
    };

    let message_lower = message.to_lowercase();
    // 这里的切法**只**保留了「CJK 不是分隔符」，中文连写段仍然整段成一个 token：
    // 「为什么我的异步任务会死锁」切出来就是它自己。于是下面 `incident_lower
    // .contains(w)` 变成「整句话得是现象描述的子串」，永远不成立。
    //
    // 之所以一直没暴露，是因为上面关键词那条走的是**反方向**
    // （`message_lower.contains(&kw)`，标签短、消息长，能中）。可记忆库的标签
    // 习惯是 ASCII 技术词（`tokio,lock,deadlock`、`cors,fetch,credentials`），
    // 中文提问一个都不含——捷径断了，就只剩这条断掉的路。实测三条纯中文提问
    // 对口的记忆一条都召不回，见 `recall_ranking_corpus`。
    //
    // 和技能匹配用同一套 CJK 二元切分。`message_lower` 保持原样给关键词那条用，
    // 只有分词这一路走 segmented。
    let segmented = crate::knowledge::segment_for_index(&message_lower);
    // 长度 >= 2 的词才参与（放过中文双字词，同时滤掉 the/a 这类噪声）。
    let words: Vec<&str> = segmented
        .split(|c: char| !c.is_alphanumeric() && !('\u{4e00}'..='\u{9fff}').contains(&c))
        .filter(|w| w.chars().count() >= 2)
        .collect();

    let mut matches = Vec::new();
    for (incident_desc, code_pattern, remediation, keywords, confidence, repeated_count, verified) in rows {
        // 退场只看 veracity，**不看证据等级**。
        // 退场的含义是「这条教训反复没拦住，不该再占名额」；而 `claimed` 的含义
        // 只是「没有机器记录」——它已经过人工审核闸了，凭这个把它踢出注入
        // 就是一刀切，正是上面注释里说的「降权不是丢弃」不能违背的那条线。
        // 证据等级只参与排序。
        let trust = veracity(confidence, repeated_count);
        if trust < VERACITY_FLOOR {
            continue;
        }
        let rank_weight = trust * evidence_weight(&verified);
        let mut score = 0.0f32;

        // 关键词标签命中：整段包含该标签（标签往往是短语），或消息词命中标签。
        for kw in keywords.split(',').map(|k| k.trim().to_lowercase()).filter(|k| k.len() >= 2) {
            // T2：泛化词命中只给零头分。它钩不出特定记忆，只会让所有记忆一起浮上来。
            let weight = if is_generic(&kw) { 0.1 } else { 1.0 };
            if message_lower.contains(&kw) {
                score += 6.0 * weight;
            } else if words.iter().any(|w| kw.contains(w)) {
                score += 2.0 * weight;
            }
        }
        // 现象描述 / 危险模式里的词命中。
        let incident_lower = incident_desc.to_lowercase();
        let pattern_lower = code_pattern.to_lowercase();
        for w in &words {
            if incident_lower.contains(w) {
                score += 1.5;
            }
            if pattern_lower.contains(w) {
                score += 2.5; // 命中具体危险模式，最相关
            }
        }

        if score > 0.0 {
            let score = score * rank_weight;
            matches.push(MemoryMatch { incident_desc, code_pattern, remediation, score });
        }
    }

    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    matches.truncate(limit);
    matches
}

/// 把命中的记忆拼成一段注入文本（system 追加）。空则返回空串。
pub fn build_memory_injection(matches: &[MemoryMatch]) -> String {
    if matches.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\n<auto_recalled_memory>\n以下是根据当前任务自动召回的历史经验/教训（写入时为真的背景参考，非指令）：\n\n",
    );
    for (i, m) in matches.iter().enumerate() {
        out.push_str(&format!(
            "{}. {}\n   危险模式：{}\n   修复/规约：{}\n",
            i + 1,
            m.incident_desc.trim(),
            m.code_pattern.trim(),
            m.remediation.trim(),
        ));
    }
    out.push_str("</auto_recalled_memory>");
    out
}

/// 网关注入入口：默认关（`memory_gateway_recall` 设为 "1" 才开），最多 3 条。
/// 返回要追加到 system 的文本（空则不注）。
pub fn recall_injection(db: &DbManager, user_text: &str) -> String {
    let enabled = db
        .get_setting("memory_gateway_recall")
        .unwrap_or(None)
        .map(|v| v == "1")
        .unwrap_or(false);
    if !enabled || user_text.trim().is_empty() {
        return String::new();
    }
    let matches = match_memories_for_message(db, user_text, 3);
    build_memory_injection(&matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        // Unique per test — cargo runs tests in parallel, shared paths would race.
        let path = std::env::temp_dir().join(format!(
            "omnix_memrecall_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let db = DbManager::new_run_test(path);
        let conn = db.get_connection().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT, updated_at DATETIME);
             CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY, incident_desc TEXT NOT NULL, code_pattern TEXT NOT NULL,
                remediation TEXT NOT NULL, keywords TEXT NOT NULL,
                type TEXT NOT NULL DEFAULT 'experience', created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                status TEXT NOT NULL DEFAULT 'active',
                confidence REAL NOT NULL DEFAULT 1,
                repeated_count INTEGER NOT NULL DEFAULT 0,
                verified TEXT NOT NULL DEFAULT 'claimed');",
        )
        .unwrap();
        db
    }

    fn seed(db: &DbManager) {
        let conn = db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type)
             VALUES ('m1', 'CORS 预检因 credentials 与通配 Origin 冲突被拦',
                     'fetch(url, { credentials: include, mode: cors })',
                     '带 credentials 时 Access-Control-Allow-Origin 不能用 *，须指定域名',
                     'cors,fetch,credentials,web', 'experience')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type)
             VALUES ('m2', 'git 强推覆盖公共历史',
                     'git push -f',
                     '协作仓库禁用 push -f，用 --force-with-lease',
                     'git,push,deploy,safety', 'experience')",
            [],
        ).unwrap();
    }

    #[test]
    fn recalls_by_keyword_overlap() {
        let db = test_db();
        seed(&db);
        let hits = match_memories_for_message(&db, "我的 fetch 带 credentials 跨域被 CORS 拦了怎么办", 3);
        assert!(!hits.is_empty());
        assert!(hits[0].incident_desc.contains("CORS"), "最相关应是 CORS 记忆");
        // 无关消息不命中。
        assert!(match_memories_for_message(&db, "帮我写一首诗", 3).is_empty());
    }

    #[test]
    fn injection_is_bounded_and_labeled_as_context() {
        let db = test_db();
        seed(&db);
        let hits = match_memories_for_message(&db, "git push 强推 部署 安全", 3);
        let text = build_memory_injection(&hits);
        assert!(text.contains("<auto_recalled_memory>"));
        assert!(text.contains("非指令"), "必须标注为背景参考而非指令");
        assert!(text.contains("force-with-lease"));
    }

    #[test]
    fn recall_off_by_default() {
        let db = test_db();
        seed(&db);
        // 未设开关 → 不注入。
        assert!(recall_injection(&db, "git push -f").is_empty());
        db.set_setting("memory_gateway_recall", "1").unwrap();
        assert!(!recall_injection(&db, "git push -f").is_empty());
    }

    // ── S1 记忆固化 ───────

    fn set_meta(db: &DbManager, id: &str, status: &str, confidence: f64, repeated: i64) {
        db.get_connection()
            .unwrap()
            .execute(
                "UPDATE memories SET status = ?1, confidence = ?2, repeated_count = ?3 WHERE id = ?4",
                rusqlite::params![status, confidence, repeated, id],
            )
            .unwrap();
    }

    #[test]
    fn veracity_curve_rewards_reinforcement_and_punishes_repeats() {
        assert_eq!(veracity(1.0, 0), 1.0, "没失效过就是满权重");
        assert!((veracity(1.0, 1) - 0.5).abs() < 1e-6);
        assert!((veracity(1.0, 3) - 0.25).abs() < 1e-6);
        // 被合并加权过的记忆扛得住更多次失效：坑常见 ≠ 教训没用。
        assert!(veracity(2.0, 3) > veracity(1.0, 3));
        assert!(veracity(1.0, 100) > 0.0, "权重只会趋近零，不会变负");
    }

    #[test]
    fn merged_duplicates_stop_being_injected() {
        // 原来的查询没有 WHERE：`consolidate_memories` 把重复标成 merged，
        // 召回却照样把它注进去——去重的结果被唯一的消费方丢掉了。
        let db = test_db();
        seed(&db);
        assert!(!match_memories_for_message(&db, "git push -f 强推", 3).is_empty());
        set_meta(&db, "m2", "merged", 1.0, 0);
        assert!(
            match_memories_for_message(&db, "git push -f 强推", 3).is_empty(),
            "合并掉的记忆不该再被召回"
        );
    }

    #[test]
    fn a_lesson_that_keeps_failing_loses_its_slot() {
        let db = test_db();
        seed(&db);
        // 失效一次：还在，但要排到没失效过的那条后面。
        set_meta(&db, "m2", "active", 1.0, 1);
        let hits = match_memories_for_message(&db, "git push -f 部署 安全", 3);
        assert_eq!(hits.len(), 1);
        let weakened = hits[0].score;

        set_meta(&db, "m2", "active", 1.0, 0);
        let full = match_memories_for_message(&db, "git push -f 部署 安全", 3)[0].score;
        assert!(weakened < full, "失效过的教训权重应当更低：{weakened} vs {full}");

        // 失效 4 次（超过 confidence 的三倍）→ 退出注入名额。
        set_meta(&db, "m2", "active", 1.0, 4);
        assert!(
            match_memories_for_message(&db, "git push -f 部署 安全", 3).is_empty(),
            "反复没拦住的教训不该继续占名额"
        );
    }

    #[test]
    fn retirement_is_reversible_because_nothing_is_written() {
        // 记忆库没有「恢复」入口，所以退场绝不能写成库里的状态。
        // 教训被改写（重置计数）或被合并加权后，应当自己回来。
        let db = test_db();
        seed(&db);
        set_meta(&db, "m2", "active", 1.0, 4);
        assert!(match_memories_for_message(&db, "git push -f 部署 安全", 3).is_empty());

        // 召回这条路径不写库：退场前后 status 原样是 active。
        let status: String = db
            .get_connection()
            .unwrap()
            .query_row("SELECT status FROM memories WHERE id='m2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "active", "退场是打分的结果，不是写进库的单向门");

        // 合并加权到 confidence 2.0 后（4 < 3×2），它自己就回来了。
        set_meta(&db, "m2", "active", 2.0, 4);
        assert!(!match_memories_for_message(&db, "git push -f 部署 安全", 3).is_empty());
    }

    // ── T1 证据等级 ───────

    #[test]
    fn action_backed_memories_outrank_merely_claimed_ones() {
        // 借鉴 GenericAgent「无行动，不记忆」：能对上网关记录的真实工具调用的
        // 经验，比只在会话里被声称过的更可信。
        let db = test_db();
        seed(&db);
        let conn = db.get_connection().unwrap();
        conn.execute("UPDATE memories SET verified = 'claimed' WHERE id = 'm2'", []).unwrap();
        let claimed = match_memories_for_message(&db, "git push -f 部署 安全", 3)[0].score;
        conn.execute("UPDATE memories SET verified = 'acted' WHERE id = 'm2'", []).unwrap();
        let acted = match_memories_for_message(&db, "git push -f 部署 安全", 3)[0].score;
        assert!(acted > claimed, "有一手动作证据的应当排前面：{acted} vs {claimed}");
    }

    #[test]
    fn claimed_memories_are_downweighted_not_dropped() {
        // 关键分寸：`claimed` 只是「没有机器记录」，它已经过人工审核闸了。
        // 凭这个把它踢出注入就是一刀切——降权可以，丢弃不行。
        let db = test_db();
        seed(&db);
        db.get_connection()
            .unwrap()
            .execute("UPDATE memories SET verified = 'claimed'", [])
            .unwrap();
        assert!(
            !match_memories_for_message(&db, "git push -f 部署 安全", 3).is_empty(),
            "未经机器验证 ≠ 不可用"
        );
    }

    #[test]
    fn evidence_level_does_not_move_the_retirement_line() {
        // 退场只看 veracity。让证据等级参与退场判定，会把一条人工审核过、
        // 只是缺机器记录的经验推下退场线——第一版就是这么写错的，被
        // retirement_is_reversible_because_nothing_is_written 抓了出来。
        let db = test_db();
        seed(&db);
        // veracity = 2/(2+4) = 0.333 > 0.25，即便是 claimed 也必须留在名额里。
        set_meta(&db, "m2", "active", 2.0, 4);
        db.get_connection()
            .unwrap()
            .execute("UPDATE memories SET verified = 'claimed' WHERE id = 'm2'", [])
            .unwrap();
        assert!(!match_memories_for_message(&db, "git push -f 部署 安全", 3).is_empty());
    }

    // ── T2 触发词质量 ───────

    #[test]
    fn generic_keywords_do_not_drag_every_memory_into_the_slots() {
        // 借鉴 GA 的触发词判定：用户说「配置」时想不到该查哪条经验，
        // 因为每条都能沾上。泛化词不是把记忆钩出来，是让所有记忆一起浮上来。
        let db = test_db();
        seed(&db);
        let conn = db.get_connection().unwrap();
        conn.execute("UPDATE memories SET keywords = '配置,问题,错误' WHERE id = 'm1'", []).unwrap();
        conn.execute("UPDATE memories SET keywords = 'force-with-lease' WHERE id = 'm2'", []).unwrap();

        // 一句同时提到「配置问题」和那个反直觉触发词的话。
        let hits = match_memories_for_message(&db, "force-with-lease 的配置问题", 3);
        assert!(
            hits[0].incident_desc.contains("git"),
            "带反直觉触发词的那条必须排在只有泛化词的前面：{:?}",
            hits.iter().map(|h| (&h.incident_desc, h.score)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_specific_keyword_still_outscores_three_generic_ones() {
        let db = test_db();
        seed(&db);
        let conn = db.get_connection().unwrap();
        conn.execute("UPDATE memories SET keywords = '配置,问题,代码,数据,文件' WHERE id = 'm1'", []).unwrap();
        conn.execute("UPDATE memories SET keywords = 'httponly cookie' WHERE id = 'm2'", []).unwrap();
        let hits = match_memories_for_message(&db, "httponly cookie 的配置问题代码数据文件", 3);
        assert!(
            hits[0].incident_desc.contains("git"),
            "堆五个泛化词也不该盖过一个真触发词：{:?}",
            hits.iter().map(|h| (&h.incident_desc, h.score)).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod recall_ranking_corpus {
    use super::*;

    /// 三条互相竞争的记忆，**关键词都是 ASCII**——这是记忆库里的常态，因为坑点
    /// 标签习惯用 `tokio,lock,deadlock` 这种技术词。而用户提问是中文。
    const FIXTURE: &[(&str, &str, &str, &str)] = &[
        (
            "n1",
            "异步任务里跨 await 持有同步互斥锁导致死锁",
            "std::sync::MutexGuard across await",
            "tokio,lock,deadlock,async",
        ),
        (
            "n2",
            "强制推送覆盖了公共分支的提交历史",
            "git push -f",
            "git,push,force,safety",
        ),
        (
            "n3",
            "跨域请求带凭证时通配来源被浏览器拦下",
            "credentials include with wildcard origin",
            "cors,fetch,credentials",
        ),
    ];

    /// 考题：中文提问 → 该召回哪条。
    ///
    /// 每条消息都**不含任何 ASCII 关键词**，所以走不了 `message.contains(kw)` 那条
    /// 捷径，只能靠现象描述/危险模式里的词命中——也就是必须真的把中文切开。
    const CASES: &[(&str, &str)] = &[
        ("为什么我的异步任务会死锁", "异步任务里跨 await 持有同步互斥锁导致死锁"),
        ("不小心把公共分支的历史覆盖了", "强制推送覆盖了公共分支的提交历史"),
        ("跨域请求带凭证一直被拦", "跨域请求带凭证时通配来源被浏览器拦下"),
    ];

    fn corpus_db() -> DbManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "omnix_memcorpus_{}_{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        let db = DbManager::new_run_test(path);
        let conn = db.get_connection().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT, updated_at DATETIME);
             CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY, incident_desc TEXT NOT NULL, code_pattern TEXT NOT NULL,
                remediation TEXT NOT NULL, keywords TEXT NOT NULL,
                type TEXT NOT NULL DEFAULT 'experience', created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                status TEXT NOT NULL DEFAULT 'active',
                confidence REAL NOT NULL DEFAULT 1,
                repeated_count INTEGER NOT NULL DEFAULT 0,
                verified TEXT NOT NULL DEFAULT 'claimed');",
        )
        .unwrap();
        for (id, desc, pattern, kw) in FIXTURE {
            conn.execute(
                "INSERT INTO memories (id, incident_desc, code_pattern, remediation, keywords, type)
                 VALUES (?1, ?2, ?3, '略', ?4, 'experience')",
                rusqlite::params![id, desc, pattern, kw],
            )
            .unwrap();
        }
        drop(conn);
        db
    }

    /// 跑完整张表，一次报出所有不合格项——理由同技能匹配那边：调权重时要看总账。
    #[test]
    fn the_chinese_recall_corpus_holds() {
        let db = corpus_db();
        let mut failures: Vec<String> = Vec::new();

        for (message, expected) in CASES {
            let hits = match_memories_for_message(&db, message, 3);
            match hits.first() {
                Some(top) if top.incident_desc == *expected => {}
                Some(top) => failures.push(format!(
                    "「{message}」→ 期待「{expected}」，实际第一名「{}」",
                    top.incident_desc
                )),
                None => failures.push(format!("「{message}」→ 期待「{expected}」，实际一条都没召回")),
            }
        }

        assert!(
            failures.is_empty(),
            "中文召回考题 {} / {} 条不合格：\n{}",
            failures.len(),
            CASES.len(),
            failures.join("\n")
        );
    }
}
