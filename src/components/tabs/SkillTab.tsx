/**
 * SkillTab — 技能中心.
 *
 * 一页流水线：收集 → 待定池（摘要+审核）→ 改造/融合 → 晋升 → 正式池网关直调。
 * 市场（外部导入）与工具同步（物理分发）作为辅助面板按需打开。
 */
import { useState } from "react";
import { ArrowRightLeft, ClipboardCheck, ShieldCheck, Sparkles, Store } from "lucide-react";

import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { skillAuditApi, skillPoolApi, skillSafetyApi, type SkillAuditResult, type SkillRisk } from "@/lib/tauri-api";
import { SkillPoolPanel } from "@/components/SkillPoolPanel";
import { SkillMarketPanel } from "@/components/SkillMarketPanel";
import { SkillSyncPanel } from "@/components/SkillSyncPanel";

export function SkillTab() {
  const [showMarket, setShowMarket] = useState(false);
  const [showSync, setShowSync] = useState(false);
  // 让市场导入后技能中心刷新：换 key 重挂载最省事且无状态耦合。
  const [poolKey, setPoolKey] = useState(0);
  // null = 还没扫过（不显示面板）；[] = 扫过且干净。
  const [risks, setRisks] = useState<SkillRisk[] | null>(null);
  const [scanning, setScanning] = useState(false);
  // 审阅结果旁边必须能直接动手——只给判决不给动作，等于把活儿又推回给你。
  const removeSkill = async (name: string) => {
    if (!window.confirm(`删除技能「${name}」？不可撤销。`)) return;
    try {
      await skillPoolApi.remove(name);
      setRisks((prev) => prev?.filter((r) => r.name !== name) ?? null);
      setPoolKey((k) => k + 1);
      toast.success(`已删除「${name}」`);
    } catch (error) {
      toast.error("删除失败", { description: String(error) });
    }
  };
  const demoteSkill = async (name: string) => {
    try {
      await skillPoolApi.setPool(name, "pending");
      setRisks((prev) => prev?.map((r) => (r.name === name ? { ...r, pool: "pending" } : r)) ?? null);
      setPoolKey((k) => k + 1);
      toast.success(`「${name}」已退回待定池`, { description: "不再注入到 agent 请求。" });
    } catch (error) {
      toast.error("退回失败", { description: String(error) });
    }
  };

  // 质量审计：`run_skill_audit` 一直是完整可跑的（它按 `{name}_core.md` 打分，
  // 那个文件约定在 skills / skill_library / skill_pool / skill_sync 里都还活着），
  // 但从来没有入口。放在风险审阅旁边——两者形状一样：跑一遍 → 出一张清单。
  // 区别是风险审阅问「这技能会不会干坏事」，质量审计问「这技能写得够不够用」。
  const [audits, setAudits] = useState<SkillAuditResult[] | null>(null);
  const [auditing, setAuditing] = useState(false);
  const runQualityAudit = async () => {
    setAuditing(true);
    try {
      const found = await skillAuditApi.run();
      // 分低的排前面——要动手的就是它们。
      setAudits([...found].sort((a, b) => a.score - b.score));
      toast.success(
        found.length === 0
          ? "质量审计完成：没有可评分的技能"
          : `质量审计完成：${found.filter((a) => a.issues.length > 0).length}/${found.length} 个技能有待改进`,
      );
    } catch (error) {
      toast.error("质量审计失败", { description: String(error) });
    } finally {
      setAuditing(false);
    }
  };

  const runSafetyScan = async () => {
    setScanning(true);
    try {
      const found = await skillSafetyApi.scanAll();
      setRisks(found);
      toast.success(found.length === 0 ? "审阅完成：没有发现可疑指令" : `审阅完成：${found.length} 个技能有可疑指令`);
    } catch (error) {
      toast.error("安全扫描失败", { description: String(error) });
    } finally {
      setScanning(false);
    }
  };

  return (
    <div className="flex h-full flex-1 min-w-0 flex-col overflow-hidden bg-background">
      <div className="flex items-center gap-2 border-b border-border px-6 py-4">
        <Sparkles className="h-5 w-5 text-primary" />
        <div>
          <div className="text-lg font-semibold">技能中心</div>
          <p className="text-xs text-muted-foreground">
            收集 → 待定池（看得懂）→ 审核（给出问题与改法）→ AI 改造/融合 → 你拍板晋升 → 正式池全 agent 直调
          </p>
        </div>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => setShowMarket(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="从 GitHub 等来源搜索并导入技能（进待定池）"
          >
            <Store className="h-3.5 w-3.5" /> 市场
          </button>
          <button
            onClick={() => void runSafetyScan()}
            disabled={scanning}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground disabled:opacity-50"
            title="把所有技能过一遍风险审阅：查可疑指令，不是病毒扫描"
          >
            <ShieldCheck className="h-3.5 w-3.5" /> {scanning ? "审阅中…" : "风险审阅"}
          </button>
          <button
            onClick={() => void runQualityAudit()}
            disabled={auditing}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground disabled:opacity-50"
            title="给每个技能的正文打分：太短、没标题、缺示例这类写法问题"
          >
            <ClipboardCheck className="h-3.5 w-3.5" /> {auditing ? "审计中…" : "质量审计"}
          </button>
          <button
            onClick={() => setShowSync(true)}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs text-muted-foreground hover:bg-muted/40 hover:text-foreground"
            title="把技能物理分发到不走网关的工具目录"
          >
            <ArrowRightLeft className="h-3.5 w-3.5" /> 同步
          </button>
        </div>
      </div>
      {/* 质量审计结果。分低的在前——要动手的就是它们。 */}
      {audits !== null && (
        <div className="max-h-[45%] overflow-y-auto border-b border-border bg-muted/10 px-6 py-3">
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
            <ClipboardCheck className="h-4 w-4 text-primary" />
            质量审计：{audits.length === 0 ? "没有可评分的技能" : `共 ${audits.length} 个`}
            <span className="font-normal text-xs text-muted-foreground">
              看的是写法（长度 / 标题 / 示例），不是安全——那是「风险审阅」
            </span>
            <button onClick={() => setAudits(null)} className="ml-auto text-xs font-normal text-muted-foreground hover:text-foreground">
              收起
            </button>
          </div>
          <div className="space-y-2">
            {audits.map((audit) => (
              <div key={audit.skill_name} className="rounded-md border border-border bg-background/60 p-2.5">
                <div className="flex items-center gap-2 text-xs">
                  <span className={cn(
                    "shrink-0 rounded px-1.5 py-0.5 font-medium tabular-nums",
                    audit.score >= 8 ? "bg-success/15 text-success"
                      : audit.score >= 5 ? "bg-warning/15 text-warning"
                      : "bg-destructive/15 text-destructive",
                  )}>
                    {audit.score} / 10
                  </span>
                  <span className="truncate font-medium">{audit.skill_name}</span>
                  {audit.issues.length === 0 && (
                    <span className="shrink-0 text-muted-foreground">没问题</span>
                  )}
                </div>
                {audit.issues.length > 0 && (
                  <ul className="mt-1.5 space-y-0.5 pl-1 text-xs text-muted-foreground">
                    {audit.issues.map((issue) => (
                      <li key={issue}>· {issue}</li>
                    ))}
                  </ul>
                )}
                {audit.suggestion.trim() && (
                  <p className="mt-1.5 text-xs text-muted-foreground">建议：{audit.suggestion}</p>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
      {/* 风险审阅结果。
          查的是**可疑指令**不是可执行病毒——技能是注入 prompt 的 Markdown 指令，
          真正的风险是「让 agent 做你没让它做的事」。这是初筛，不替代你自己看一眼，
          所以每条都带行号和原文，误报你能一眼认出来。 */}
      {risks !== null && (
        <div className="max-h-[45%] overflow-y-auto border-b border-border bg-muted/10 px-6 py-3">
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
            <ShieldCheck className="h-4 w-4 text-primary" />
            风险审阅：{risks.length === 0 ? "没有发现可疑指令" : `${risks.length} 个技能有可疑指令`}
            <span className="font-normal text-xs text-muted-foreground">
              查的是可疑指令（外传数据 / 读凭证 / 留后门 / 瞒着你做事），不是病毒扫描
            </span>
            <button onClick={() => setRisks(null)} className="ml-auto text-xs font-normal text-muted-foreground hover:text-foreground">
              收起
            </button>
          </div>
          <div className="space-y-2">
            {risks.map((risk) => (
              <div key={risk.name} className="rounded-md border border-border bg-background/60 p-2.5">
                <div className="flex items-center gap-2 text-xs">
                  <span className={cn(
                    "shrink-0 rounded px-1.5 py-0.5 font-medium",
                    risk.level === "critical" ? "bg-destructive/20 text-destructive"
                      : risk.level === "high" ? "bg-destructive/10 text-destructive"
                      : "bg-warning/15 text-warning",
                  )}>
                    {risk.level}
                  </span>
                  <span className="truncate font-medium">{risk.name}</span>
                  <span className="shrink-0 text-muted-foreground">
                    {risk.pool === "official" ? "正式池" : "待定池"}
                  </span>
                  <div className="ml-auto flex shrink-0 items-center gap-1">
                    {risk.pool === "official" && (
                      <button
                        onClick={() => void demoteSkill(risk.name)}
                        className="rounded px-2 py-0.5 text-warning hover:bg-warning/10"
                        title="退回待定池，立刻停止注入到每个 agent 请求"
                      >
                        退回待定池
                      </button>
                    )}
                    <button
                      onClick={() => void removeSkill(risk.name)}
                      className="rounded px-2 py-0.5 text-destructive hover:bg-destructive/10"
                    >
                      删除
                    </button>
                  </div>
                </div>
                <div className="mt-1.5 space-y-1">
                  {risk.findings.slice(0, 4).map((finding, index) => (
                    <div key={index} className="text-[11px] leading-4">
                      <span className="text-muted-foreground">第 {finding.line} 行 · {finding.why}</span>
                      <div className="mt-0.5 break-all rounded bg-muted/40 px-1.5 py-1 font-mono text-foreground/80">
                        {finding.excerpt}
                      </div>
                    </div>
                  ))}
                  {risk.findings.length > 4 && (
                    <div className="text-[11px] text-muted-foreground">…还有 {risk.findings.length - 4} 条</div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-hidden p-4">
        <SkillPoolPanel key={poolKey} />
      </div>

      {showMarket && (
        <SkillMarketPanel
          onClose={() => setShowMarket(false)}
          onImported={() => setPoolKey((k) => k + 1)}
        />
      )}
      {showSync && <SkillSyncPanel onClose={() => setShowSync(false)} />}
    </div>
  );
}
