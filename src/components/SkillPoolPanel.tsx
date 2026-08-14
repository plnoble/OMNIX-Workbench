/**
 * SkillPoolPanel — 技能中心 (R2 重构：看得懂、改得动).
 *
 * 一条流水线代替七个分区：收集 → 待定池（中文摘要看得懂 + AI 审核给出具体
 * 问题与改造建议）→ AI 改造/融合（预览后应用，改完必须重新过审）→ 用户拍板
 * 晋升 → 正式池（网关注入直调，全 agent 共享，零分发）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Ban,
  CheckCircle2,
  ChevronUp,
  Combine,
  Download,
  Hammer,
  Loader2,
  RefreshCw,
  Search,
  Share2,
  ShieldCheck,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import type { PlatformModel } from "@/types";
import {
  modelApi,
  settingsApi,
  skillPoolApi,
  skillProvenanceApi,
  skillUpdatesApi,
  type SkillConflict,
  type SkillFusionProposal,
  type SkillPoolItem,
  type SkillProvenance,
  type SkillReformProposal,
} from "@/lib/tauri-api";

/**
 * 存证徽标。**只给「不一致」的状态**——`ok` 不出徽标。
 *
 * 一屏绿勾等于没有信息；这里要看见的是那几条对不上的。
 */
const LOCK_META: Record<string, { label: string; cls: string }> = {
  drifted: { label: "内容已变", cls: "border-destructive/50 bg-destructive/10 text-destructive" },
  unlocked: { label: "未存证", cls: "border-border bg-muted/30 text-muted-foreground" },
  missing: { label: "文件缺失", cls: "border-warning/50 bg-warning/10 text-warning" },
};

const VERDICT_META: Record<string, { label: string; cls: string }> = {
  pass: { label: "审核通过", cls: "border-success/40 bg-success/10 text-success" },
  needs_work: { label: "需改造", cls: "border-warning/40 bg-warning/10 text-warning" },
  reject: { label: "不合格", cls: "border-destructive/40 bg-destructive/10 text-destructive" },
};

type PoolFilter = "all" | "pending" | "official" | "unreviewed";

export function SkillPoolPanel() {
  const [items, setItems] = useState<SkillPoolItem[]>([]);
  /** 技能名 → 存证。空 Map 表示还没读到，不是「都没问题」。 */
  const [provenance, setProvenance] = useState<Map<string, SkillProvenance>>(new Map());
  const [relocking, setRelocking] = useState("");
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [chatModel, setChatModel] = useState("");
  const [injectionOn, setInjectionOn] = useState(true);
  const [memoryRecallOn, setMemoryRecallOn] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<PoolFilter>("all");
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [showContent, setShowContent] = useState(false);

  const [collecting, setCollecting] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [reviewing, setReviewing] = useState<Set<string>>(new Set());
  const [batchReviewing, setBatchReviewing] = useState(false);
  const [summarizing, setSummarizing] = useState(false);

  // 融合多选
  const [fuseMode, setFuseMode] = useState(false);
  const [fusePicks, setFusePicks] = useState<Set<string>>(new Set());
  const [fusing, setFusing] = useState(false);
  const [fusionProposal, setFusionProposal] = useState<SkillFusionProposal | null>(null);
  // 默认让源技能退出正式池：融合的本意是「取代」，不是再多一个重叠技能
  // （留着会被审核判为与源技能重复，越忠实融合越吃亏）。可取消。
  const [retireSources, setRetireSources] = useState(true);
  /** 融合结果的落库名。可编辑——见 freeSkillName 的注释。 */
  const [fusionName, setFusionName] = useState("");
  /** 非空表示我们替模型改过名，界面要说明原名是什么、为什么改。 */
  const [fusionRenamed, setFusionRenamed] = useState("");

  // AI 改造
  const [reformOpen, setReformOpen] = useState(false);
  const [reformInstruction, setReformInstruction] = useState("");
  const [reforming, setReforming] = useState(false);
  const [reformProposal, setReformProposal] = useState<SkillReformProposal | null>(null);

  const summarizedOnce = useRef<Set<string>>(new Set());

  // 技能自动更新：进入面板时静默检查一次；干净更新自动应用（已备份），
  // 冲突（两边都改过）绝不猜，列出来等用户定。
  const [conflicts, setConflicts] = useState<SkillConflict[]>([]);
  const updateCheckedOnce = useRef(false);

  const load = useCallback(async () => {
    try {
      setItems(await skillPoolApi.list());
    } catch (e) {
      toast.error(`读取技能失败：${e}`);
    }
    // 存证单独一路：审计读的是磁盘上的技能文件，比列表慢，而且失败了不该连
    // 技能列表一起看不到——那才是这个面板的主体。
    try {
      const rows = await skillProvenanceApi.audit();
      setProvenance(new Map(rows.map((r) => [r.name, r])));
    } catch {
      /* 存证读不到就不显示徽标，不打扰 */
    }
  }, []);

  /**
   * 重新上锁：把指纹更新到当前内容。
   *
   * 故意不做成「检测到变化就自动更新」——那样这个功能就白做了：内容被谁改过
   * 都会被无声抹平。「内容变了」和「我认可这个变化」是两件事，后者必须有人按。
   */
  const relock = useCallback(
    async (name: string) => {
      setRelocking(name);
      try {
        await skillProvenanceApi.relock(name);
        toast.success(`已重新上锁：${name}`);
        await load();
      } catch (e) {
        toast.error(`上锁失败：${e}`);
      } finally {
        setRelocking("");
      }
    },
    [load],
  );

  useEffect(() => {
    void load();
    if (!updateCheckedOnce.current) {
      updateCheckedOnce.current = true;
      skillUpdatesApi
        .check(true)
        .then((report) => {
          setConflicts(report.conflicts);
          if (report.updated.length > 0) {
            const reReview = report.updated.filter((u) => u.needs_re_review).length;
            toast.success(`已自动更新 ${report.updated.length} 个技能（原版本已备份）`, {
              description: reReview > 0 ? `其中 ${reReview} 个在正式池，建议复审` : undefined,
            });
            void load();
          }
          for (const err of report.errors) toast.error(err);
        })
        .catch(() => {});
    }
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
    settingsApi
      .get("memory_gateway_recall")
      .then((v) => setMemoryRecallOn(v === "1"))
      .catch(() => {});
  }, [load]);

  const selected = useMemo(
    () => items.find((i) => i.name === selectedName) ?? null,
    [items, selectedName],
  );

  /**
   * 融合名的实时校验。撞名这件事必须在**点保存之前**就说清楚——
   * 原来是存的时候后端才报「已存在——换个名字」，而那个对话框里根本没有
   * 能改名字的地方，用户只剩「放弃」。
   */
  const fusionNameError = useMemo(() => {
    const n = fusionName.trim();
    if (!n) return "技能名不能为空";
    if (/[/\:*?"<>|]/.test(n)) return "技能名会作为文件夹名，不能含 / \ : * ? \" < > |";
    if (n === "." || n === ".." || n.includes("..")) return "技能名不能是 . 或 .. ，也不能含 ..";
    if (items.some((i) => i.name === n)) return `已有技能叫「${n}」，换一个`;
    return "";
  }, [fusionName, items]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return items.filter((i) => {
      if (filter === "pending" && i.pool !== "pending") return false;
      if (filter === "official" && i.pool !== "official") return false;
      if (filter === "unreviewed" && i.reviewed_at) return false;
      if (!q) return true;
      return (
        i.name.toLowerCase().includes(q) ||
        i.description.toLowerCase().includes(q) ||
        i.summary_zh.toLowerCase().includes(q) ||
        (i.category ?? "").toLowerCase().includes(q)
      );
    });
  }, [items, search, filter]);

  // 选中技能：拉内容 + 摘要为空时自动生成一次（看得懂）
  useEffect(() => {
    if (!selectedName) return;
    setContent("");
    setShowContent(false);
    skillPoolApi
      .content(selectedName)
      .then(setContent)
      .catch(() => setContent("（读取内容失败）"));
    const item = items.find((i) => i.name === selectedName);
    if (
      item &&
      !item.summary_zh &&
      chatModel &&
      !summarizedOnce.current.has(selectedName)
    ) {
      summarizedOnce.current.add(selectedName);
      setSummarizing(true);
      skillPoolApi
        .summarize(selectedName, chatModel)
        .then(() => void load())
        .catch(() => {})
        .finally(() => setSummarizing(false));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedName]);

  const handleCollect = async () => {
    setCollecting(true);
    try {
      const r = await skillPoolApi.collectAll();
      toast.success(
        `收集完成：扫描 ${r.tools_scanned} 个工具，发现 ${r.found_total} 个，新纳入待定池 ${r.imported} 个`,
      );
      void load();
    } catch (e) {
      toast.error(`收集失败：${e}`);
    } finally {
      setCollecting(false);
    }
  };

  const [exporting, setExporting] = useState(false);

  /**
   * T3：把正式池镜像到 ~/.agents/skills/——pi / Claude Code / Codex 都读这个
   * 目录，同一批技能就能对用户装的所有 harness 生效。
   * 那是共享目录，所以后端绝不覆盖别人改过的文件；冲突要原样报给用户，
   * 静默跳过会让人以为技能已经共享出去了。
   */
  const handleExportToAgentsDir = async () => {
    setExporting(true);
    try {
      const summary = await skillPoolApi.exportToAgentsDir();
      if (summary.includes("被跳过")) toast.warning(summary);
      else toast.success(summary);
    } catch (e) {
      toast.error(`导出到公共技能目录失败：${e}`);
    } finally {
      setExporting(false);
    }
  };

  const handleCleanup = async () => {
    if (
      !window.confirm(
        "清理散落原件：把各工具目录里已收集的技能原件备份后删除，让中央库成为唯一来源。\n\n备份位置可在「设置→数据备份→存储位置」里修改。继续？",
      )
    )
      return;
    setCleaning(true);
    try {
      const r = await skillPoolApi.cleanupScattered();
      toast.success(`已清理 ${r.cleaned} 处，备份在 ${r.backup_dir}`);
      if (r.errors.length > 0) toast.warning(`${r.errors.length} 处失败：${r.errors[0]}…`);
      void load();
    } catch (e) {
      toast.error(`清理失败：${e}`);
    } finally {
      setCleaning(false);
    }
  };

  const ensureModel = () => {
    if (!chatModel) {
      toast.error("请先在「模型中心」启用一个对话模型");
      return false;
    }
    return true;
  };

  const reviewOne = useCallback(
    async (name: string) => {
      if (!ensureModel()) return false;
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [chatModel],
  );

  const handleReview = async (name: string) => {
    if (await reviewOne(name)) {
      toast.success(`「${name}」审核完成`);
      void load();
    }
  };

  const handleBatchReview = async () => {
    const targets = items.filter((i) => i.pool === "pending" && !i.reviewed_at).map((i) => i.name);
    if (targets.length === 0) {
      toast.info("没有未审核的待定技能");
      return;
    }
    if (!window.confirm(`将逐个 AI 审核 ${targets.length} 个技能，可能较慢。继续？`)) return;
    setBatchReviewing(true);
    let ok = 0;
    for (const name of targets) {
      if (await reviewOne(name)) ok += 1;
      void load();
    }
    setBatchReviewing(false);
    toast.success(`批量审核完成：${ok}/${targets.length}`);
    void load();
  };

  const handleSummarize = async (name: string) => {
    if (!ensureModel()) return;
    setSummarizing(true);
    try {
      await skillPoolApi.summarize(name, chatModel);
      void load();
    } catch (e) {
      toast.error(`生成摘要失败：${e}`);
    } finally {
      setSummarizing(false);
    }
  };

  const handleSetPool = async (name: string, pool: "pending" | "official") => {
    try {
      await skillPoolApi.setPool(name, pool);
      toast.success(pool === "official" ? `「${name}」已晋升正式池` : `「${name}」已退回待定池`);
      void load();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (name: string) => {
    if (!window.confirm(`删除技能「${name}」？中央库文件会先备份再删除。`)) return;
    try {
      await skillPoolApi.remove(name);
      toast.success(`已删除「${name}」`);
      if (selectedName === name) setSelectedName(null);
      void load();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleReform = async () => {
    if (!selected || !ensureModel()) return;
    setReforming(true);
    setReformProposal(null);
    try {
      const p = await skillPoolApi.reform(selected.name, chatModel, reformInstruction || undefined);
      setReformProposal(p);
    } catch (e) {
      toast.error(`AI 改造失败：${e}`);
    } finally {
      setReforming(false);
    }
  };

  const handleApplyReform = async () => {
    if (!selected || !reformProposal) return;
    try {
      await skillPoolApi.applyReform(selected.name, reformProposal.new_content);
      toast.success("改造已应用——技能回到待定池，请重新审核");
      setReformOpen(false);
      setReformProposal(null);
      setReformInstruction("");
      summarizedOnce.current.delete(selected.name);
      void load();
    } catch (e) {
      toast.error(String(e));
    }
  };

  /**
   * 融合结果起个不撞车的名字。
   *
   * 模型几乎必然把「gstack + browse 的融合」命名为 gstack——它就是想表达
   * 「这是升级版的 gstack」。但技能名是唯一键，撞上就存不进去。
   * 原来这是个死路：名字在对话框里只是只读文本，报错之后只剩「放弃」，
   * 融合结果整个丢掉。**最常见的路径被当成了异常路径。**
   *
   * 现在打开对话框时就先让开，并在界面上说明为什么改了名。用户仍可改回
   * 任何没被占用的名字。
   */
  const freeSkillName = (proposed: string, taken: Set<string>): string => {
    if (!taken.has(proposed)) return proposed;
    if (!taken.has(`${proposed}-merged`)) return `${proposed}-merged`;
    for (let i = 2; i < 100; i += 1) {
      if (!taken.has(`${proposed}-merged${i}`)) return `${proposed}-merged${i}`;
    }
    return `${proposed}-${Date.now()}`;
  };

  const handleFuse = async () => {
    if (fusePicks.size < 2) {
      toast.error("至少勾选 2 个技能");
      return;
    }
    if (!ensureModel()) return;
    setFusing(true);
    try {
      const p = await skillPoolApi.fuse([...fusePicks], chatModel);
      const taken = new Set(items.map((i) => i.name));
      const safe = freeSkillName(p.name, taken);
      setFusionProposal(p);
      setFusionName(safe);
      setFusionRenamed(safe !== p.name ? p.name : "");
    } catch (e) {
      toast.error(`融合失败：${e}`);
    } finally {
      setFusing(false);
    }
  };

  const handleApplyFusion = async () => {
    if (!fusionProposal) return;
    try {
      const sources = fusionProposal.sources ?? [];
      const finalName = fusionName.trim();
      await skillPoolApi.applyFusion(
        finalName,
        fusionProposal.description,
        fusionProposal.content,
        sources,
        retireSources,
      );
      toast.success(
        `融合技能「${finalName}」已进入待定池`,
        retireSources && sources.length
          ? { description: `源技能 ${sources.join("、")} 已退回待定池，可随时恢复` }
          : undefined,
      );
      setFusionProposal(null);
      setFusionName("");
      setFusionRenamed("");
      setFuseMode(false);
      setFusePicks(new Set());
      void load();
    } catch (e) {
      toast.error(String(e));
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

  const toggleMemoryRecall = async () => {
    const next = !memoryRecallOn;
    try {
      await settingsApi.set("memory_gateway_recall", next ? "1" : "0");
      setMemoryRecallOn(next);
      toast.success(next ? "记忆自动召回已开启：相关历史经验会自动进入上下文" : "记忆自动召回已关闭");
    } catch (e) {
      toast.error(String(e));
    }
  };

  const pendingCount = items.filter((i) => i.pool === "pending").length;
  const officialCount = items.filter((i) => i.pool === "official").length;

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      {/* toolbar */}
      <div className="card flex flex-wrap items-center gap-2 p-3">
        <button
          onClick={() => void handleCollect()}
          disabled={collecting}
          className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
        >
          {collecting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
          一键收集
        </button>
        <button
          onClick={() => void handleExportToAgentsDir()}
          disabled={exporting}
          title="把正式池技能镜像到 ~/.agents/skills/，让 Claude Code、Codex、pi 等其它 agent 也能用。不会覆盖你在那里手改过的文件。"
          className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border px-3 text-sm hover:bg-muted/40 disabled:opacity-50"
        >
          {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Share2 className="h-4 w-4" />}
          共享给其它 agent
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
          批量审核
        </button>
        <button
          onClick={() => {
            setFuseMode((v) => !v);
            setFusePicks(new Set());
          }}
          className={cn(
            "inline-flex h-8 items-center gap-1.5 rounded-lg border px-3 text-sm",
            fuseMode
              ? "border-primary bg-primary/10 text-primary"
              : "border-border hover:bg-muted/40",
          )}
        >
          <Combine className="h-4 w-4" /> {fuseMode ? "取消融合" : "融合…"}
        </button>
        {fuseMode && (
          <button
            onClick={() => void handleFuse()}
            disabled={fusing || fusePicks.size < 2}
            className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {fusing ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
            融合选中的 {fusePicks.size} 个
          </button>
        )}
        <select
          value={chatModel}
          onChange={(e) => setChatModel(e.target.value)}
          className="h-8 max-w-44 rounded-lg border border-border bg-background px-2 text-sm"
          title="审核/摘要/改造使用的模型"
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
            className={cn("relative h-5 w-9 rounded-full transition", injectionOn ? "bg-success" : "bg-muted")}
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
        <label className="flex cursor-pointer items-center gap-2 text-sm text-muted-foreground">
          <span>记忆自动召回</span>
          <button
            onClick={() => void toggleMemoryRecall()}
            className={cn("relative h-5 w-9 rounded-full transition", memoryRecallOn ? "bg-success" : "bg-muted")}
            title="开启后：按当前任务词法匹配相关历史经验/教训，自动注入上下文（最多 3 条，默认关）"
          >
            <span
              className={cn(
                "absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all",
                memoryRecallOn ? "left-[18px]" : "left-0.5",
              )}
            />
          </button>
        </label>
      </div>

      {/* search + filter */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-52 flex-1">
          <Search className="absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索技能名 / 描述 / 摘要…"
            className="h-8 w-full rounded-lg border border-border bg-background pl-8 pr-2 text-sm outline-none focus:border-primary"
          />
        </div>
        {(
          [
            ["all", `全部 ${items.length}`],
            ["pending", `待定 ${pendingCount}`],
            ["official", `正式 ${officialCount}`],
            ["unreviewed", "未审核"],
          ] as [PoolFilter, string][]
        ).map(([id, label]) => (
          <button
            key={id}
            onClick={() => setFilter(id)}
            className={cn(
              "h-7 rounded-full border px-3 text-xs",
              filter === id
                ? "border-primary bg-primary/10 text-primary"
                : "border-border text-muted-foreground hover:bg-muted/40",
            )}
          >
            {label}
          </button>
        ))}
      </div>

      {/* 更新冲突：中央副本和源都改过，必须用户拍板 */}
      {conflicts.length > 0 && (
        <div className="flex flex-col gap-1.5 rounded-lg border border-warning/40 bg-warning/5 p-2.5 text-xs">
          <div className="flex items-center gap-1.5 font-medium text-warning">
            <RefreshCw className="h-3.5 w-3.5" />
            {conflicts.length} 个技能的源更新需要你裁决——保留哪边？
          </div>
          {conflicts.map((c) => (
            <div key={c.name} className="flex items-center gap-2">
              <span className="truncate text-muted-foreground" title={c.reason}>
                「{c.name}」（源：{c.from_tool}）
                <span className="ml-1 text-warning/80">· {c.reason}</span>
              </span>
              <button
                className="ml-auto shrink-0 rounded border border-border px-2 py-0.5 hover:bg-muted/40"
                onClick={() => {
                  void skillUpdatesApi
                    .resolveConflict(c.name, c.source_path, true)
                    .then(() => {
                      toast.success(`「${c.name}」已用源覆盖（原版本已备份）`);
                      setConflicts((prev) => prev.filter((x) => x.name !== c.name));
                      void load();
                    })
                    .catch((e) => toast.error(String(e)));
                }}
              >
                用源覆盖
              </button>
              <button
                className="shrink-0 rounded border border-border px-2 py-0.5 hover:bg-muted/40"
                onClick={() => {
                  void skillUpdatesApi
                    .resolveConflict(c.name, c.source_path, false)
                    .then(() => {
                      toast.success(`「${c.name}」保留中央版本`);
                      setConflicts((prev) => prev.filter((x) => x.name !== c.name));
                    })
                    .catch((e) => toast.error(String(e)));
                }}
              >
                保留中央
              </button>
            </div>
          ))}
        </div>
      )}

      {/* list + detail */}
      <div className="flex min-h-0 flex-1 gap-3">
        {/* list */}
        <div className="flex w-80 shrink-0 flex-col gap-1.5 overflow-y-auto pr-1">
          {filtered.length === 0 && (
            <div className="rounded-lg border border-dashed border-border p-6 text-center text-xs text-muted-foreground">
              {items.length === 0 ? "还没有技能——点「一键收集」把电脑里散落的技能收进来。" : "没有匹配的技能"}
            </div>
          )}
          {filtered.map((item) => {
            const verdict = item.review_verdict ? VERDICT_META[item.review_verdict] : null;
            return (
              <div
                key={item.name}
                onClick={() => setSelectedName(item.name)}
                className={cn(
                  "cursor-pointer rounded-lg border p-2.5 transition",
                  selectedName === item.name
                    ? "border-primary bg-primary/5"
                    : "border-border glass-surface hover:border-primary/40",
                )}
              >
                <div className="flex items-center gap-1.5">
                  {fuseMode && (
                    <input
                      type="checkbox"
                      checked={fusePicks.has(item.name)}
                      onChange={(e) => {
                        e.stopPropagation();
                        setFusePicks((prev) => {
                          const next = new Set(prev);
                          if (next.has(item.name)) next.delete(item.name);
                          else next.add(item.name);
                          return next;
                        });
                      }}
                      onClick={(e) => e.stopPropagation()}
                      className="h-3.5 w-3.5"
                    />
                  )}
                  <span className="truncate text-sm font-medium">{item.name}</span>
                  <span
                    className={cn(
                      "ml-auto shrink-0 rounded px-1.5 py-0.5 text-[10px]",
                      item.pool === "official"
                        ? "bg-success/15 text-success"
                        : "bg-warning/15 text-warning",
                    )}
                  >
                    {item.pool === "official" ? "正式" : "待定"}
                  </span>
                </div>
                <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                  {item.summary_zh || item.description}
                </p>
                <div className="mt-1 flex items-center gap-1.5">
                  {item.needs_re_review && (
                    <span
                      className="rounded border border-warning/40 bg-warning/10 px-1 py-0.5 text-[10px] text-warning"
                      title="内容在上次审核之后被自动更新过——建议重跑一次审核"
                    >
                      更新待复审
                    </span>
                  )}
                  {verdict ? (
                    <span className={cn("rounded border px-1 py-0.5 text-[10px]", verdict.cls)}>
                      {verdict.label}
                      {item.review_score != null ? ` ${item.review_score}` : ""}
                    </span>
                  ) : (
                    <span className="rounded border border-border bg-muted/30 px-1 py-0.5 text-[10px] text-muted-foreground">
                      未审核
                    </span>
                  )}
                  {item.usage_count > 0 && (
                    <span
                      className="text-[10px] text-muted-foreground"
                      title="网关把这条技能追加进 system prompt 的次数。不代表它起没起作用——OMNIX 无从得知。"
                    >
                      注入 {item.usage_count}
                    </span>
                  )}
                  {(() => {
                    // 只在正式池且状态不是「一致」时出现。ok 不显示徽标——
                    // 一屏绿勾等于没有信息，真正要看见的是那几条对不上的。
                    const prov = provenance.get(item.name);
                    if (!prov || prov.status.state === "ok") return null;
                    const meta = LOCK_META[prov.status.state];
                    return (
                      <span
                        className={cn("rounded border px-1 py-0.5 text-[10px]", meta.cls)}
                        title={
                          prov.status.state === "drifted"
                            ? `审核时的指纹 ${prov.status.approved.slice(0, 12)}，现在是 ${prov.status.current.slice(0, 12)}`
                            : prov.status.state === "missing"
                              ? prov.status.reason
                              : "本功能上线前晋升的老技能，还没有存证"
                        }
                      >
                        {meta.label}
                      </span>
                    );
                  })()}
                  {provenance.get(item.name)?.status.state === "drifted" && (
                    <button
                      className="rounded border border-border px-1 py-0.5 text-[10px] hover:bg-muted/40 disabled:opacity-50"
                      disabled={relocking === item.name}
                      title="确认当前内容就是想要的，把指纹更新到这一份"
                      onClick={(e) => {
                        e.stopPropagation();
                        void relock(item.name);
                      }}
                    >
                      {relocking === item.name ? "上锁中…" : "重新上锁"}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {/* detail */}
        <div className="flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto">
          {!selected ? (
            <div className="flex flex-1 items-center justify-center rounded-xl border border-dashed border-border text-sm text-muted-foreground">
              选择左侧一个技能查看详情
            </div>
          ) : (
            <>
              {/* header + actions */}
              <div className="card flex flex-wrap items-center gap-2 p-3">
                <span className="text-base font-semibold">{selected.name}</span>
                {selected.source_ref?.startsWith("tool:") && (
                  <span className="rounded bg-muted/40 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                    来自 {selected.source_ref.slice(5)}
                  </span>
                )}
                <div className="ml-auto flex flex-wrap items-center gap-1.5">
                  <button
                    onClick={() => void handleReview(selected.name)}
                    disabled={reviewing.has(selected.name) || batchReviewing}
                    className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40 disabled:opacity-50"
                  >
                    {reviewing.has(selected.name) ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : (
                      <ShieldCheck className="h-3 w-3" />
                    )}
                    AI 审核
                  </button>
                  <button
                    onClick={() => {
                      setReformOpen(true);
                      setReformProposal(null);
                    }}
                    className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40"
                  >
                    <Hammer className="h-3 w-3" /> AI 改造
                  </button>
                  {selected.pool === "official" ? (
                    <button
                      onClick={() => void handleSetPool(selected.name, "pending")}
                      className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs text-muted-foreground hover:bg-muted/40"
                    >
                      <Ban className="h-3 w-3" /> 退回待定
                    </button>
                  ) : (
                    <button
                      onClick={() => void handleSetPool(selected.name, "official")}
                      disabled={!selected.reviewed_at}
                      title={selected.reviewed_at ? "晋升正式池" : "必须先通过 AI 审核"}
                      className="inline-flex h-7 items-center gap-1 rounded-md border border-success/40 bg-success/10 px-2 text-xs text-success hover:bg-success/20 disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      <ChevronUp className="h-3 w-3" /> 晋升正式
                    </button>
                  )}
                  <button
                    onClick={() => void handleDelete(selected.name)}
                    className="inline-flex h-7 items-center gap-1 rounded-md border border-border px-2 text-xs text-muted-foreground hover:text-destructive"
                  >
                    <Trash2 className="h-3 w-3" /> 删除
                  </button>
                </div>
              </div>

              {/* summary — 看得懂 */}
              <div className="card p-3">
                <div className="flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                  <Sparkles className="h-3.5 w-3.5 text-primary" /> 这个技能讲了啥
                  {summarizing && <Loader2 className="h-3 w-3 animate-spin" />}
                  {!selected.summary_zh && !summarizing && (
                    <button
                      onClick={() => void handleSummarize(selected.name)}
                      className="rounded border border-border px-1.5 py-0.5 text-[10px] hover:bg-muted/40"
                    >
                      生成摘要
                    </button>
                  )}
                </div>
                <p className="mt-1.5 text-sm leading-6">
                  {selected.summary_zh || (summarizing ? "AI 正在读这个技能…" : selected.description)}
                </p>
              </div>

              {/* review — 具体到能行动 */}
              <div className="card p-3">
                <div className="flex items-center gap-2 text-xs font-semibold text-muted-foreground">
                  <ShieldCheck className="h-3.5 w-3.5 text-primary" /> 审核结果
                  {selected.review_verdict && (
                    <span
                      className={cn(
                        "rounded border px-1.5 py-0.5 text-[10px]",
                        VERDICT_META[selected.review_verdict]?.cls,
                      )}
                    >
                      {VERDICT_META[selected.review_verdict]?.label} {selected.review_score}
                    </span>
                  )}
                </div>
                {!selected.reviewed_at ? (
                  <p className="mt-1.5 text-sm text-muted-foreground">还没审核——点上方「AI 审核」。</p>
                ) : (
                  <div className="mt-1.5 flex flex-col gap-2 text-sm leading-6">
                    {selected.review_summary && <p>{selected.review_summary}</p>}
                    {selected.review_problems.length > 0 && (
                      <div>
                        <div className="text-xs font-medium text-warning">问题：</div>
                        <ul className="ml-4 list-disc text-xs leading-5 text-muted-foreground">
                          {selected.review_problems.map((p, i) => (
                            <li key={i}>{p}</li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {selected.review_improve && (
                      <div>
                        <div className="text-xs font-medium text-primary">怎么改造成强技能：</div>
                        <p className="text-xs leading-5 text-muted-foreground">{selected.review_improve}</p>
                      </div>
                    )}
                  </div>
                )}
              </div>

              {/* content */}
              <div className="card p-3">
                <button
                  onClick={() => setShowContent((v) => !v)}
                  className="text-xs font-semibold text-muted-foreground hover:text-foreground"
                >
                  {showContent ? "▼ 收起原文" : "▶ 查看原文"}（{content.length} 字符）
                </button>
                {showContent && (
                  <pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap rounded-lg bg-muted/20 p-3 text-xs leading-5">
                    {content}
                  </pre>
                )}
              </div>
            </>
          )}
        </div>
      </div>

      {/* AI 改造 modal */}
      {reformOpen && selected && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8"
          onClick={() => !reforming && setReformOpen(false)}
        >
          <div
            className="flex max-h-full w-full max-w-3xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">
                <Hammer className="mr-1 inline h-4 w-4 text-primary" />
                AI 改造「{selected.name}」
              </span>
              <button onClick={() => setReformOpen(false)} disabled={reforming}>
                <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
              </button>
            </div>
            {!reformProposal ? (
              <>
                <p className="text-xs text-muted-foreground">
                  会基于审核意见把它改造成「强技能」（具体、可执行、去空洞）。可以补充你的要求：
                </p>
                <textarea
                  value={reformInstruction}
                  onChange={(e) => setReformInstruction(e.target.value)}
                  rows={3}
                  placeholder="（可选）比如：聚焦在 Rust 项目；加一节常见报错排查；合并重复段落…"
                  className="w-full resize-none rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
                />
                <button
                  onClick={() => void handleReform()}
                  disabled={reforming}
                  className="inline-flex h-9 items-center justify-center gap-2 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
                >
                  {reforming ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
                  {reforming ? "AI 改造中…" : "生成改造版"}
                </button>
              </>
            ) : (
              <>
                {reformProposal.explanation && (
                  <p className="rounded-lg bg-primary/5 p-2 text-xs leading-5 text-primary">
                    {reformProposal.explanation}
                  </p>
                )}
                <pre className="max-h-[50vh] flex-1 overflow-auto whitespace-pre-wrap rounded-lg bg-muted/20 p-3 text-xs leading-5">
                  {reformProposal.new_content}
                </pre>
                <div className="flex items-center justify-end gap-2">
                  <button
                    onClick={() => setReformProposal(null)}
                    className="h-8 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
                  >
                    重新生成
                  </button>
                  <button
                    onClick={() => void handleApplyReform()}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90"
                  >
                    <CheckCircle2 className="h-4 w-4" /> 应用（回待定池重审）
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* 融合 proposal modal */}
      {fusionProposal && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-8"
          onClick={() => setFusionProposal(null)}
        >
          <div
            className="flex max-h-full w-full max-w-3xl flex-col gap-3 rounded-xl border border-border bg-background p-4 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold">
                <Combine className="mr-1 inline h-4 w-4 text-primary" />
                融合结果：{fusionName || "（未命名）"}
              </span>
              <button onClick={() => setFusionProposal(null)}>
                <X className="h-4 w-4 text-muted-foreground hover:text-foreground" />
              </button>
            </div>
            <p className="text-xs text-muted-foreground">{fusionProposal.description}</p>
            {fusionProposal.explanation && (
              <p className="rounded-lg bg-primary/5 p-2 text-xs leading-5 text-primary">
                {fusionProposal.explanation}
              </p>
            )}
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground" htmlFor="fusion-name">
                技能名（落库用，不能与已有技能重名）
              </label>
              <input
                id="fusion-name"
                value={fusionName}
                onChange={(e) => setFusionName(e.target.value)}
                spellCheck={false}
                className={cn(
                  "h-9 w-full rounded-lg border bg-background px-3 font-mono text-sm outline-none focus:border-primary",
                  fusionNameError ? "border-destructive" : "border-border",
                )}
              />
              {fusionNameError ? (
                <p className="text-xs text-destructive">{fusionNameError}</p>
              ) : fusionRenamed ? (
                <p className="text-xs text-muted-foreground">
                  模型起的名字是 <span className="font-mono">{fusionRenamed}</span>
                  ，与已有技能重名，已自动让开。可以再改。
                </p>
              ) : null}
            </div>
            {(fusionProposal.sources?.length ?? 0) > 0 && (
              <label className="flex cursor-pointer items-start gap-2 rounded-lg border border-border bg-muted/10 p-2 text-xs leading-5">
                <input
                  type="checkbox"
                  className="mt-0.5"
                  checked={retireSources}
                  onChange={(e) => setRetireSources(e.target.checked)}
                />
                <span>
                  入库后让源技能 <strong>{fusionProposal.sources.join("、")}</strong> 退出正式池
                  <span className="block text-muted-foreground">
                    融合的本意是取代它们。留着的话，本技能会因为「与源技能高度重叠」被审核扣分。
                    退回的是待定池，不删除文件，随时可以恢复。
                  </span>
                </span>
              </label>
            )}
            <pre className="max-h-[50vh] flex-1 overflow-auto whitespace-pre-wrap rounded-lg bg-muted/20 p-3 text-xs leading-5">
              {fusionProposal.content}
            </pre>
            <div className="flex items-center justify-end gap-2">
              <button
                onClick={() => setFusionProposal(null)}
                className="h-8 rounded-lg border border-border px-3 text-sm hover:bg-muted/40"
              >
                放弃
              </button>
              <button
                onClick={() => void handleApplyFusion()}
                disabled={!!fusionNameError}
                title={fusionNameError || undefined}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
              >
                <CheckCircle2 className="h-4 w-4" /> 存入待定池
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
