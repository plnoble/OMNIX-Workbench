/**
 * PlatformSubTab — 模型中心主体：供应商、Key、模型列表、路由。
 *
 * 这里的重点是**把路由摆到明面上**。同一个模型名可以注册在多个平台上，网关按
 * `priority DESC, weight DESC` + 模型名哈希决胜负；以前 priority 在界面上完全
 * 不可编辑，用户既看不见会走哪个平台，也无法改——挑中不支持该模型的那个就是
 * 一句没头没脑的 `Model does not exist`。现在：列表顺序即优先级（可拖），模型行
 * 直接标出同名竞争者、当前赢家和最近一次真实失败。
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { usePlatformsStore } from "@/store/AppStore";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Activity, AlertTriangle, Brain, Code, Edit, Eye, GitCompare, GripVertical, Layers, Maximize2, Mic, Plus, RefreshCw, Search, Star, Trash2, Wrench, Zap } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { apiPresetApi, platformApi, platformRoutingApi, modelApi, modelSyncApi, settingsApi, type ModelSyncResult } from "@/lib/tauri-api";
import type { ModelPlatform, ModelRouting, PlatformModel } from "@/types";

/**
 * 预设清单。**id 必须和 `apply_api_preset_core` 里那张表一一对应**——后端按 id
 * 查，对不上就是 `Unknown preset`。这里只放 id 和显示名，地址/协议/默认模型全在
 * 后端那一份，不在前端复制第二份（复制了就会漂）。
 */
const API_PRESETS: [string, string][] = [
  ["openai", "OpenAI"],
  ["anthropic", "Anthropic"],
  ["openrouter", "OpenRouter"],
  ["deepseek", "DeepSeek"],
  ["siliconflow", "硅基流动"],
  ["zhipu", "智谱 GLM"],
  ["moonshot", "月之暗面 Kimi"],
  ["minimax", "MiniMax"],
  ["bailian", "百炼"],
  ["volcengine", "火山引擎"],
  ["ollama", "Ollama（本地）"],
  ["lmstudio", "LM Studio（本地）"],
];

export function PlatformSubTab() {
  const p = usePlatformsStore();
  const {
    platforms, selectedPlatformId, platformModels, modelTestingState,
    fetchingModels, batchTesting,
    selectPlatform: onSelectPlatform,
    togglePlatform: onTogglePlatform,
    deletePlatform: onDeletePlatform,
    fetchRemoteModels: onFetchRemoteModels,
    openModelModal: onAddModel,
    toggleModelEnabled: onToggleModelEnabled,
    testModel: onTestModel,
    deleteModel: onDeleteModel,
    batchTestModels: onBatchTestModels,
  } = p;
  const onAddPlatform = () => p.openPlatformModal();
  const onEditPlatform = (plat: ModelPlatform) => p.openPlatformModal(plat);
  const selectedPlatform = platforms.find((p) => p.id === selectedPlatformId);

  const [applyingPreset, setApplyingPreset] = useState(false);
  const [savingWeight, setSavingWeight] = useState(false);

  /**
   * 存路由权重。**priority 原样带回去**——后端那条命令一次写两列，只传 weight
   * 会把优先级重置成 0，而优先级是用户拖列表拖出来的，悄悄清零最难查。
   */
  const saveWeight = useCallback(
    async (plat: ModelPlatform, weight: number) => {
      setSavingWeight(true);
      try {
        await platformRoutingApi.update(plat.id, weight, plat.priority ?? 0);
        toast.success(`已把「${plat.name}」的权重设为 ${weight}`);
        await p.loadPlatforms();
      } catch (e) {
        toast.error(`保存权重失败：${e}`);
      } finally {
        setSavingWeight(false);
      }
    },
    [p],
  );

  /**
   * 按预设建供应商。**不在这里收 Key**——先把平台建出来，Key 走已有的多 Key 管理
   * （那边才有加密存储）。在这里顺手要一个 Key 会绕开加密策略，正是本轮修过的坑。
   */
  const applyPreset = useCallback(async (presetId: string) => {
    setApplyingPreset(true);
    try {
      const msg = await apiPresetApi.apply(presetId, "");
      toast.success(msg || "已添加，去右侧填 API Key");
      await p.loadPlatforms();
    } catch (e) {
      toast.error(`添加失败：${e}`);
    } finally {
      setApplyingPreset(false);
    }
  }, [p]);

  /**
   * 和上游对比的结果。**先看后做**——不自动应用。
   *
   * 「获取模型」是拉列表全加，看不出上游**下架**了什么；这里的差异能看出来，
   * 而下架恰恰是最该被人过目的那一类（本地还留着一个上游已经没有的模型，
   * 路由过去就是失败）。所以差异摆出来、动作分开按，不合成一个「一键同步」。
   */
  const [diff, setDiff] = useState<ModelSyncResult | null>(null);
  const [comparing, setComparing] = useState(false);
  const [applying, setApplying] = useState(false);

  const compareUpstream = useCallback(async (platformId: string) => {
    setComparing(true);
    try {
      const result = await modelSyncApi.syncPlatform(platformId);
      setDiff(result);
      if (result.error) toast.error(`对比失败：${result.error}`);
    } catch (e) {
      toast.error(`对比失败：${e}`);
    } finally {
      setComparing(false);
    }
  }, []);

  const applyDiff = useCallback(
    async (add: string[], remove: string[]) => {
      if (!diff) return;
      setApplying(true);
      try {
        const [added, removed] = await modelSyncApi.apply(diff.platform_id, add, remove);
        toast.success(`已新增 ${added} 个、移除 ${removed} 个`);
        setDiff(null);
        onFetchRemoteModels();
      } catch (e) {
        toast.error(`应用失败：${e}`);
      } finally {
        setApplying(false);
      }
    },
    [diff, onFetchRemoteModels],
  );

  // 路由说明（同名竞争者 / 当前赢家 / 最近一次真实失败）——按模型 id 索引。
  const [routing, setRouting] = useState<Record<string, ModelRouting>>({});
  const loadRouting = useCallback((platformId: string) => {
    if (!platformId) { setRouting({}); return; }
    modelApi.routing(platformId)
      .then((rows) => setRouting(Object.fromEntries(rows.map((r) => [r.model_id, r]))))
      .catch(() => setRouting({}));
  }, []);
  useEffect(() => { loadRouting(selectedPlatformId); }, [selectedPlatformId, platformModels, loadRouting]);

  // 模型搜索 + 只看某种能力。接一个聚合站就是几十上百个模型，没有筛选没法用。
  const [query, setQuery] = useState("");
  const [capability, setCapability] = useState<"" | keyof PlatformModel>("");
  const visibleModels = useMemo(() => {
    const term = query.trim().toLowerCase();
    return platformModels.filter((model) => {
      if (term && !model.model_name.toLowerCase().includes(term)) return false;
      if (capability && !model[capability]) return false;
      return true;
    });
  }, [platformModels, query, capability]);

  // 排序 = 写 priority。列表顺序就是路由优先级。
  //
  // **不能用 HTML5 draggable**：Tauri 的文件拖放处理器开着（我们靠它实现「拖文件
  // 进对话框」），而它在 Windows 上会吞掉 HTML5 拖放 API——Tauri 自己的文档原话是
  // 「Disables the drag and drop handler. This is required to use HTML5 drag and drop
  // APIs on the frontend on Windows.」两者互斥。所以这里用指针事件自己实现：
  // 按住把手上下移，松手落位。
  const [order, setOrder] = useState<ModelPlatform[]>(platforms);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  useEffect(() => { setOrder(platforms); }, [platforms]);

  const persistOrder = useCallback(async (next: ModelPlatform[]) => {
    try {
      await platformApi.reorder(next.map((p) => p.id));
      toast.success("已更新供应商优先级", { description: "同名模型将优先走靠前的供应商。" });
    } catch (error) {
      toast.error("保存优先级失败", { description: String(error) });
      setOrder(platforms);
    }
  }, [platforms]);

  const startDrag = (event: React.PointerEvent, id: string) => {
    event.preventDefault();
    event.stopPropagation();
    setDraggingId(id);
    const container = (event.currentTarget as HTMLElement).closest("[data-platform-list]");
    if (!container) return;

    let latest = order;
    const move = (moveEvent: PointerEvent) => {
      const rows = Array.from(container.querySelectorAll<HTMLElement>("[data-platform-id]"));
      const overRow = rows.find((row) => {
        const box = row.getBoundingClientRect();
        return moveEvent.clientY >= box.top && moveEvent.clientY <= box.bottom;
      });
      const overId = overRow?.dataset.platformId;
      if (!overId || overId === id) return;
      const from = latest.findIndex((p) => p.id === id);
      const to = latest.findIndex((p) => p.id === overId);
      if (from < 0 || to < 0) return;
      const next = [...latest];
      next.splice(to, 0, next.splice(from, 1)[0]);
      latest = next;
      setOrder(next);
    };
    const finish = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      setDraggingId(null);
      if (latest.map((p) => p.id).join() !== platforms.map((p) => p.id).join()) {
        void persistOrder(latest);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
  };

  // Global default model (Cherry-style hierarchy: global default → Agent binding → session).
  // Stored as "platform_id:model_name" and used by the runtime when an Agent has no binding.
  const [defaultModelKey, setDefaultModelKey] = useState<string>("");
  useEffect(() => {
    settingsApi.get("default_model").then((value) => setDefaultModelKey(value || "")).catch(() => {});
  }, []);
  const modelKey = (model: PlatformModel) => `${model.platform_id}:${model.model_name}`;
  const setAsDefaultModel = async (model: PlatformModel) => {
    const key = modelKey(model);
    const next = defaultModelKey === key ? "" : key;
    try {
      await settingsApi.set("default_model", next);
      setDefaultModelKey(next);
      toast.success(next ? `已设为 Agent 默认模型：${model.model_name}` : "已取消 Agent 默认模型");
    } catch (error) {
      toast.error("设置默认模型失败", { description: String(error) });
    }
  };

  return (
    <div className="flex h-full gap-0">
      {/* 首次配置引导。判断依据必须是「一个平台都没有」——以前查的是
          `p.api_key` 旧列，而 Key 现在存在 `platform_api_keys` 新表里，
          于是配好了 Key 的用户照样看见这条「快速开始」。 */}
      {platforms.length === 0 && (
        <div className="absolute top-2 right-2 z-10 bg-amber-500/10 border border-amber-500/30 rounded-lg px-4 py-2.5 text-xs text-amber-400 flex items-center gap-2">
          <Zap className="h-4 w-4 flex-shrink-0" />
          <span>
            <strong>快速开始：</strong>在下方选择一个平台，填入 API Key 后启用模型，即可使用 QA 翻译、知识库等 AI 功能。
            推荐先配置 <strong>DeepSeek</strong>（国内直连）或 <strong>Ollama</strong>（本地免费）。
          </span>
        </div>
      )}

      {/* Platform List Sidebar — always visible */}
      <div className="w-52 border-r border-border pr-3 flex flex-col gap-3 shrink-0">
        <div className="flex justify-between items-center">
          <span className="text-sm font-semibold text-muted-foreground">模型提供商</span>
          <div className="flex items-center gap-1">
            <select
              className="h-7 rounded border border-border bg-background px-1 text-[11px]"
              value=""
              disabled={applyingPreset}
              title="按预设一键建供应商：地址、协议类型、默认模型都填好，只差你的 Key。"
              onChange={(e) => {
                if (e.target.value) void applyPreset(e.target.value);
              }}
            >
              <option value="">{applyingPreset ? "添加中…" : "按预设添加"}</option>
              {API_PRESETS.map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
            <Button size="sm" variant="outline" onClick={onAddPlatform} className="h-7 w-7 p-0" title="手动新增">
              <Plus className="h-3 w-3" />
            </Button>
          </div>
        </div>
        <p className="-mt-1 text-[11px] leading-4 text-muted-foreground">
          顺序即<strong className="text-foreground">路由优先级</strong>：同一个模型名挂在多个供应商上时，靠前的先用。拖左侧手柄调整。
        </p>

        <div data-platform-list className="flex flex-col gap-1.5 flex-1 overflow-y-auto">
          {order.length === 0 ? (
            <div className="py-5 text-center text-muted-foreground text-xs">无平台</div>
          ) : (
            order.map((plat, index) => {
              const isActive = selectedPlatformId === plat.id;
              return (
                <div
                  key={plat.id}
                  data-platform-id={plat.id}
                  className={cn(
                    "p-2 rounded-lg border cursor-pointer flex justify-between items-center gap-1.5 transition-all",
                    isActive ? "bg-accent/[0.06] border-accent/30" : "bg-muted/5 border-border hover:bg-muted/20",
                    draggingId === plat.id && "opacity-50 ring-1 ring-accent"
                  )}
                  onClick={() => onSelectPlatform(plat.id)}
                >
                  <span
                    onPointerDown={(e) => startDrag(e, plat.id)}
                    onClick={(e) => e.stopPropagation()}
                    title="按住上下拖动，调整优先级"
                    className="shrink-0 cursor-grab touch-none p-0.5 text-muted-foreground/50 hover:text-foreground active:cursor-grabbing"
                  >
                    <GripVertical className="h-3.5 w-3.5" />
                  </span>
                  <div className="min-w-0 flex-1">
                    <span className="font-semibold text-sm block truncate">{plat.name}</span>
                    <span className="text-xs text-muted-foreground">#{index + 1} · {plat.api_type}</span>
                  </div>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Switch
                      checked={plat.is_enabled}
                      onCheckedChange={() => onTogglePlatform(plat)}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Platform Detail */}
      <div className="flex-1 flex flex-col gap-4 min-w-0 pl-4">
        {selectedPlatform ? (
          <>
            {/* Header */}
            <Card>
              <CardContent className="p-4 flex flex-wrap justify-between items-center gap-3">
                <div className="min-w-0">
                  <h3 className="text-base font-semibold mb-1">{selectedPlatform.name}</h3>
                  <span className="text-xs text-muted-foreground">
                    Endpoint: <code className="break-all">{selectedPlatform.api_address}</code>
                  </span>
                  {/*
                    权重：决胜规则 `priority DESC, weight DESC` 的第二个因子。
                    priority 由左侧列表顺序决定（可拖），而 weight **此前没有任何
                    界面入口**——同优先级的两个平台之间怎么分流，用户改不了。
                    改完立刻落库，不需要额外「保存」：这是个单值设置，加一步确认
                    只会让人以为没生效。
                  */}
                  <div className="mt-1.5 flex items-center gap-1.5">
                    <span className="text-xs text-muted-foreground">权重</span>
                    <input
                      type="number"
                      min={1}
                      max={100}
                      defaultValue={selectedPlatform.weight ?? 1}
                      disabled={savingWeight}
                      title="同优先级时按权重分流，1~100。优先级由左侧列表顺序决定。"
                      className="h-6 w-16 rounded border border-border bg-background px-1.5 text-xs"
                      onBlur={(e) => {
                        const next = Number(e.target.value);
                        if (Number.isFinite(next) && next !== selectedPlatform.weight) {
                          void saveWeight(selectedPlatform, next);
                        }
                      }}
                    />
                  </div>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button size="sm" variant="outline" onClick={onFetchRemoteModels} disabled={fetchingModels}>
                    <RefreshCw className={cn("h-3 w-3", fetchingModels && "animate-spin")} />
                    {fetchingModels ? "拉取中..." : "获取模型"}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => void compareUpstream(selectedPlatform.id)}
                    disabled={comparing}
                    title="和上游对比一遍：哪些是新增的、哪些上游已经下架。「获取模型」只会把远端的加进来，发现不了下架。"
                  >
                    <GitCompare className={cn("h-3 w-3", comparing && "animate-pulse")} />
                    {comparing ? "对比中..." : "对比上游"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onBatchTestModels(selectedPlatform.id)} disabled={batchTesting[selectedPlatform.id]}>
                    {batchTesting[selectedPlatform.id] ? <RefreshCw className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
                    {batchTesting[selectedPlatform.id] ? "检测中..." : "健康检测"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onEditPlatform(selectedPlatform)}>
                    <Edit className="h-3 w-3" /> 编辑
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      // 删平台会连 Key 和整张模型列表一起没，且不可撤销。
                      if (window.confirm(`删除供应商「${selectedPlatform.name}」？

它的 API Key 和 ${platformModels.length} 个模型会一并删除，无法撤销。`)) {
                        onDeletePlatform(selectedPlatform.id);
                      }
                    }}
                  >
                    <Trash2 className="h-3 w-3 text-destructive" /> 删除
                  </Button>
                </div>
              </CardContent>
            </Card>

              {diff && diff.platform_id === selectedPlatform.id && !diff.error && (
                <div className="mb-3 rounded-lg border border-border bg-muted/20 p-3">
                  <div className="mb-2 flex items-center justify-between">
                    <span className="text-xs font-medium">
                      与上游对比：本地 {diff.local_models.length} 个 · 上游{" "}
                      {diff.upstream_models.length} 个
                    </span>
                    <button
                      className="text-xs text-muted-foreground hover:text-foreground"
                      onClick={() => setDiff(null)}
                    >
                      收起
                    </button>
                  </div>
                  {diff.new_models.length === 0 && diff.removed_models.length === 0 ? (
                    <p className="text-xs text-muted-foreground">完全一致，无需改动。</p>
                  ) : (
                    <div className="space-y-2">
                      {diff.new_models.length > 0 && (
                        <div className="flex items-start gap-2">
                          <span className="shrink-0 rounded border border-success/50 bg-success/10 px-1.5 py-0.5 text-[10px] text-success">
                            上游新增 {diff.new_models.length}
                          </span>
                          <span className="min-w-0 flex-1 break-all text-xs text-muted-foreground">
                            {diff.new_models.join("、")}
                          </span>
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={applying}
                            onClick={() => void applyDiff(diff.new_models, [])}
                          >
                            添加
                          </Button>
                        </div>
                      )}
                      {diff.removed_models.length > 0 && (
                        <div className="flex items-start gap-2">
                          <span className="shrink-0 rounded border border-destructive/50 bg-destructive/10 px-1.5 py-0.5 text-[10px] text-destructive">
                            上游已下架 {diff.removed_models.length}
                          </span>
                          <span
                            className="min-w-0 flex-1 break-all text-xs text-muted-foreground"
                            title="本地还留着上游已经没有的模型。路由到它就是失败——但失败要等真发出请求才看得见。"
                          >
                            {diff.removed_models.join("、")}
                          </span>
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={applying}
                            onClick={() => void applyDiff([], diff.removed_models)}
                          >
                            移除
                          </Button>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}

            {/* Models List */}
            <Card className="flex-1 flex flex-col overflow-hidden">
              <div className="flex flex-wrap justify-between items-center gap-2 mb-3">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-semibold">模型列表</span>
                  {defaultModelKey ? (
                    <Badge variant="outline" className="gap-1 text-xs">
                      <Star className="h-3 w-3 fill-amber-400 text-amber-400" />
                      Agent 默认：{defaultModelKey.split(":").slice(1).join(":")}
                    </Badge>
                  ) : (
                    <span className="text-xs text-muted-foreground">未设 Agent 默认模型（点 ☆ 设置）</span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <div className="relative">
                    <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
                    <input
                      value={query}
                      onChange={(e) => setQuery(e.target.value)}
                      placeholder="搜索模型…"
                      className="h-7 w-40 rounded-md border border-border bg-background pl-7 pr-2 text-xs"
                    />
                  </div>
                  <select
                    value={capability}
                    onChange={(e) => setCapability(e.target.value as "" | keyof PlatformModel)}
                    className="h-7 rounded-md border border-border bg-background px-2 text-xs"
                  >
                    <option value="">全部能力</option>
                    <option value="has_tool_use">工具调用</option>
                    <option value="has_vision">视觉</option>
                    <option value="has_reasoning">推理</option>
                    <option value="has_coding">编程</option>
                    <option value="has_long_context">长上下文</option>
                    <option value="has_embedding">嵌入</option>
                  </select>
                  <Button size="sm" variant="outline" onClick={onAddModel}>
                    <Plus className="h-3 w-3" /> 自定义模型
                  </Button>
                </div>
              </div>

              <div className="flex-1 overflow-y-auto flex flex-col gap-2">
                {platformModels.length === 0 ? (
                  <div className="text-center text-muted-foreground py-10 text-xs">
                    暂无可用模型，请点击上方"获取模型"自动从服务商同步。
                  </div>
                ) : visibleModels.length === 0 ? (
                  <div className="text-center text-muted-foreground py-10 text-xs">
                    没有匹配的模型（共 {platformModels.length} 个）。
                  </div>
                ) : (
                  visibleModels.map((model) => {
                    const testState = modelTestingState[model.id] || "idle";
                    const route = routing[model.id];
                    const contested = (route?.rival_platforms.length ?? 0) > 0;
                    const winsRouting = route?.winner_platform_id === model.platform_id;
                    return (
                      <div
                        key={model.id}
                        className="flex flex-col gap-1 px-3 py-2 border-b border-border"
                      >
                        <div className="flex justify-between items-center gap-2">
                        <div className="flex items-center gap-2.5 min-w-0">
                          <Checkbox
                            checked={model.is_enabled}
                            onCheckedChange={() => onToggleModelEnabled(model)}
                          />
                          <span className={cn("text-sm font-medium truncate", !model.is_enabled && "opacity-60")}>
                            {model.model_name}
                          </span>
                          {/* 同名模型挂在多个平台上时，只写裸名字会由路由静默二选一。
                              这里直接说清有几个竞争者、当前会走谁。 */}
                          {contested && (
                            <Badge
                              variant={winsRouting ? "outline" : "warning"}
                              className="shrink-0 gap-1 whitespace-nowrap text-[11px]"
                              title={`「${model.model_name}」也存在于：${route?.rival_platforms.join("、")}。只写裸模型名时，路由按供应商顺序 + 权重决定走哪个。`}
                            >
                              {winsRouting ? "同名 · 当前走这里" : `同名 · 当前走别处`}
                            </Badge>
                          )}
                        </div>

                        <div className="flex items-center gap-3 shrink-0">
                          {/* Default-model star */}
                          <button
                            onClick={() => setAsDefaultModel(model)}
                            disabled={!model.is_enabled}
                            title={
                              !model.is_enabled
                                ? "请先启用该模型"
                                : defaultModelKey === modelKey(model)
                                ? "Agent 默认模型（点击取消）"
                                : "设为 Agent 默认模型（Codex/Claude 未单独绑定时使用）"
                            }
                            className={cn(
                              "p-0.5 inline-flex",
                              model.is_enabled ? "cursor-pointer" : "cursor-not-allowed opacity-30"
                            )}
                          >
                            <Star
                              className={cn(
                                "h-3.5 w-3.5",
                                defaultModelKey === modelKey(model)
                                  ? "fill-amber-400 text-amber-400"
                                  : "text-muted-foreground"
                              )}
                            />
                          </button>

                          {/* Capability Icons (read-only — auto-detected) */}
                          <div className="flex gap-0.5">
                            {([
                              { key: "has_vision" as keyof PlatformModel, icon: <Eye className="h-3 w-3" />, title: "视觉", color: "text-blue-400" },
                              { key: "has_audio" as keyof PlatformModel, icon: <Mic className="h-3 w-3" />, title: "音频", color: "text-purple-400" },
                              { key: "has_reasoning" as keyof PlatformModel, icon: <Brain className="h-3 w-3" />, title: "推理", color: "text-amber-400" },
                              { key: "has_coding" as keyof PlatformModel, icon: <Code className="h-3 w-3" />, title: "编程", color: "text-green-400" },
                              { key: "has_long_context" as keyof PlatformModel, icon: <Maximize2 className="h-3 w-3" />, title: "长上下文", color: "text-cyan-400" },
                              { key: "has_tool_use" as keyof PlatformModel, icon: <Wrench className="h-3 w-3" />, title: "工具调用", color: "text-orange-400" },
                              { key: "has_embedding" as keyof PlatformModel, icon: <Layers className="h-3 w-3" />, title: "嵌入", color: "text-pink-400" },
                              { key: "has_speedy" as keyof PlatformModel, icon: <Zap className="h-3 w-3" />, title: "快速", color: "text-yellow-400" },
                            ]).map(({ key, icon, title, color }) => {
                              const isActive = model[key] as boolean;
                              return (
                                <span
                                  key={key}
                                  title={`${title}${isActive ? " ✓" : " —"} (自动检测)`}
                                  className={cn(
                                    "p-0.5 inline-flex",
                                    isActive ? `opacity-100 ${color}` : "opacity-20 text-muted-foreground"
                                  )}
                                >
                                  {icon}
                                </span>
                              );
                            })}
                          </div>

                          {/* Test Status */}
                          <div className="flex items-center gap-1.5">
                            <div
                              title={
                                testState === "success" ? "可用" :
                                testState === "auth_error" ? "认证失败" :
                                testState === "no_api_key" ? "无 API Key" :
                                testState === "rate_limited" ? "限流中" :
                                testState === "unreachable" ? "不可达" :
                                testState === "error" ? "错误" :
                                testState === "testing" ? "测试中" : "未测试"
                              }
                              className={cn(
                                "w-2 h-2 rounded-full",
                                testState === "success" && "bg-success shadow-[0_0_8px_#10b981]",
                                testState === "auth_error" && "bg-destructive shadow-[0_0_8px_#ef4444]",
                                testState === "no_api_key" && "bg-destructive/70 shadow-[0_0_8px_#f87171]",
                                testState === "rate_limited" && "bg-warning shadow-[0_0_8px_#f59e0b]",
                                testState === "error" && "bg-destructive shadow-[0_0_8px_#ef4444]",
                                testState === "unreachable" && "bg-destructive shadow-[0_0_8px_#ef4444]",
                                testState === "testing" && "bg-warning animate-pulse",
                                testState === "idle" && "bg-muted-foreground"
                              )}
                            />
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => onTestModel(model.id)}
                              disabled={testState === "testing"}
                              className="text-xs px-2 py-0.5"
                            >
                              {testState === "testing" ? "测试中..." : "⚡ 测试"}
                            </Button>
                          </div>

                          <button
                            onClick={() => {
                              if (window.confirm(`从「${selectedPlatform.name}」删除模型「${model.model_name}」？`)) {
                                onDeleteModel(model.id);
                              }
                            }}
                            className="bg-transparent border-none text-destructive cursor-pointer"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        </div>
                        </div>

                        {/* 最近一次**真实**失败。这条早就在 request_logs 里了，
                            以前模型中心一个字不显示——界面只有一个绿点，而那个绿点
                            说的是「上次手动测过」，不是「现在能用」。 */}
                        {route?.last_error && (
                          <div className="flex items-start gap-1.5 pl-7 text-[11px] leading-4 text-destructive/80">
                            <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
                            <span className="min-w-0">
                              最近一次失败（{route.last_error_at}）：
                              <span className="break-all">{route.last_error.slice(0, 180)}</span>
                            </span>
                          </div>
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            </Card>
          </>
        ) : (
          <div className="flex flex-1 justify-center items-center text-muted-foreground text-sm">
            请在左侧选择一个模型平台配置以查看详情
          </div>
        )}
      </div>
    </div>
  );
}

// ── System Sub-Tab ───────────────────────────────────

/** 悬浮状态坞开关（默认关、不开机自启，即时生效）。自管状态，不走保存流程。 */
