//! P2 技能锁：正式池技能的**内容指纹 + 来源存证**。
//!
//! ## 为什么现在需要它
//!
//! 晋升正式池要过两道门（AI 审核 + 注入扫描，见 `commands::skill_pool`）。但那
//! 两道门只在**晋升那一刻**看一眼——之后技能文件在磁盘上被改成什么样，没有任何
//! 东西会发现。
//!
//! P1 把这件事的代价放大了：`/mcp` 现在把正式池技能交给 Claude Code、Codex 这些
//! **别的 agent** 当作可信说明书执行。一个通过审核后被改掉的技能文件，会直接以
//! 「OMNIX 已审核」的身份流出去。审核的是当时那份内容，不是这个文件名。
//!
//! 所以：晋升时把内容指纹钉下来，之后每次交出去之前核一遍。
//!
//! ## 为什么用 SHA-256 而不是现成的 fnv1a
//!
//! 仓库里已有的 `hash::fnv1a_hash` 用在技能三方合并上，做**变更检测**够用。但锁
//! 要防的是「内容被换掉却看起来没变」，fnv1a 是非密码学哈希，构造碰撞是容易的。
//! 用途不同，别复用。

use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::DbManager;

/// 技能锁的状态。区分「没锁」和「对不上」很重要——前者是历史数据，
/// 后者是内容真的变了，处理方式完全不同。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LockStatus {
    /// 指纹一致
    Ok,
    /// 内容变了。审核过的不是现在这份。
    Drifted { approved: String, current: String },
    /// 还没上锁（本次功能之前晋升的老技能）
    Unlocked,
    /// 文件读不到
    Missing { reason: String },
}

impl LockStatus {
    /// 能不能把这份技能交出去。只有指纹一致才算数——
    /// 「没锁」也不放行：交出去的东西必须有据可查。
    pub fn is_trusted(&self) -> bool {
        matches!(self, LockStatus::Ok)
    }
}

pub fn sha256_hex(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

/// 技能正文所在的文件。`central_path` 是集中存储，可能是目录（内含 SKILL.md）。
pub fn skill_file(db: &DbManager, name: &str) -> Result<std::path::PathBuf, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let path: String = conn
        .query_row(
            "SELECT COALESCE(NULLIF(central_path,''), file_path) FROM skills WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .map_err(|_| format!("技能不存在: {name}"))?;
    let p = std::path::PathBuf::from(path);
    Ok(if p.is_dir() { p.join("SKILL.md") } else { p })
}

/// 上锁：把当前内容的指纹记下来。在晋升正式池时调用。
///
/// 同时记下**当时的来源**（source_ref / revision 已有列），这样「这份内容是从
/// 哪来的、什么时候被谁认可的」是一条可查的记录，而不是口头记忆。
pub fn lock(db: &DbManager, name: &str) -> Result<String, String> {
    let file = skill_file(db, name)?;
    let content = std::fs::read_to_string(&file)
        .map_err(|e| format!("读不了技能文件 {}：{e}", file.display()))?;
    let hash = sha256_hex(&content);
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE skills SET approved_hash = ?1, approved_at = CURRENT_TIMESTAMP WHERE name = ?2",
        params![hash, name],
    )
    .map_err(|e| e.to_string())?;
    Ok(hash)
}

/// 核对：现在磁盘上这份，还是不是当初被认可的那份。
pub fn verify(db: &DbManager, name: &str) -> LockStatus {
    let approved: Option<String> = match db.get_connection() {
        Ok(conn) => conn
            .query_row(
                "SELECT approved_hash FROM skills WHERE name = ?1",
                params![name],
                |r| r.get(0),
            )
            .ok()
            .flatten(),
        Err(e) => return LockStatus::Missing { reason: e.to_string() },
    };
    let Some(approved) = approved.filter(|h| !h.is_empty()) else {
        return LockStatus::Unlocked;
    };
    let file = match skill_file(db, name) {
        Ok(f) => f,
        Err(e) => return LockStatus::Missing { reason: e },
    };
    let content = match std::fs::read_to_string(&file) {
        Ok(c) => c,
        Err(e) => {
            return LockStatus::Missing {
                reason: format!("{}：{e}", file.display()),
            }
        }
    };
    let current = sha256_hex(&content);
    if current == approved {
        LockStatus::Ok
    } else {
        LockStatus::Drifted { approved, current }
    }
}

/// 一条技能的存证。前端的「技能锁」面板照着这个列。
#[derive(Debug, Clone, Serialize)]
pub struct SkillProvenance {
    pub name: String,
    pub status: LockStatus,
    /// 来源类型：local / git / builtin
    pub source_type: String,
    /// Git URL、导入路径，或 `omnix:fusion(a+b)`
    pub source_ref: String,
    pub source_revision: String,
    pub approved_at: String,
}

/// 全部正式池技能的存证清单，drift 的排在最前面——
/// 报告是给人从上往下读的，需要处理的必须先出现。
pub fn audit(db: &DbManager) -> Result<Vec<SkillProvenance>, String> {
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT name, COALESCE(source_type,''), COALESCE(source_ref,''),
                    COALESCE(source_revision,''), COALESCE(approved_at,'')
             FROM skills WHERE pool = 'official' ORDER BY name",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    drop(stmt);
    drop(conn);

    let mut out: Vec<SkillProvenance> = rows
        .into_iter()
        .map(|(name, source_type, source_ref, source_revision, approved_at)| {
            let status = verify(db, &name);
            SkillProvenance { name, status, source_type, source_ref, source_revision, approved_at }
        })
        .collect();
    out.sort_by_key(|p| match p.status {
        LockStatus::Drifted { .. } => 0,
        LockStatus::Missing { .. } => 1,
        LockStatus::Unlocked => 2,
        LockStatus::Ok => 3,
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Fixture {
        db: Arc<DbManager>,
        dir: std::path::PathBuf,
    }

    fn setup(tag: &str) -> Fixture {
        let base = std::env::temp_dir().join(format!("omnix_lock_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let db = Arc::new(DbManager::new_with_path(base.join("t.db")));
        Fixture { db, dir: base }
    }

    fn add_skill(f: &Fixture, name: &str, content: &str) -> std::path::PathBuf {
        let file = f.dir.join(format!("{name}.md"));
        std::fs::write(&file, content).unwrap();
        let conn = f.db.get_connection().unwrap();
        conn.execute(
            "INSERT INTO skills (name, description, file_path, central_path, pool)
             VALUES (?1, '测试技能', ?2, ?2, 'official')",
            params![name, file.to_string_lossy()],
        )
        .unwrap();
        file
    }

    #[test]
    fn locking_then_verifying_is_ok() {
        let f = setup("ok");
        add_skill(&f, "alpha", "原始内容");
        lock(&f.db, "alpha").unwrap();
        assert_eq!(verify(&f.db, "alpha"), LockStatus::Ok);
        assert!(verify(&f.db, "alpha").is_trusted());
    }

    /// 核心承诺：审核通过之后内容被改掉，必须查得出来。
    /// P1 会把正式池技能交给别的 agent 执行，这是那条链路的最后一道检查。
    #[test]
    fn content_changed_after_approval_is_detected() {
        let f = setup("drift");
        let file = add_skill(&f, "beta", "审核时的内容");
        let approved = lock(&f.db, "beta").unwrap();

        std::fs::write(&file, "被偷偷改掉的内容").unwrap();
        match verify(&f.db, "beta") {
            LockStatus::Drifted { approved: a, current } => {
                assert_eq!(a, approved);
                assert_ne!(current, approved);
            }
            other => panic!("应报 Drifted，实际 {other:?}"),
        }
        assert!(!verify(&f.db, "beta").is_trusted(), "变了就不能再交出去");
    }

    /// 只改一个字符也要抓到——这正是非密码学哈希不够用的地方。
    #[test]
    fn a_single_character_change_is_caught() {
        let f = setup("onechar");
        let file = add_skill(&f, "gamma", "执行前请先备份");
        lock(&f.db, "gamma").unwrap();
        std::fs::write(&file, "执行前无需备份").unwrap();
        assert!(matches!(verify(&f.db, "gamma"), LockStatus::Drifted { .. }));
    }

    /// 老数据没有锁。要跟「对不上」区分开：前者是历史遗留，后者是内容真变了。
    #[test]
    fn never_locked_is_distinct_from_drifted() {
        let f = setup("unlocked");
        add_skill(&f, "delta", "内容");
        assert_eq!(verify(&f.db, "delta"), LockStatus::Unlocked);
        assert!(!verify(&f.db, "delta").is_trusted(), "没有存证也不该放行");
    }

    #[test]
    fn missing_file_is_reported_not_silently_trusted() {
        let f = setup("missing");
        let file = add_skill(&f, "eps", "内容");
        lock(&f.db, "eps").unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(matches!(verify(&f.db, "eps"), LockStatus::Missing { .. }));
        assert!(!verify(&f.db, "eps").is_trusted());
    }

    /// 审计清单要把需要处理的排在最前面。
    #[test]
    fn audit_puts_problems_first() {
        let f = setup("audit");
        add_skill(&f, "a_ok", "好的");
        lock(&f.db, "a_ok").unwrap();
        add_skill(&f, "b_unlocked", "没锁");
        let drifted = add_skill(&f, "c_drift", "原始");
        lock(&f.db, "c_drift").unwrap();
        std::fs::write(&drifted, "改了").unwrap();

        let list = audit(&f.db).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "c_drift", "drift 必须排第一: {:?}",
                   list.iter().map(|p| &p.name).collect::<Vec<_>>());
        assert_eq!(list[2].name, "a_ok", "正常的排最后");
    }

    #[test]
    fn sha256_is_not_the_fnv_hash_used_for_change_detection() {
        // 同样输入两种哈希必须不同，防止有人以为可以互换
        let s = "内容";
        assert_ne!(sha256_hex(s), crate::hash::fnv1a_hash(s));
        assert_eq!(sha256_hex(s).len(), 64, "SHA-256 十六进制是 64 字符");
    }
}
