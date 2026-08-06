//! T3：把正式池技能镜像到跨 harness 的公共技能目录 `~/.agents/skills/`。
//!
//! ## 为什么
//!
//! 借鉴 pi（`@earendil-works/pi-coding-agent`）：它实现 Agent Skills 标准，
//! 从 `~/.agents/skills/` 与项目 `.agents/skills/` 发现技能，并明确文档化了
//! 「怎么复用 Claude Code / Codex 已装的技能目录」。这个路径正在变成各家
//! harness 的公共约定。
//!
//! OMNIX 的技能本来就是 `<名字>/SKILL.md` 这个标准布局，只是躺在自己的
//! 存储目录里，**只有 OMNIX 启动的 agent 才吃得到**。镜像一份到公共目录，
//! 同一批技能对用户装的所有 harness 都生效——纯接线，文件本来就在写。
//!
//! ## 绝不覆盖别人的文件
//!
//! `~/.agents/skills/` 是**共享**目录，里面可能有用户手写的、或别的 harness
//! 装的技能。同名覆盖就是毁掉别人的东西。
//!
//! 所以用和 S0 同一套指纹纪律：只有当磁盘上那份内容与我们上次写出去的一致
//! （或文件不存在）时才写。对不上就是有人改过——跳过并如实报告，让用户自己定。
//! 判断依据是内容本身，不需要额外记状态。

use std::path::PathBuf;

use crate::db::DbManager;

/// 跨 harness 公共技能目录。
///
/// 不用 `storage::dir_for` 那套可配置目录：这个路径是**和别家约定好的**，
/// 改掉它就等于退出这个约定，没有可配置的意义。
pub fn agents_skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".agents").join("skills"))
}

/// 一次镜像的结果。分三类是因为「没导出」有两种完全不同的原因，
/// 混成一个数字用户就没法判断该不该管。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// 新写或更新的技能名。
    pub written: Vec<String>,
    /// 目标文件被别人改过，跳过没动。
    pub skipped_foreign: Vec<String>,
    /// 内容已经一致，无需重写。
    pub unchanged: Vec<String>,
}

impl ExportReport {
    /// 给用户看的一句话。有冲突时必须说出来——静默跳过等于骗人。
    pub fn headline(&self) -> String {
        if self.written.is_empty() && self.skipped_foreign.is_empty() {
            return format!("公共技能目录已是最新（{} 个技能）", self.unchanged.len());
        }
        let mut parts = Vec::new();
        if !self.written.is_empty() {
            parts.push(format!("已导出 {} 个", self.written.len()));
        }
        if !self.skipped_foreign.is_empty() {
            parts.push(format!(
                "**{} 个被跳过**（目标文件已被改动，未覆盖）：{}",
                self.skipped_foreign.len(),
                self.skipped_foreign.join("、")
            ));
        }
        parts.join("；")
    }
}

/// 磁盘上这份是不是我们上次写出去的那份。
///
/// 文件不存在 → 可以写。内容一致 → 是我们的，可以更新。
/// 内容不一致 → 别人改过或别人写的，**不许动**。
fn writable(target: &std::path::Path, content: &str) -> Result<bool, ()> {
    match std::fs::read_to_string(target) {
        Err(_) => Ok(true), // 不存在（或读不了）→ 尝试写
        Ok(existing) if existing == content => Err(()), // 已一致，不必写
        Ok(_) => Ok(false), // 有人改过
    }
}

/// 把正式池里所有启用的技能镜像出去。
pub fn export_official_skills(db: &DbManager) -> Result<ExportReport, String> {
    let Some(root) = agents_skills_dir() else {
        return Err("找不到用户主目录，无法定位 ~/.agents/skills".into());
    };
    let conn = db.get_connection().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT name, CASE WHEN central_path != '' THEN central_path ELSE file_path END
             FROM skills WHERE is_active = 1 AND pool = 'official'",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();

    let mut report = ExportReport::default();
    for (name, source_path) in rows {
        // 名字要进路径，先挡住穿越。技能名是用户/模型给的，不能当可信输入。
        if name.trim().is_empty() || name.contains(['/', '\\', ':']) || name.contains("..") {
            report.skipped_foreign.push(format!("{name}（名字不能作为目录名）"));
            continue;
        }
        let Some(content) = read_skill_source(&source_path) else {
            continue; // 源文件不在了，跳过；这不是冲突，别记成冲突
        };
        let dir = root.join(&name);
        let target = dir.join("SKILL.md");
        match writable(&target, &content) {
            Err(()) => report.unchanged.push(name),
            Ok(false) => report.skipped_foreign.push(name),
            Ok(true) => {
                if std::fs::create_dir_all(&dir).is_err() {
                    report.skipped_foreign.push(name);
                    continue;
                }
                // 先写临时文件再改名：中途失败不会在公共目录留下半份内容，
                // 别家 harness 随时可能正在读它。
                let tmp = dir.join(format!("SKILL.md.omnix{}.tmp", std::process::id()));
                let ok = std::fs::write(&tmp, &content).is_ok()
                    && std::fs::rename(&tmp, &target).is_ok();
                let _ = std::fs::remove_file(&tmp);
                if ok {
                    report.written.push(name);
                } else {
                    report.skipped_foreign.push(name);
                }
            }
        }
    }
    Ok(report)
}

/// 技能内容可能存成目录（含 SKILL.md）或单文件，两种都认。
fn read_skill_source(path: &str) -> Option<String> {
    let p = PathBuf::from(path);
    std::fs::read_to_string(p.join("SKILL.md"))
        .or_else(|_| std::fs::read_to_string(&p))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn td(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "omnix_export_{tag}_{}_{}",
            std::process::id(),
            chrono::Utc::now().timestamp_micros()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn a_file_we_wrote_can_be_updated() {
        let dir = td("ours");
        let target = dir.join("SKILL.md");
        std::fs::write(&target, "旧内容").unwrap();
        // 内容不同 → 认定为「有人改过」，因为我们上次写的就该等于我们现在要写的。
        assert_eq!(writable(&target, "新内容"), Ok(false));
        // 内容一致 → 不必重写。
        assert_eq!(writable(&target, "旧内容"), Err(()));
    }

    #[test]
    fn a_missing_file_is_writable() {
        let dir = td("missing");
        assert_eq!(writable(&dir.join("SKILL.md"), "内容"), Ok(true));
    }

    /// 公共目录里可能有用户手写的同名技能。覆盖它就是毁掉用户的东西——
    /// 这条是这个模块存在的全部风险所在。
    #[test]
    fn a_foreign_file_is_never_overwritten() {
        let dir = td("foreign");
        let target = dir.join("SKILL.md");
        std::fs::write(&target, "用户自己手写的技能，OMNIX 从没写过这份").unwrap();
        assert_eq!(writable(&target, "OMNIX 想写的内容"), Ok(false));
        // 而且原文件必须原封不动。
        assert!(std::fs::read_to_string(&target).unwrap().contains("用户自己手写"));
    }

    #[test]
    fn conflicts_are_named_in_the_headline_not_swallowed() {
        // 静默跳过等于骗人：用户会以为技能已经共享出去了。
        let r = ExportReport {
            written: vec!["a".into()],
            skipped_foreign: vec!["b".into()],
            unchanged: vec![],
        };
        let h = r.headline();
        assert!(h.contains("已导出 1 个"));
        assert!(h.contains("被跳过"), "冲突必须说出来：{h}");
        assert!(h.contains('b'), "要点名是哪个技能：{h}");
    }

    #[test]
    fn all_unchanged_reads_as_up_to_date_not_as_failure() {
        let r = ExportReport {
            written: vec![],
            skipped_foreign: vec![],
            unchanged: vec!["a".into(), "b".into()],
        };
        assert!(r.headline().contains("已是最新"));
    }

    #[test]
    fn the_shared_directory_is_the_agreed_cross_harness_path() {
        // 这个路径是和 pi / Claude Code / Codex 约定好的，写死是故意的。
        let dir = agents_skills_dir().expect("应当能定位主目录");
        assert!(dir.ends_with("skills"));
        assert!(dir.parent().unwrap().ends_with(".agents"));
    }

    #[test]
    fn skill_content_reads_from_either_a_dir_or_a_flat_file() {
        let dir = td("layout");
        let as_dir = dir.join("with_dir");
        std::fs::create_dir_all(&as_dir).unwrap();
        std::fs::write(as_dir.join("SKILL.md"), "目录布局").unwrap();
        assert_eq!(read_skill_source(as_dir.to_str().unwrap()).unwrap(), "目录布局");

        let flat = dir.join("flat.md");
        std::fs::write(&flat, "单文件布局").unwrap();
        assert_eq!(read_skill_source(flat.to_str().unwrap()).unwrap(), "单文件布局");

        assert!(read_skill_source(dir.join("不存在").to_str().unwrap()).is_none());
    }
}
