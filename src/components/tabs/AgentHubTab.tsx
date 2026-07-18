import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Bot,
  CheckCircle2,
  Download,
  Loader2,
  Play,
  RefreshCw,
  Sparkles,
  Terminal,
  Wrench,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { AGENT_NAMES } from "@/lib/constants";
import { agentApi, agentBindingApi, runtimeApi, grokAuthApi, type GrokModel } from "@/lib/tauri-api";
import { getRuntimeAgentId, isAcpAgent } from "@/lib/agentRegistry";
import { cn } from "@/lib/utils";
import { AgentInstallManager } from "@/components/AgentInstallManager";
import { toast } from "@/components/ui/sonner";
import type { AgentAccount, AgentUpdateInfo, DetectedAgent, PlatformModel, RuntimeAgentCatalogEntry } from "@/types";
import type { AgentPlatformBinding } from "@/lib/tauri-api";

interface AgentHubTabProps {
  detectedAgents: DetectedAgent[];
  activeAgent: string;
  accounts: AgentAccount[];
  activeModels: PlatformModel[];
  onSwitchAgent: (name: string) => void;
  onAddAccount: () => void;
  onEditAccount: (acc: AgentAccount) => void;
  onDeleteAccount: (id: string) => void;
  onSwitchAccount: (id: string) => void;
  onStartWork?: (name: string) => void;
  /** Refreshes the App-level detection list — the ONE source of truth that the
   * workspace also reads. Every install/update/refresh here must go through
   * this, or the workspace keeps showing 「未检测到」 until an app restart. */
  onRefreshAgents: () => Promise<void>;
}

const BUILTIN_MODELS: Record<string, string[]> = {
  "OpenCode": ["opencode/free", "opencode/auto"],
  "Gemini CLI": ["gemini-cli/default"],
  "Codex": ["gpt-5-codex"],
  "Claude Code": ["sonnet", "opus", "haiku"],
};

const FEATURED_AGENTS = ["Claude Code", "Codex", "Gemini CLI", "OpenCode"];
const DEFAULT_BINDING_VALUE = "__agent_default__";

function getBindingValue(binding?: AgentPlatformBinding) {
  if (!binding || binding.binding_kind === "default") return DEFAULT_BINDING_VALUE;
  if (binding.binding_kind === "builtin" && binding.builtin_model) {
    return `builtin::${binding.builtin_model}`;
  }
  if (binding.binding_kind === "omnix" && binding.model_name) {
    return `omnix::${binding.platform_id}::${binding.model_name}`;
  }
  return DEFAULT_BINDING_VALUE;
}

function getBindingLabel(agentName: string, binding?: AgentPlatformBinding) {
  if (!binding || binding.binding_kind === "default") {
    return "跟随 Agent 默认";
  }
  if (binding.binding_kind === "builtin") {
    return binding.builtin_model || BUILTIN_MODELS[agentName]?.[0] || "Agent 官方/自带";
  }
  return binding.model_name || "OMNIX 模型";
}

export function AgentHubTab({
  detectedAgents,
  activeAgent,
  accounts,
  activeModels,
  onSwitchAgent,
  onAddAccount,
  onEditAccount,
  onDeleteAccount,
  onSwitchAccount,
  onStartWork,
  onRefreshAgents,
}: AgentHubTabProps) {
  // No local copy of the detection list — `detectedAgents` (App-level) is the
  // single source of truth shared with the workspace. A fork here once made the
  // workspace claim an installed agent was 「未检测到」 until restart.
  const [bindings, setBindings] = useState<AgentPlatformBinding[]>([]);
  const [selectedAgent, setSelectedAgent] = useState(activeAgent || FEATURED_AGENTS[0]);
  const [busyAgent, setBusyAgent] = useState<string | null>(null);
  const [runtimeCatalog, setRuntimeCatalog] = useState<RuntimeAgentCatalogEntry[]>([]);
  const [updates, setUpdates] = useState<Record<string, AgentUpdateInfo>>({});
  const [refreshing, setRefreshing] = useState(false);
  const [showInstallManager, setShowInstallManager] = useState(false);
  // Free-form custom model (per agent). For ACP agents it maps to the ACP model
  // preference (session/set_config_option); for others to a builtin binding.
  const [customModel, setCustomModel] = useState("");
  const [savingCustom, setSavingCustom] = useState(false);
  // Grok advertises models only per-session; probe the account (zero-token
  // handshake) so its picker isn't empty before the first session.
  const [grokModels, setGrokModels] = useState<GrokModel[]>([]);
  const [grokModelsBusy, setGrokModelsBusy] = useState(false);

  useEffect(() => {
    agentBindingApi.getAll().then(setBindings).catch(() => setBindings([]));
  }, []);

  /** 「刷新」= 重新检测 + 检查更新 + 运行适配状态，一个动作看全现状。 */
  const refreshAll = useCallback(async () => {
    setRefreshing(true);
    try {
      await onRefreshAgents();
      const [catalog, updateList] = await Promise.all([
        runtimeApi.getAgentCatalog().catch(() => [] as RuntimeAgentCatalogEntry[]),
        agentApi.checkUpdates().catch(() => [] as AgentUpdateInfo[]),
      ]);
      setRuntimeCatalog(catalog);
      setUpdates(Object.fromEntries(updateList.map((info) => [info.name, info])));
    } catch (error) {
      toast.error(`刷新失败：${error}`);
    } finally {
      setRefreshing(false);
    }
  }, [onRefreshAgents]);

  useEffect(() => {
    void refreshAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const agents = useMemo(() => {
    const names = [...FEATURED_AGENTS, ...AGENT_NAMES.filter((name) => !FEATURED_AGENTS.includes(name))];
    return names.map((name) => {
      const detected = detectedAgents.find((agent) => agent.name === name);
      const binding = bindings.find((item) => item.agent_name === name);
      const runtime = runtimeCatalog.find((item) => item.name === name);
      const agentAccounts = accounts.filter((account) => account.agent_name === name || account.agent_name === name.toLowerCase());
      return {
        name,
        detected,
        binding,
        accounts: agentAccounts,
        installed: detected?.status === "installed",
        currentModel: getBindingLabel(name, binding),
        runtime,
      };
    });
  }, [accounts, bindings, detectedAgents, runtimeCatalog]);

  const selected = agents.find((agent) => agent.name === selectedAgent) ?? agents[0];
  const selectedIsAcp = selected ? isAcpAgent(selected.name) : false;

  // Load the current custom model for the selected agent (ACP: preference key;
  // others: the builtin binding value).
  useEffect(() => {
    if (!selected) { setCustomModel(""); return; }
    if (isAcpAgent(selected.name)) {
      const wireId = getRuntimeAgentId(selected.name);
      if (wireId) {
        runtimeApi.getAgentModelPreference(wireId).then(setCustomModel).catch(() => setCustomModel(""));
        return;
      }
    }
    setCustomModel(selected.binding?.binding_kind === "builtin" ? selected.binding.builtin_model ?? "" : "");
  }, [selected]);

  // Selecting Grok (installed) auto-probes its account models for the picker.
  useEffect(() => {
    if (selected?.name !== "Grok Build" || !selected.installed) {
      setGrokModels([]);
      return;
    }
    setGrokModelsBusy(true);
    grokAuthApi
      .availableModels()
      .then(setGrokModels)
      .catch(() => setGrokModels([]))
      .finally(() => setGrokModelsBusy(false));
  }, [selected?.name, selected?.installed]);

  const saveCustomModel = async () => {
    if (!selected) return;
    const model = customModel.trim();
    setSavingCustom(true);
    try {
      if (selectedIsAcp) {
        const wireId = getRuntimeAgentId(selected.name);
        if (!wireId) throw new Error("无法解析 Agent 运行时 ID");
        await runtimeApi.setAgentModelPreference(wireId, model);
        toast.success(model ? `已为 ${selected.name} 设置自定义模型：${model}` : "已清除自定义模型（用 Agent 默认）");
      } else if (model) {
        await agentBindingApi.setBuiltin(selected.name, model);
        setBindings(await agentBindingApi.getAll());
        toast.success(`已为 ${selected.name} 绑定自定义模型：${model}`);
      } else {
        await agentBindingApi.remove(selected.name);
        setBindings(await agentBindingApi.getAll());
        toast.success("已清除模型绑定（用 Agent 默认）");
      }
    } catch (error) {
      toast.error("保存自定义模型失败", { description: String(error) });
    } finally {
      setSavingCustom(false);
    }
  };

  const savedModelOptions = activeModels.map((model) => ({
    value: `omnix::${model.platform_id}::${model.model_name}`,
    label: `${model.model_name} · ${model.platform_id}`,
    platformId: model.platform_id,
    modelName: model.model_name,
  }));
  const builtinModelOptions = BUILTIN_MODELS[selected?.name ?? ""] ?? [];

  const runAgentAction = async (agentName: string, action: "install" | "update") => {
    setBusyAgent(agentName);
    try {
      if (action === "install") {
        toast.info(`正在安装 ${agentName}…`);
        await agentApi.install(agentName);
        toast.success(`${agentName} 安装完成`);
      } else {
        await agentApi.update(agentName);
        toast.success(`${agentName} 已更新`);
      }
      // Post-mutation state flows through the shared refresh so the workspace
      // sees the new reality immediately (no restart, no fork).
      await refreshAll();
    } catch (error) {
      toast.error(`${agentName} 操作失败：${error}`);
    } finally {
      setBusyAgent(null);
    }
  };

  const bindSavedModel = async (value: string) => {
    if (!selected) return;
    try {
      if (value === DEFAULT_BINDING_VALUE) {
        await agentBindingApi.remove(selected.name);
        setBindings(await agentBindingApi.getAll());
        toast.success("已切回 Agent 默认模型");
        return;
      }

      if (value.startsWith("builtin::")) {
        const builtinModel = value.slice("builtin::".length);
        await agentBindingApi.setBuiltin(selected.name, builtinModel);
        setBindings(await agentBindingApi.getAll());
        toast.success("已绑定 Agent 官方/自带模型");
        return;
      }

      const option = savedModelOptions.find((item) => item.value === value);
      if (!option) return;
      await agentBindingApi.set(selected.name, option.platformId, option.modelName, "omnix");
      setBindings(await agentBindingApi.getAll());
      toast.success("模型绑定已保存");
    } catch (error) {
      toast.error("模型绑定失败", { description: String(error) });
    }
  };

  return (
    <div className="flex h-full flex-1 overflow-hidden bg-background">
      <section className="min-w-0 flex-1 overflow-y-auto p-6">
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 text-lg font-semibold">
              <Bot className="h-5 w-5 text-primary" />
              本地智能体
            </div>
            <p className="mt-1 max-w-3xl text-sm leading-6 text-muted-foreground">
              每张卡就是一个 Agent 的现状：未安装的点「安装」，有新版的点「更新」，装好的点「开始工作」。
              状态由「刷新」一次看全（检测本地 CLI + 检查新版本）。
            </p>
          </div>
          <div className="flex gap-2">
            <Button variant="outline" onClick={() => void refreshAll()} disabled={refreshing || !!busyAgent}>
              {refreshing ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
              刷新
            </Button>
            <Button
              variant="outline"
              onClick={() => setShowInstallManager((v) => !v)}
              title="盘点系统里每个 Agent 的所有安装副本（PATH / npm 全局 / OMNIX 托管），清理重复、统一到托管目录"
            >
              <Wrench className="h-4 w-4" />
              安装位置{showInstallManager ? " ▲" : ""}
            </Button>
          </div>
        </div>

        {/* 安装位置管理：进阶盘点，默认收起——和上面的「刷新」是两回事：
            刷新回答「现在哪些能用」，这里回答「系统里都装在了哪、要不要清理」。 */}
        {showInstallManager && (
          <div className="mb-6 animate-sheet-in">
            <AgentInstallManager />
          </div>
        )}

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {agents.map((agent) => (
            <div
              key={agent.name}
              role="button"
              data-press="soft"
              tabIndex={0}
              aria-label={`选择 ${agent.name}`}
              className={cn(
                "min-h-56 cursor-pointer rounded-md border bg-card/40 p-5 text-left outline-none transition-colors hover:bg-muted/20 focus-visible:ring-2 focus-visible:ring-primary/40",
                selectedAgent === agent.name && "border-primary/40 bg-primary/10"
              )}
              onClick={() => setSelectedAgent(agent.name)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  setSelectedAgent(agent.name);
                }
              }}
            >
              <div className="flex items-start justify-between gap-3">
                <AgentMark name={agent.name} active={activeAgent === agent.name} />
                <Badge variant={agent.runtime?.runtime_status === "supported" && agent.installed ? "success" : "secondary"}>
                  {agent.runtime?.runtime_status === "pending" ? "待适配" : agent.installed ? "已检测" : "未安装"}
                </Badge>
              </div>
              <div className="mt-5 flex items-center gap-2 text-xl font-semibold">
                {agent.name}
                {updates[agent.name]?.has_update && (
                  <Badge variant="warning" className="text-[10px]">可更新</Badge>
                )}
              </div>
              <div className="mt-2 text-sm text-muted-foreground">
                {agent.installed
                  ? `${agent.runtime?.installation_source === "managed" ? "OMNIX 托管" : "系统安装"} · ${agent.detected?.version || "版本未知"}`
                  : "需要安装对应 CLI 后才能启动"}
              </div>
              {updates[agent.name]?.has_update && (
                <div className="mt-1 text-xs text-warning">
                  有新版本 {updates[agent.name].latest} （当前 {updates[agent.name].current}）
                </div>
              )}
              <div className="mt-4 rounded-md border border-border bg-background/50 p-3">
                <div className="text-xs text-muted-foreground">当前模型</div>
                <div className="mt-1 truncate text-sm font-medium">{agent.currentModel}</div>
              </div>
              {/* 动作按状态收敛：任何时刻只出现「当下该做的那一件事」。
                  卡片本身即入口——点卡片在右侧看详情/绑模型/配账号。 */}
              <div className="mt-4 flex flex-wrap gap-2">
                {!agent.installed ? (
                  <Button
                    size="sm"
                    type="button"
                    disabled={busyAgent === agent.name}
                    onClick={(event) => {
                      event.stopPropagation();
                      void runAgentAction(agent.name, "install");
                    }}
                  >
                    {busyAgent === agent.name ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Download className="h-3.5 w-3.5" />
                    )}
                    安装
                  </Button>
                ) : (
                  <>
                    <Button
                      size="sm"
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onSwitchAgent(agent.name);
                        onStartWork?.(agent.name);
                      }}
                      disabled={agent.runtime?.runtime_status !== "supported"}
                    >
                      <Play className="h-3.5 w-3.5" />
                      开始工作
                    </Button>
                    {updates[agent.name]?.has_update && (
                      <Button
                        size="sm"
                        type="button"
                        variant="outline"
                        disabled={busyAgent === agent.name}
                        onClick={(event) => {
                          event.stopPropagation();
                          void runAgentAction(agent.name, "update");
                        }}
                        title={`更新到 ${updates[agent.name].latest}`}
                      >
                        {busyAgent === agent.name ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <Download className="h-3.5 w-3.5" />
                        )}
                        更新
                      </Button>
                    )}
                  </>
                )}
              </div>
            </div>
          ))}
        </div>
      </section>

      {selected && (
        <aside className="hidden w-[420px] shrink-0 border-l border-border bg-card/30 lg:flex lg:flex-col">
          <div className="flex items-start justify-between gap-3 border-b border-border p-5">
            <div>
              <div className="flex items-center gap-2 text-base font-semibold">
                <Terminal className="h-4 w-4 text-primary" />
                {selected.name}
              </div>
              <p className="mt-1 text-xs text-muted-foreground">检测、安装、更新和模型绑定</p>
            </div>
            <button className="rounded p-1 text-muted-foreground hover:bg-muted/20" onClick={() => setSelectedAgent(activeAgent)}>
              <X className="h-4 w-4" />
            </button>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-5">
            <div className="space-y-4">
              <InfoRow label="状态" value={selected.installed ? "已检测到本地 CLI" : "未检测到本地 CLI"} ok={selected.installed} />
              <InfoRow label="版本" value={selected.detected?.version || "未知"} />
              <InfoRow label="来源" value={selected.runtime?.installation_source === "managed" ? "OMNIX 托管安装" : selected.installed ? "系统安装" : "未安装"} />
              <InfoRow label="运行适配" value={selected.runtime?.runtime_status === "supported" ? "结构化协议已接入" : "待适配，不会显示为可运行"} ok={selected.runtime?.runtime_status === "supported"} />
              <InfoRow label="路径" value={selected.detected?.path || "未找到"} />

              <div className="rounded-md border border-border bg-background/50 p-4">
                <div className="mb-3 text-sm font-semibold">模型绑定</div>
                <select
                  className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
                  value={getBindingValue(selected.binding)}
                  onChange={(event) => bindSavedModel(event.target.value)}
                >
                  <option value={DEFAULT_BINDING_VALUE}>Agent 默认</option>
                  {builtinModelOptions.map((model) => (
                    <option key={model} value={`builtin::${model}`}>{model}（Agent 官方/自带）</option>
                  ))}
                  {savedModelOptions.map((model) => (
                    <option key={model.value} value={model.value}>{model.label}（OMNIX）</option>
                  ))}
                </select>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                  Agent 默认沿用 CLI 自己的配置；Agent 官方模型由 CLI 直连；OMNIX 模型使用模型中心中已启用的供应商。运行时仍会检查协议兼容性。
                </p>

                {/* Grok：账号实际可用的模型（零 token 探测）——点一下即填入。 */}
                {selected.name === "Grok Build" && (
                  <div className="mt-3 border-t border-border pt-3">
                    <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium">
                      Grok 账号可用模型
                      {grokModelsBusy && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />}
                    </div>
                    {grokModels.length > 0 ? (
                      <div className="flex flex-wrap gap-1.5">
                        {grokModels.map((m) => (
                          <button
                            key={m.id}
                            className={cn(
                              "rounded border px-2 py-0.5 text-xs hover:bg-muted/40",
                              customModel === m.id ? "border-primary bg-primary/10 text-primary" : "border-border",
                            )}
                            onClick={() => setCustomModel(m.id)}
                            title={m.id}
                          >
                            {m.name}
                          </button>
                        ))}
                      </div>
                    ) : (
                      <p className="text-xs text-muted-foreground">
                        {grokModelsBusy ? "正在读取账号模型…" : "未读取到——请确认已在认证中心登录 Grok，且 CLI 已安装。"}
                      </p>
                    )}
                    <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
                      选一个再点下方「保存」即绑定；Grok 不支持会话中切换，下次会话经 <code>-m</code> 生效。
                    </p>
                  </div>
                )}

                {/* Free-form custom model — beyond the preset dropdown above. */}
                <div className="mt-3 border-t border-border pt-3">
                  <div className="mb-1.5 text-xs font-medium">自定义模型</div>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={customModel}
                      onChange={(event) => setCustomModel(event.target.value)}
                      placeholder={selectedIsAcp ? "如 gemini-2.5-pro / glm-4.6，直接填写" : "如 claude-opus-4，作为 --model 传入"}
                      className="h-9 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-sm"
                      onKeyDown={(event) => { if (event.key === "Enter") void saveCustomModel(); }}
                    />
                    <Button size="sm" variant="outline" disabled={savingCustom} onClick={() => void saveCustomModel()}>
                      保存
                    </Button>
                  </div>
                  <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
                    {selected.name === "Grok Build"
                      ? "Grok Build 暂不支持从这里下发模型（其 CLI 不接受标准 ACP 配置协议，实测返回 Method not found）——会话运行在你 Grok 账号的默认模型上；模型选择通道接入中。"
                      : selectedIsAcp
                        ? "ACP Agent（Gemini/Qwen/OpenCode/Copilot）：下次会话经 set_config_option 下发，可填 Agent 未在列表里声明的模型。留空＝用 Agent 默认。"
                        : "Claude Code / Codex：作为自带模型（--model）绑定。留空并保存＝清除绑定，回到 Agent 默认。"}
                  </p>
                </div>
              </div>

              {/* 情境化动作：与宫格卡片同一逻辑（检测由顶部「刷新」统一负责）。 */}
              <div className="flex gap-2">
                {!selected.installed ? (
                  <Button className="flex-1" onClick={() => runAgentAction(selected.name, "install")} disabled={busyAgent === selected.name}>
                    {busyAgent === selected.name ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
                    安装
                  </Button>
                ) : (
                  <>
                    <Button
                      className="flex-1"
                      onClick={() => {
                        onSwitchAgent(selected.name);
                        onStartWork?.(selected.name);
                      }}
                      disabled={selected.runtime?.runtime_status !== "supported"}
                    >
                      <Play className="h-4 w-4" />
                      开始工作
                    </Button>
                    {updates[selected.name]?.has_update && (
                      <Button variant="outline" onClick={() => runAgentAction(selected.name, "update")} disabled={busyAgent === selected.name}>
                        {busyAgent === selected.name ? <Loader2 className="h-4 w-4 animate-spin" /> : <Wrench className="h-4 w-4" />}
                        更新到 {updates[selected.name].latest}
                      </Button>
                    )}
                  </>
                )}
              </div>

              <div className="rounded-md border border-border bg-background/50 p-4">
                <div className="mb-3 text-sm font-semibold">账号凭据</div>
                <div className="space-y-2">
                  {selected.accounts.length === 0 ? (
                    <div className="text-sm text-muted-foreground">还没有为该 Agent 配置账号。</div>
                  ) : selected.accounts.map((account) => (
                    <div key={account.id} className="flex items-center justify-between gap-2 rounded-md border border-border px-3 py-2">
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium">{account.account_name}</div>
                        <div className="truncate text-xs text-muted-foreground">{account.target_model}</div>
                      </div>
                      <Button size="sm" variant="ghost" onClick={() => onSwitchAccount(account.id)}>
                        {account.is_active ? "当前" : "启用"}
                      </Button>
                    </div>
                  ))}
                </div>
                <div className="mt-3 flex gap-2">
                  <Button size="sm" variant="outline" onClick={onAddAccount}>新增账号</Button>
                  {selected.accounts[0] && <Button size="sm" variant="ghost" onClick={() => onEditAccount(selected.accounts[0])}>编辑</Button>}
                  {selected.accounts[0] && <Button size="sm" variant="ghost" className="text-destructive" onClick={() => onDeleteAccount(selected.accounts[0].id)}>删除</Button>}
                </div>
              </div>
            </div>
          </div>
        </aside>
      )}
    </div>
  );
}

function AgentMark({ name, active }: { name: string; active: boolean }) {
  return (
    <div className={cn("flex h-16 w-16 items-center justify-center rounded-md border", active ? "border-primary/40 bg-primary/15 text-primary" : "border-border bg-background/60")}>
      {name === "Claude Code" ? <Sparkles className="h-8 w-8 text-orange-500" /> : <Bot className="h-8 w-8" />}
    </div>
  );
}

function InfoRow({ label, value, ok }: { label: string; value: string; ok?: boolean }) {
  return (
    <div className="rounded-md border border-border bg-background/50 p-3">
      <div className="mb-1 flex items-center gap-2 text-xs text-muted-foreground">
        {ok && <CheckCircle2 className="h-3.5 w-3.5 text-success" />}
        {label}
      </div>
      <div className="break-all text-sm">{value}</div>
    </div>
  );
}
