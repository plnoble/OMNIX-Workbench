/**
 * SkillPoolPanel — 技能池治理 (#3 技能池重构).
 *
 * 待定池（收集/锻造/融合的一切先进这里，绝不直接用）→ AI 审核（硬性门槛）→
 * 用户拍板晋升 → 正式池（网关注入直调，所有 agent 共享，零分发）。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Ban,
  CheckCircle2,
  ChevronUp,
  Download,
  Eye,
  Loader2,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import type { PlatformModel } from "@/types";
import {
  modelApi,
  settingsApi,
  skillPoolApi,
  type SkillPoolItem,
} from "@/lib/tauri-api";

const VERDICT_META: Record<string, { label: string; cls: string }> = {
  pass: { label: "审核通过", cls: "border-success/40 bg-success/10 text-success" },
  needs_work: { label: "需改造", cls: "border-warning/40 bg-warning/10 text-warning" },
  reject: { label: "不合格", cls: "border-destructive/40 bg-destructive/10 text-destructive" },
};

export function SkillPoolPanel() {
  const [items, setItems] = useState<SkillPoolItem[]>([]);
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [chatModel, setChatModel] = useState("");
  const [injectionOn, setInjectionOn] = useState(true);
  const [collecting, setCollecting] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [reviewing, setReviewing] = useState<Set<string>>(new Set());
  const [batchReviewing, setBatchReviewing] = useState(false);
  const [preview, setPreview] = useState<{ name: string; content: string } | null>(null);

  const load = useCallback(async () => {
    try {
      setItems(await skillPoolApi.list());
    } catch (e) {
      toast.error(`读取技能池失败：${e}`);
    }
  }, []);

  useEffect(() => {
    void load();
    modelApi
      .getActive()
      .then((list) => {
        const usable = list.filter(
          (m) =>
            !m.model_name.toLowerCase().includes("embedding") &&
            !m.model_name.toLowerCase().includes("rerank"),
        );
        setModels(usable);
        if (usable.length > 0) setChatModel(`${usable[0].platform_id}:${usable[0].model_name}`);
      })
      .catch(() => {});
    settingsApi
      .get("skill_gateway_injection")
      .then((v) => setInjectionOn(v !== "0"))
      .catch(() => {});
  }, [load]);

  const pending = useMemo(() => items.filter((i) => i.pool === "pending"), [items]);
  const official = useMemo(() => items.filter((i) => i.pool === "official"), [items]);

  const handleCollect = async () => {
    setCollecting(true);
    try {
      const r = await skillPoolApi.collectAll();
      toast.success(
        `收集完成：扫描 ${r.tools_scanned} 个工具，发现 ${r.found_total} 个技能，新纳入待定池 ${r.imported} 个`,
      );
      void load();
    } catch (e) {
      toast.error(`收集失败：${e}`);
    } finally {
      setCollecting(false);
    }
  };

  const handleCleanup = async () => {
    if (
      !window.confirm(
        "清理散落原件：把各工具目录里已收集的技能原件备份后删除，让中央库成为唯一来源。\n\n所有被删内容都会先备份到 ~/.omnix/backups/。继续？",
      )
    )
      return;
    setCleaning(true);
    try {
      const r = await skillPoolApi.cleanupScattered();
      toast.success(`已清理 ${r.cleaned} 处散落技能，备份在 ${r.backup_dir}`);
      if (r.errors.length > 0) toast.warning(`有 ${r.errors.length} 处失败：${r.errors[0]}…`);
      void load();
    } catch (e) {
      toast.error(`清理失败：${e}`);
    } finally {
      setCleaning(false);
    }
  };

  const reviewOne = useCallback(
    async (name: string) => {
      if (!chatModel) {
        toast.error("请先在「模型中心」启用一个对话模型");
        return false;
      }
      setReviewing((prev) => new Set(prev).add(name));
      try {
        await skillPoolApi.review(name, chatModel);
        return true;
      } catch (e) {
        toast.error(`审核「${name}」失败：${e}`);
        return false;
      } finally {
        setReviewing((prev) => {
          const next = new Set(prev);
          next.delete(name);
          return next;
        });
      }
    },
    [chatModel],
  );

  const handleReview = async (name: string) => {
    if (await reviewOne(name)) {
      toast.success(`「${name}」审核完成`);
      void load();
    }
  };

  const handleBatchReview = async () => {
    const targets = pending.filter((i) => !i.reviewed_at).map((i) => i.name);
    if (targets.length === 0) {
      toast.info("待定池里没有未审核的技能");
      return;
    }
    if (!window.confirm(`将逐个 AI 审核 ${targets.length} 个未审核技能，可能需要一些时间。继续？`)) return;
    setBatchReviewing(true);
    let ok = 0;
    for (const name of targets) {
      if (await reviewOne(name)) ok += 1;
      void load();
    }
    setBatchReviewing(false);
    toast.success(`批量审核完成：${ok}/${targets.length} 成功`);
    void load();
  };

  const handlePromote = async (name: string) => {
    try {
      await skillPoolApi.setPool(name, "official");
      toast.success(`「${name}」已晋升正式池`);
      void load();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDemote = async (name: string) => {
    try {
      await skillPoolApi.setPool(name, "pending");
      toast.success(`「${name}」已退回待定池`);
      void load();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handlePreview = async (name: string) => {
    try {
      setPreview({ name, content: await skillPoolApi.content(name) });
    } catch (e) {
      toast.error(`读取内容失败：${e}`);
    }
  };

  const toggleInjection = async () => {
    const next = !injectionOn;
    try {
      await settingsApi.set("skill_gateway_injection", next ? "1" : "0");
      setInjectionOn(next);
      toast.success(next ? "网关注入已开启：正式池技能对所有 agent 直接生效" : "网关注入已关闭");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const renderCard = (item: SkillPoolItem, isOfficial: boolean) => {
    const verdict = item.review_verdict ? VERDICT_META[item.review_verdict] : null;
    const busy = reviewing.has(item.name);
    return (
      <div key={item.name} className="rounded-lg border border-border bg-card/40 p-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="truncate text-sm font-semibold" title={item.name}>{item.name}</span>
              {verdict && (
                <span className={cn("shrink-0 rounded border px-1.5 py-0.5 text-[10px]", verdict.cls)}>
                  {verdict.label}{item.review_score != null ? ` ${item.review_score}` : ""}
                </span>
              )}
              {!item.reviewed_at && (
                <span className="shrink-0 rounded border border-border bg-muted/30 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  未审核
                </span>
              )}
            </div>
            <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground" title={item.description}>
              {item.description}
            </p>
            {item.review_summary && (
              <p className="mt-1 line-clamp-2 text-xs text-primary/80" title={item.review_summary}>
                审核：{item.review_summary}
              </p>
            )}
            <div className="mt-1 flex items-center gap-2 text-[10px] text-muted-foreground">
              {item.source_ref?.startsWith("tool:") && <span>来自 {item.source_ref.slice(5)}</span>}
              {item.usage_count > 0 && <span>被调用 {item.usage_count} 次</span>}
            </div>
          </div>
          <div className="flex shrink-0 flex-col items-end gap-1.5">
            <div className="flex items-center gap-1">
              <button
                onClick={() => void handlePreview(item.name)}
                title="查看内容"
                className="rounded p-1 text-muted-foreground hover:bg-muted/40 hover:text-foreground"
              >
                <Eye className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={() => void handleReview(item.name)}
                disabled={busy || batchReviewing}
                title="AI 审核"
                className="rounded p-1 text-muted-foreground hover:bg-muted/40 hover:text-foreground disabled:opacity-40"
              >
                {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
              </button>
            </div>
            {isOfficial ? (
              <button
                onClick={() => void handleDemote(item.name)}
                className="inline-flex items-center gap-1 rounded border border-border px-2 py-0.5 text-[11px] text-muted-foreground hover:bg-muted/40"
              >
                <Ban className="h-3 w-3" /> 退回待定
              </button>
            ) : (
              <button
                onClick={() => void handlePromote(item.name)}
                disabled={!item.reviewed_at}
                title={item.reviewed_at ? "晋升正式池" : "必须先通过 AI 审核"}
                className="inline-flex items-center gap-1 rounded border border-success/40 bg-success/10 px-2 py-0.5 text-[11px] text-success hover:bg-success/20 disabled:cursor-not-allowed disabled:opacity-40"
              >
                <ChevronUp className="h-3 w-3" /> 晋升正式
              </button>
            )}
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="flex flex-col gap-4">
      {/* toolbar */}
      <div className="card flex flex-wrap items-center gap-2 p-3">
        <button
          onClick={() => void handleCollect()}
          disabled={collecting}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
        >
          {collecting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
          一键收集全盘技能
        </button>
        <button
          onClick={() => void handleCleanup()}
          disabled={cleaning}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
        >
          {cleaning ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
          清理散落原件
        </button>
        <button
          onClick={() => void handleBatchReview()}
          disabled={batchReviewing}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
        >
          {batchReviewing ? <Loader2 className="h-4 w-4 animate-spin" /> : <ShieldCheck className="h-4 w-4" />}
          批量审核未审核项
        </button>
        <select
          value={chatModel}
          onChange={(e) => setChatModel(e.target.value)}
          className="h-8 rounded-lg border border-border bg-background px-2 text-sm"
          title="审核使用的模型"
        >
          {models.length === 0 && <option value="">（无可用模型）</option>}
          {models.map((m) => (
            <option key={`${m.platform_id}:${m.model_name}`} value={`${m.platform_id}:${m.model_name}`}>
              {m.model_name}
            </option>
          ))}
        </select>
        <label className="ml-auto flex cursor-pointer items-center gap-2 text-sm text-muted-foreground">
          <span>网关注入直调</span>
          <button
            onClick={() => void toggleInjection()}
            className={cn(
              "relative h-5 w-9 rounded-full transition",
              injectionOn ? "bg-success" : "bg-muted",
            )}
            title="开启后：正式池技能按语义匹配自动注入所有经过网关的请求，零分发"
          >
            <span
              className={cn(
                "absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all",
                injectionOn ? "left-[18px]" : "left-0.5",
              )}
            />
          </button>
        </label>
      </div>

      <p className="text-xs leading-5 text-muted-foreground">
        收集/锻造/融合的技能一律先进<b>待定池</b>，绝不直接使用。经 AI 审核（实质性/质量/安全/重复）后由你拍板晋升
        <b>正式池</b>——正式池技能按语义匹配注入网关请求，所有 agent 直接调用，无需分发。宁少而强。
      </p>

      {/* two pools */}
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <div>
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
            <Sparkles className="h-4 w-4 text-warning" /> 待定池
            <span className="rounded bg-muted/60 px-1.5 text-xs text-muted-foreground">{pending.length}</span>
          </div>
          <div className="flex flex-col gap-2">
            {pending.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border p-6 text-center text-xs text-muted-foreground">
                空——点「一键收集全盘技能」把散落各处的技能收进来。
              </div>
            ) : (
              pending.map((i) => renderCard(i, false))
            )}
          </div>
        </div>
        <div>
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold">
            <CheckCircle2 className="h-4 w-4 text-success" /> 正式池
            <span className="rounded bg-muted/60 px-1.5 text-xs text-muted-foreground">{official.length}</span>
          </div>
          <div className="flex flex-col gap-2">
            {official.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border p-6 text-center text-xs text-muted-foreground">
                空——待定池技能通过审核后，点「晋升正式」放进来。
              </div>
            ) : (
              official.map((i) => renderCard(i, true))
            )}
          </div>
        </div>
      </div>

      {/* content preview modal */}
      {preview && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8"
          onClick={() => setPreview(null)}
        >
          <div
            className="flex max-h-full w-full max-w-3xl flex-col rounded-xl border border-border bg-background shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
              <span className="text-sm font-semibold">{preview.name}</span>
              <button
                onClick={() => setPreview(null)}
                className="rounded px-2 py-1 text-sm text-muted-foreground hover:bg-muted/40"
              >
                关闭
              </button>
            </div>
            <pre className="flex-1 overflow-auto whitespace-pre-wrap p-4 text-xs leading-5">
              {preview.content}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
}
