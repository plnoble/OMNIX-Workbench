import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Bot,
  CheckCircle2,
  Download,
  Loader2,
  Play,
  RefreshCw,
  Settings2,
  Sparkles,
  Terminal,
  Wrench,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { AGENT_NAMES } from "@/lib/constants";
import { agentApi, agentBindingApi, runtimeApi } from "@/lib/tauri-api";
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
}: AgentHubTabProps) {
  const [localAgents, setLocalAgents] = useState<DetectedAgent[]>(detectedAgents);
  const [bindings, setBindings] = useState<AgentPlatformBinding[]>([]);
  const [selectedAgent, setSelectedAgent] = useState(activeAgent || FEATURED_AGENTS[0]);
  const [busyAgent, setBusyAgent] = useState<string | null>(null);
  const [runtimeCatalog, setRuntimeCatalog] = useState<RuntimeAgentCatalogEntry[]>([]);
  const [updates, setUpdates] = useState<Record<string, AgentUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  // Free-form custom model (per agent). For ACP agents it maps to the ACP model
  // preference (session/set_config_option); for others to a builtin binding.
  const [customModel, setCustomModel] = useState("");
  const [savingCustom, setSavingCustom] = useState(false);

  useEffect(() => {
    setLocalAgents(detectedAgents);
  }, [detectedAgents]);

  useEffect(() => {
    agentBindingApi.getAll().then(setBindings).catch(() => setBindings([]));
    runtimeApi.getAgentCatalog().then(setRuntimeCatalog).catch(() => setRuntimeCatalog([]));
  }, []);

  // Check installed agents against npm latest so we can flag available updates.
  const checkUpdates = useCallback(async () => {
    setCheckingUpdates(true);
    try {
      const list = await agentApi.checkUpdates();
      setUpdates(Object.fromEntries(list.map((info) => [info.name, info])));
    } catch (error) {
      toast.error(`检查更新失败：${error}`);
    } finally {
      setCheckingUpdates(false);
    }
  }, []);

  useEffect(() => {
    void checkUpdates();
  }, [checkUpdates, localAgents.length]);

  const agents = useMemo(() => {
    const names = [...FEATURED_AGENTS, ...AGENT_NAMES.filter((name) => !FEATURED_AGENTS.includes(name))];
    return names.map((name) => {
      const detected = localAgents.find((agent) => agent.name === name);
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
  }, [accounts, bindings, localAgents, runtimeCatalog]);

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

  const runAgentAction = async (agentName: string, action: "detect" | "install" | "update") => {
    setBusyAgent(agentName);
    try {
      if (action === "detect") {
        const list = await agentApi.detectInstalled();
        setLocalAgents(list);
        setRuntimeCatalog(await runtimeApi.getAgentCatalog());
        toast.success("检测完成");
      } else if (action === "install") {
        await agentApi.install(agentName);
        const list = await agentApi.detectInstalled();
        setLocalAgents(list);
        setRuntimeCatalog(await runtimeApi.getAgentCatalog());
        toast.success(`${agentName} 安装完成`);
      } else {
        await agentApi.update(agentName);
        const list = await agentApi.detectInstalled();
        setLocalAgents(list);
        setRuntimeCatalog(await runtimeApi.getAgentCatalog());
        void checkUpdates();
        toast.success(`${agentName} 已更新`);
      }
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
              检测、安装、更新 Claude Code、Codex、Gemini CLI、OpenCode 等 Agent，并为每个 Agent 绑定模型。
            </p>
          </div>
          <Button variant="outline" onClick={() => runAgentAction(selected?.name ?? "Claude Code", "detect")} disabled={!!busyAgent}>
            {busyAgent ? <Loader2 className="h-4 w-4 animate-spin" /> : <RefreshCw className="h-4 w-4" />}
            重新检测
          </Button>
          <Button variant="outline" onClick={() => void checkUpdates()} disabled={checkingUpdates} title="检查各 Agent CLI 是否有新版本">
            {checkingUpdates ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
            检查更新
          </Button>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {agents.map((agent) => (
            <div
              key={agent.name}
              role="button"
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
              <div className="mt-4 flex flex-wrap gap-2">
                <Button
                  size="sm"
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    onSwitchAgent(agent.name);
                    onStartWork?.(agent.name);
                  }}
                  disabled={!agent.installed || agent.runtime?.runtime_status !== "supported"}
                >
                  <Play className="h-3.5 w-3.5" />
                  开始工作
                </Button>
                {agent.installed && updates[agent.name]?.has_update && (
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
                <Button
                  size="sm"
                  type="button"
                  variant="outline"
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedAgent(agent.name);
                  }}
                >
                  <Settings2 className="h-3.5 w-3.5" />
                  详情
                </Button>
              </div>
            </div>
          ))}
        </div>

        {/* R3 统一安装：扫描全部安装副本 / 删多余 / 收进托管目录 */}
        <div className="mt-6">
          <AgentInstallManager />
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
                    {selectedIsAcp
                      ? "ACP Agent（Gemini/Qwen/OpenCode/Copilot）：下次会话经 set_config_option 下发，可填 Agent 未在列表里声明的模型。留空＝用 Agent 默认。"
                      : "Claude Code / Codex：作为自带模型（--model）绑定。留空并保存＝清除绑定，回到 Agent 默认。"}
                  </p>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <Button variant="outline" onClick={() => runAgentAction(selected.name, "detect")} disabled={busyAgent === selected.name}>
                  <RefreshCw className={cn("h-4 w-4", busyAgent === selected.name && "animate-spin")} />
                  检测
                </Button>
                <Button variant="outline" onClick={() => runAgentAction(selected.name, "install")} disabled={busyAgent === selected.name || selected.installed}>
                  <Download className="h-4 w-4" />
                  安装
                </Button>
                <Button variant="outline" onClick={() => runAgentAction(selected.name, "update")} disabled={busyAgent === selected.name || !selected.installed}>
                  <Wrench className="h-4 w-4" />
                  更新
                </Button>
                <Button
                  onClick={() => {
                    onSwitchAgent(selected.name);
                    onStartWork?.(selected.name);
                  }}
                  disabled={!selected.installed || selected.runtime?.runtime_status !== "supported"}
                >
                  <Play className="h-4 w-4" />
                  开始
                </Button>
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
