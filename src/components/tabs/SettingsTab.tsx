import { useState, useEffect } from "react";

/**
 * SettingsTab — 应用设置
 *
 * Sub-tabs: Platform (Model Hub), System, MCP Servers, Backup
 */

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import {
  Plus, Edit, Trash2, RefreshCw, Eye, Brain, Mic, Code,
  Maximize2, Wrench, Layers, Zap, Activity, Star,
  Save, Plug, Settings, MousePointerClick,
  Languages, ArrowRightLeft, Search, Server, Database, Download, Upload,
  ExternalLink, Store, Key, FileText,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { modelApi, settingsApi, mcpSyncApi, type AgentMcpState } from "@/lib/tauri-api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BUILTIN_LANGUAGES } from "@/lib/translate-constants";
import type { ModelPlatform, PlatformModel, AgentAccount, ModelTestState, SettingsSubTab, SelectionHistoryEntry, SearchProvider, WebSearchResult, McpServer, BackupTableInfo, ImportResult } from "@/types";

export interface SettingsTabProps {
  settingsSubTab: SettingsSubTab;
  setSettingsSubTab: (tab: SettingsSubTab) => void;

  // Platform sub-tab
  platforms: ModelPlatform[];
  selectedPlatformId: string;
  platformModels: PlatformModel[];
  modelTestingState: Record<string, ModelTestState>;
  fetchingModels: boolean;
  onSelectPlatform: (id: string) => void;
  onTogglePlatform: (plat: ModelPlatform) => void;
  onAddPlatform: () => void;
  onEditPlatform: (plat: ModelPlatform) => void;
  onDeletePlatform: (id: string) => void;
  onFetchRemoteModels: () => void;
  onAddModel: () => void;
  onToggleModelEnabled: (model: PlatformModel) => void;
  // onToggleCapability removed — capabilities are now auto-detected
  onTestModel: (id: string) => Promise<import("@/types").HealthCheckDetail>;
  onDeleteModel: (id: string) => void;
  batchTesting: Record<string, boolean>;
  onBatchTestModels: (platformId: string) => void;

  // System sub-tab
  accounts: AgentAccount[];
  onAddAccount: () => void;
  onEditAccount: (acc: AgentAccount) => void;
  onDeleteAccount: (id: string) => void;
  onSwitchAccount: (id: string) => void;

  // Settings form
  targetModel: string;
  gpuAcceleration: boolean;
  idleTimeout: string;
  autoStart: boolean;
  startToTray: boolean;
  useWsl: boolean;
  wslDistro: string;
  setTargetModel: (v: string) => void;
  setGpuAcceleration: (v: boolean) => void;
  setIdleTimeout: (v: string) => void;
  setAutoStart: (v: boolean) => void;
  setStartToTray: (v: boolean) => void;
  setUseWsl: (v: boolean) => void;
  setWslDistro: (v: string) => void;
  onSaveSettings: () => Promise<void>;

  // Selection Assistant
  selectionCaptureMode: string;
  selectionShowOnCapture: boolean;
  selectionPreserveClipboard: boolean;
  isSelectionCapturing: boolean;
  lastSelectionCapture: string | null;
  selectionCaptureError: string | null;
  selectionHistory: SelectionHistoryEntry[];
  onSetSelectionCaptureMode: (v: string) => void;
  onSetSelectionShowOnCapture: (v: boolean) => void;
  onSetSelectionPreserveClipboard: (v: boolean) => void;
  onTestSelectionCapture: () => Promise<string | null>;
  onSaveSelectionSettings: (updates: Record<string, unknown>) => Promise<void>;
  onLoadSelectionHistory: () => Promise<void>;
  onDeleteSelectionHistoryItem: (id: string) => Promise<void>;
  onClearSelectionHistory: () => Promise<void>;

  // Translation
  translatePreferredLang: string;
  translateAlterLang: string;
  translateModel: string;
  translateAutoDetect: boolean;
  translateCustomPrompt: string;
  onSetTranslatePreferredLang: (v: string) => void;
  onSetTranslateAlterLang: (v: string) => void;
  onSetTranslateModel: (v: string) => void;
  onSetTranslateAutoDetect: (v: boolean) => void;
  onSetTranslateCustomPrompt: (v: string) => void;
  onSaveTranslationSettings: (updates: Record<string, unknown>) => Promise<void>;

  // Theme
  themeMode: "dark" | "light" | "auto";
  onSetThemeMode: (v: "dark" | "light" | "auto") => void;

  // Search
  searchProviders: SearchProvider[];
  searchSelectedProviderId: string;
  searchResults: WebSearchResult[];
  searchQuery: string;
  isSearching: boolean;
  onSetSearchQuery: (q: string) => void;
  onSetSearchSelectedProviderId: (id: string) => void;
  onSearch: (query: string) => Promise<WebSearchResult[]>;
  onAddSearchProvider: () => void;
  onEditSearchProvider: (provider: SearchProvider) => void;
  onDeleteSearchProvider: (id: string) => Promise<void>;
  // Search provider modal
  showSearchProviderModal: boolean;
  editingSearchProvider: SearchProvider | null;
  searchProviderForm: { id: string; name: string; api_type: string; api_key: string; api_address: string; is_enabled: boolean };
  onCloseSearchProviderModal: () => void;
  onUpdateSearchProviderForm: (field: string, value: string | boolean) => void;
  onSaveSearchProvider: () => Promise<void>;

  // MCP Servers
  mcpServers: McpServer[];
  showMcpModal: boolean;
  editingMcpServer: McpServer | null;
  mcpForm: { id: string; name: string; command: string; args: string; env: string; url: string; server_type: "stdio" | "sse"; is_enabled: boolean };
  onOpenMcpModal: (server?: McpServer) => void;
  onCloseMcpModal: () => void;
  onUpdateMcpForm: (field: string, value: string | boolean) => void;
  onSaveMcpServer: () => Promise<void>;
  onDeleteMcpServer: (id: string) => Promise<void>;

  // Backup
  backupTableInfo: BackupTableInfo[];
  backupSelectedTables: Set<string>;
  isBackupExporting: boolean;
  isBackupImporting: boolean;
  lastImportResult: ImportResult | null;
  onLoadBackupInfo: () => Promise<void>;
  onToggleBackupTable: (tableName: string) => void;
  onSelectAllBackupTables: () => void;
  onDeselectAllBackupTables: () => void;
  onExportBackup: () => Promise<string | null>;
  onImportBackup: (jsonStr: string) => Promise<ImportResult | null>;
}

const SETTINGS_TABS: { id: SettingsSubTab; label: string; icon: React.ReactNode }[] = [
  { id: "platform", label: "大模型平台", icon: <Plug className="h-3.5 w-3.5" /> },
  { id: "system", label: "系统设置", icon: <Settings className="h-3.5 w-3.5" /> },
  { id: "mcp", label: "MCP 服务器", icon: <Server className="h-3.5 w-3.5" /> },
  { id: "backup", label: "数据备份", icon: <Database className="h-3.5 w-3.5" /> },
];

export type PlatformSubTabProps = Pick<
  SettingsTabProps,
  | "platforms"
  | "selectedPlatformId"
  | "platformModels"
  | "modelTestingState"
  | "fetchingModels"
  | "onSelectPlatform"
  | "onTogglePlatform"
  | "onAddPlatform"
  | "onEditPlatform"
  | "onDeletePlatform"
  | "onFetchRemoteModels"
  | "onAddModel"
  | "onToggleModelEnabled"
  | "onTestModel"
  | "onDeleteModel"
  | "batchTesting"
  | "onBatchTestModels"
>;

export function SettingsTab(props: SettingsTabProps) {
  return (
    <div className="flex flex-col h-full overflow-hidden flex-1">
      {/* Top horizontal Tab bar */}
      <div className="flex items-center gap-1 px-5 pt-4 pb-2 border-b border-border bg-[rgba(10,10,14,0.1)]">
        {/* platform → focused 模型中心; mcp → focused MCP page. Settings keeps only system + backup. */}
        {SETTINGS_TABS.filter((tab) => tab.id !== "platform" && tab.id !== "mcp").map((tab) => (
          <button
            key={tab.id}
            onClick={() => props.setSettingsSubTab(tab.id)}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium transition-all cursor-pointer",
              props.settingsSubTab === tab.id
                ? "bg-accent/10 text-accent border border-accent/30"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/20 border border-transparent"
            )}
          >
            {tab.icon}
            {tab.label}
          </button>
        ))}
      </div>

      {/* Content panel */}
      <div className="flex-1 overflow-y-auto p-5">
        {props.settingsSubTab === "platform" && <PlatformSubTab {...props} />}
        {props.settingsSubTab === "system" && <SystemSubTab {...props} />}
        {props.settingsSubTab === "mcp" && <McpSubTab {...props} />}
        {props.settingsSubTab === "backup" && <BackupSubTab {...props} />}
      </div>
    </div>
  );
}

// ── Platform Sub-Tab ─────────────────────────────────

export function PlatformSubTab({
  platforms,
  selectedPlatformId,
  platformModels,
  modelTestingState,
  fetchingModels,
  onSelectPlatform,
  onTogglePlatform,
  onAddPlatform,
  onEditPlatform,
  onDeletePlatform,
  onFetchRemoteModels,
  onAddModel,
  onToggleModelEnabled,
  onTestModel,
  onDeleteModel,
  batchTesting,
  onBatchTestModels,
}: PlatformSubTabProps) {
  const selectedPlatform = platforms.find((p) => p.id === selectedPlatformId);

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
      {/* Setup guide banner when no platform has API key */}
      {platforms.every(p => !p.api_key) && (
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
          <Button size="sm" variant="outline" onClick={onAddPlatform} className="h-7 w-7 p-0">
            <Plus className="h-3 w-3" />
          </Button>
        </div>

        <div className="flex flex-col gap-1.5 flex-1 overflow-y-auto">
          {platforms.length === 0 ? (
            <div className="py-5 text-center text-muted-foreground text-xs">无平台</div>
          ) : (
            platforms.map((plat) => {
              const isActive = selectedPlatformId === plat.id;
              return (
                <div
                  key={plat.id}
                  className={cn(
                    "p-2 rounded-lg border cursor-pointer flex justify-between items-center transition-all",
                    isActive ? "bg-accent/[0.06] border-accent/30" : "bg-muted/5 border-border hover:bg-muted/20"
                  )}
                  onClick={() => onSelectPlatform(plat.id)}
                >
                  <div className="min-w-0">
                    <span className="font-semibold text-sm block truncate">{plat.name}</span>
                    <span className="text-xs text-muted-foreground">{plat.api_type}</span>
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
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button size="sm" variant="outline" onClick={onFetchRemoteModels} disabled={fetchingModels}>
                    <RefreshCw className={cn("h-3 w-3", fetchingModels && "animate-spin")} />
                    {fetchingModels ? "拉取中..." : "获取模型"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onBatchTestModels(selectedPlatform.id)} disabled={batchTesting[selectedPlatform.id]}>
                    {batchTesting[selectedPlatform.id] ? <RefreshCw className="h-3 w-3 animate-spin" /> : <Activity className="h-3 w-3" />}
                    {batchTesting[selectedPlatform.id] ? "检测中..." : "健康检测"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onEditPlatform(selectedPlatform)}>
                    <Edit className="h-3 w-3" /> 编辑
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => onDeletePlatform(selectedPlatform.id)}>
                    <Trash2 className="h-3 w-3 text-destructive" /> 删除
                  </Button>
                </div>
              </CardContent>
            </Card>

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
                <Button size="sm" variant="outline" onClick={onAddModel}>
                  <Plus className="h-3 w-3" /> 自定义模型
                </Button>
              </div>

              <div className="flex-1 overflow-y-auto flex flex-col gap-2">
                {platformModels.length === 0 ? (
                  <div className="text-center text-muted-foreground py-10 text-xs">
                    暂无可用模型，请点击上方"获取模型"自动从服务商同步。
                  </div>
                ) : (
                  platformModels.map((model) => {
                    const testState = modelTestingState[model.id] || "idle";
                    return (
                      <div
                        key={model.id}
                        className="flex justify-between items-center px-3 py-2 border-b border-border"
                      >
                        <div className="flex items-center gap-2.5 min-w-0">
                          <Checkbox
                            checked={model.is_enabled}
                            onCheckedChange={() => onToggleModelEnabled(model)}
                          />
                          <span className={cn("text-sm font-medium truncate", !model.is_enabled && "opacity-60")}>
                            {model.model_name}
                          </span>
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
                                testState === "success" && "bg-emerald-500 shadow-[0_0_8px_#10b981]",
                                testState === "auth_error" && "bg-red-500 shadow-[0_0_8px_#ef4444]",
                                testState === "no_api_key" && "bg-red-400 shadow-[0_0_8px_#f87171]",
                                testState === "rate_limited" && "bg-amber-500 shadow-[0_0_8px_#f59e0b]",
                                testState === "error" && "bg-red-500 shadow-[0_0_8px_#ef4444]",
                                testState === "unreachable" && "bg-red-500 shadow-[0_0_8px_#ef4444]",
                                testState === "testing" && "bg-amber-500 animate-pulse",
                                testState === "idle" && "bg-gray-500"
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
                            onClick={() => onDeleteModel(model.id)}
                            className="bg-transparent border-none text-destructive cursor-pointer"
                          >
                            <Trash2 className="h-3 w-3" />
                          </button>
                        </div>
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

function SystemSubTab({
  accounts,
  onAddAccount,
  onEditAccount,
  onDeleteAccount,
  onSwitchAccount,
  targetModel, setTargetModel,
  gpuAcceleration, setGpuAcceleration,
  idleTimeout, setIdleTimeout,
  autoStart, setAutoStart,
  startToTray, setStartToTray,
  useWsl, setUseWsl,
  wslDistro, setWslDistro,
  onSaveSettings,
  selectionCaptureMode, onSetSelectionCaptureMode,
  selectionShowOnCapture, onSetSelectionShowOnCapture,
  selectionPreserveClipboard, onSetSelectionPreserveClipboard,
  isSelectionCapturing,
  lastSelectionCapture: _lastSelectionCapture,
  selectionCaptureError,
  selectionHistory: _selectionHistory,
  onTestSelectionCapture,
  onSaveSelectionSettings: _onSaveSelectionSettings,
  onLoadSelectionHistory: _onLoadSelectionHistory,
  onDeleteSelectionHistoryItem: _onDeleteSelectionHistoryItem,
  onClearSelectionHistory: _onClearSelectionHistory,
  translatePreferredLang, onSetTranslatePreferredLang,
  translateAlterLang, onSetTranslateAlterLang,
  translateModel, onSetTranslateModel,
  translateAutoDetect, onSetTranslateAutoDetect,
  translateCustomPrompt, onSetTranslateCustomPrompt,
  onSaveTranslationSettings,
  themeMode,
  onSetThemeMode,
  searchProviders,
  onAddSearchProvider,
  onEditSearchProvider,
  onDeleteSearchProvider,
  showSearchProviderModal,
  editingSearchProvider: _editingSearchProvider,
  searchProviderForm,
  onCloseSearchProviderModal,
  onUpdateSearchProviderForm,
  onSaveSearchProvider,
}: SettingsTabProps) {
  // ── Available models for dropdowns ────────────────────
  const [availableModels, setAvailableModels] = useState<string[]>([]);

  useEffect(() => {
    modelApi.getAvailableNames()
      .then(names => setAvailableModels(names))
      .catch(e => console.error("[Settings] Failed to load available models:", e));
  }, []);

  return (
    <div className="flex flex-col gap-4 max-w-4xl mx-auto">
      {/* Theme selector lives in the title bar. */}
      {false && (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            🎨 外观主题
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex gap-2">
            {([
              { value: "dark" as const, label: "🌙 深色", desc: "默认暗色主题" },
              { value: "light" as const, label: "☀️ 浅色", desc: "明亮简洁风格" },
              { value: "auto" as const, label: "🔄 跟随系统", desc: "自动适配 OS 主题" },
            ]).map((opt) => (
              <button
                key={opt.value}
                onClick={() => onSetThemeMode(opt.value)}
                className={cn(
                  "flex-1 flex flex-col items-center gap-1 p-3 rounded-lg border transition-all",
                  themeMode === opt.value
                    ? "border-primary bg-primary/10 text-primary"
                    : "border-border hover:border-primary/30"
                )}
              >
                <span className="text-sm font-medium">{opt.label}</span>
                <span className="text-xs text-muted-foreground">{opt.desc}</span>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>
      )}

      {/* Account Management */}
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="text-sm">🔑 智能体云端账户授权管理器</CardTitle>
          <Button size="sm" variant="outline" onClick={onAddAccount}>
            <Plus className="h-3 w-3" /> 新增账户
          </Button>
        </CardHeader>
        <CardContent>
          {accounts.length === 0 ? (
            <div className="py-2.5 text-center text-muted-foreground text-xs">暂无账户凭证</div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {accounts.map((acc) => (
                <div
                  key={acc.id}
                  className={cn(
                    "flex justify-between items-center p-2.5 border-b border-border rounded-md",
                    acc.is_active && "bg-accent/[0.04]"
                  )}
                >
                  <div>
                    <span className="font-semibold text-sm">{acc.account_name}</span>
                    <span className="text-xs text-muted-foreground ml-2.5">
                      Endpoint: <code>{acc.api_host}</code> | Model: <code>{acc.target_model}</code>
                    </span>
                  </div>
                  <div className="flex gap-1.5">
                    {!acc.is_active && (
                      <Button size="sm" variant="outline" onClick={() => onSwitchAccount(acc.id)}>启用</Button>
                    )}
                    <Button size="sm" variant="outline" onClick={() => onEditAccount(acc)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteAccount(acc.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* System Configuration */}
      <Card>
        <CardContent className="p-5 flex flex-col gap-3">
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>内置功能默认模型</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={targetModel}
                onChange={(e) => setTargetModel(e.target.value)}
              >
                <option value="">— 请选择 —</option>
                {availableModels.map(m => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <span className="text-xs text-muted-foreground">
                供 OMNIX 自身的内置功能使用（划词翻译、语言检测、知识库问答等），与 Agent 对话无关。
                Agent（Codex/Claude）默认模型请到「模型中心」用 ☆ 设置。
              </span>
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 my-2.5">
            <div className="flex items-center gap-2.5">
              <Switch checked={gpuAcceleration} onCheckedChange={setGpuAcceleration} id="gpu_chk" />
              <Label htmlFor="gpu_chk" className="m-0">启用本地 LLM 硬件 GPU 加速</Label>
            </div>
            <div className="space-y-1.5">
              <Label>智能体进程超时时间 (分钟)</Label>
              <Input type="number" value={idleTimeout} onChange={(e) => setIdleTimeout(e.target.value)} />
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={autoStart} onCheckedChange={setAutoStart} id="autostart_chk" />
              <Label htmlFor="autostart_chk" className="m-0">跟随 Windows 开机自启动</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={startToTray} onCheckedChange={setStartToTray} id="tray_chk" />
              <Label htmlFor="tray_chk" className="m-0">启动时最小化至系统托盘</Label>
            </div>
          </div>

          <Separator className="my-2.5" />

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch checked={useWsl} onCheckedChange={setUseWsl} id="wsl_chk" />
              <Label htmlFor="wsl_chk" className="m-0">在 WSL 中启动</Label>
            </div>
            {useWsl && (
              <div className="space-y-1.5">
                <Label>WSL 发行版名称</Label>
                <Input value={wslDistro} onChange={(e) => setWslDistro(e.target.value)} />
              </div>
            )}
          </div>

          <Button className="w-full mt-4" onClick={onSaveSettings}>
            <Save className="h-4 w-4" /> 💾 保存系统配置并重载网关
          </Button>
        </CardContent>
      </Card>

      {/* ── Selection Assistant ─────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <MousePointerClick className="h-4 w-4" /> 🖱️ 划词助手
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="bg-muted/30 rounded-md px-3 py-2 text-xs text-muted-foreground flex items-start gap-2">
            <MousePointerClick className="h-3.5 w-3.5 mt-0.5 shrink-0" />
            <span>
              <strong>使用方法：</strong>开启自动捕获后，在任意应用中用鼠标选中文字，QA 窗口会自动弹出操作栏（翻译/解释/总结/搜索）。
              按 Ctrl+Shift+Space 可手动唤起 QA 窗口。
            </span>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="flex items-center gap-2.5">
              <Switch
                checked={selectionShowOnCapture}
                onCheckedChange={async (v) => {
                  onSetSelectionShowOnCapture(v);
                  try {
                    const { selectionApi } = await import("@/lib/tauri-api");
                    await selectionApi.toggleAutoCapture(v);
                  } catch (e) {
                    console.error("[AutoCapture] Toggle failed:", e);
                  }
                }}
                id="sel_auto_capture"
              />
              <Label htmlFor="sel_auto_capture" className="m-0">🖱️ 自动捕获选中文字</Label>
            </div>
            <div className="flex items-center gap-2.5">
              <Switch checked={selectionPreserveClipboard} onCheckedChange={onSetSelectionPreserveClipboard} id="sel_preserve_cb" />
              <Label htmlFor="sel_preserve_cb" className="m-0">保护剪贴板原内容</Label>
            </div>
          </div>

          <div className="space-y-1.5">
            <Label>捕获模式</Label>
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              value={selectionCaptureMode}
              onChange={(e) => onSetSelectionCaptureMode(e.target.value)}
            >
              <option value="hybrid">混合模式（UIA 优先，自动回退剪贴板）</option>
              <option value="uia_only">仅 UI Automation（被动读取，不修改剪贴板）</option>
              <option value="clipboard_only">仅剪贴板（模拟 Ctrl+C）</option>
            </select>
          </div>

          <div className="flex gap-2 mt-1">
            <Button
              size="sm"
              variant="outline"
              onClick={async () => {
                const text = await onTestSelectionCapture();
                if (text) {
                  toast.success("捕获成功！选中文本: " + text.slice(0, 60) + (text.length > 60 ? "…" : ""));
                } else {
                  toast.error("未捕获到文本，请确保有文字被选中");
                }
              }}
              disabled={isSelectionCapturing}
            >
              {isSelectionCapturing ? "捕获中..." : "🎯 测试捕获"}
            </Button>
          </div>

          {selectionCaptureError && (
            <div className="text-xs text-destructive bg-destructive/10 rounded px-2 py-1.5 mt-1">
              ⚠️ {selectionCaptureError}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ── Translation Settings ────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Languages className="h-4 w-4" /> 🌐 AI 翻译助手
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>首选目标语言</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={translatePreferredLang}
                onChange={(e) => onSetTranslatePreferredLang(e.target.value)}
              >
                {BUILTIN_LANGUAGES.map(l => (
                  <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
                ))}
              </select>
            </div>
            <div className="space-y-1.5">
              <Label>备选目标语言</Label>
              <select
                className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
                value={translateAlterLang}
                onChange={(e) => onSetTranslateAlterLang(e.target.value)}
              >
                {BUILTIN_LANGUAGES.map(l => (
                  <option key={l.langCode} value={l.langCode}>{l.emoji} {l.value}</option>
                ))}
              </select>
            </div>
          </div>

          <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground bg-muted/30 rounded-md px-3 py-2">
            <ArrowRightLeft className="h-3 w-3 flex-shrink-0" />
            <span>智能双向：识别为首选语言时自动翻译为备选，反之亦然</span>
          </div>

          <div className="space-y-1.5">
            <Label>翻译模型（留空使用全局默认）</Label>
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              value={translateModel}
              onChange={(e) => onSetTranslateModel(e.target.value)}
            >
              <option value="">— 使用全局默认 —</option>
              {availableModels.map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </div>

          <div className="flex items-center gap-2.5">
            <Switch checked={translateAutoDetect} onCheckedChange={onSetTranslateAutoDetect} id="translate_auto_detect" />
            <Label htmlFor="translate_auto_detect" className="m-0">自动检测源语言</Label>
          </div>

          <div className="space-y-1.5">
            <Label>自定义翻译 Prompt（留空使用默认）</Label>
            <textarea
              className="w-full border border-border rounded-md px-3 py-2 text-xs bg-background min-h-20 font-mono"
              value={translateCustomPrompt}
              onChange={(e) => onSetTranslateCustomPrompt(e.target.value)}
              placeholder="可用占位符: {{target_language}}, {{text}}"
            />
          </div>

          <Button
            size="sm"
            variant="default"
            onClick={async () => {
              try {
                await onSaveTranslationSettings({
                  preferredLang: translatePreferredLang,
                  alterLang: translateAlterLang,
                  translateModel,
                  autoDetect: translateAutoDetect,
                  customPrompt: translateCustomPrompt,
                });
                toast.success("翻译设置已保存！");
              } catch (e) {
                toast.error("保存失败：" + String(e));
              }
            }}
          >
            <Save className="h-3 w-3" /> 保存翻译设置
          </Button>
        </CardContent>
      </Card>

      {/* ── Document Processing ────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <FileText className="h-4 w-4" /> 📄 文档处理
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            配置文档处理器后，导入 PDF/Word 等文件时可自动转换为 Markdown 再入库，提高检索精度。
          </span>

          <div className="space-y-1.5">
            <Label>文档转 Markdown 处理器</Label>
            <select
              className="w-full border border-border rounded-md px-3 py-2 text-sm bg-background"
              defaultValue="system_ocr"
            >
              <option value="system_ocr">系统 OCR（Windows 内置）</option>
              <option value="tesseract">Tesseract OCR（需安装）</option>
              <option value="mineru">MinerU API</option>
              <option value="doc2x">Doc2X API</option>
              <option value="mistral_ocr">Mistral OCR API</option>
              <option value="paddleocr">PaddleOCR API</option>
            </select>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label>API 地址（可选）</Label>
              <Input placeholder="https://api.example.com/v1/convert" />
            </div>
            <div className="space-y-1.5">
              <Label>API Key（可选）</Label>
              <Input type="password" placeholder="留空则无需认证" />
            </div>
          </div>

          <div className="flex items-center gap-2.5">
            <Switch defaultChecked={true} id="doc_auto_convert" />
            <Label htmlFor="doc_auto_convert" className="m-0">导入文档时自动转换为 Markdown</Label>
          </div>

          <div className="bg-muted/30 rounded-md px-3 py-2 text-xs text-muted-foreground">
            💡 <strong>提示：</strong>本地 OCR 无需额外配置；API 处理器需填写地址和密钥。
            推荐使用 Doc2X 或 MinerU 以获得最佳转换质量。
          </div>
        </CardContent>
      </Card>

      {/* ── Search Providers ─────────────────────── */}
      <Card>
        <CardHeader className="flex-row justify-between items-center">
          <CardTitle className="text-sm flex items-center gap-2">
            <Search className="h-4 w-4" /> 🔍 网络搜索配置
          </CardTitle>
          <Button size="sm" variant="outline" onClick={onAddSearchProvider}>
            <Plus className="h-3 w-3" /> 新增
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            配置网络搜索引擎后，在智能体对话中启用"联网搜索"即可让 AI 获取实时网络信息。
          </span>

          {/* Preset search providers (Cherry Studio style) */}
          <div>
            <span className="text-xs font-medium text-muted-foreground mb-2 block">🌐 搜索引擎</span>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {([
                { name: "Google", type: "google", url: "https://developers.google.com/custom-search", color: "text-blue-400", desc: "Custom Search API" },
                { name: "Bing", type: "bing", url: "https://www.microsoft.com/en-us/bing/apis", color: "text-cyan-400", desc: "Bing Search API" },
                { name: "DuckDuckGo", type: "duckduckgo", url: "https://duckduckgo.com/api", color: "text-orange-400", desc: "免费免 Key" },
                { name: "SearXNG", type: "searxng", url: "https://docs.searxng.org", color: "text-amber-400", desc: "自托管元搜索" },
              ]).map(preset => (
                <button
                  key={preset.name}
                  className="flex items-start gap-1.5 p-2 rounded-md border border-border/30 bg-background/20 hover:bg-background/40 transition-all text-left cursor-pointer"
                  onClick={() => {
                    onUpdateSearchProviderForm("api_type", preset.type);
                    onUpdateSearchProviderForm("name", preset.name);
                    if (preset.type === "searxng") {
                      onUpdateSearchProviderForm("api_address", "http://localhost:8080");
                    } else if (preset.type === "google") {
                      onUpdateSearchProviderForm("api_address", "");
                    }
                    openUrl(preset.url).catch(() => window.open(preset.url, "_blank"));
                  }}
                >
                  <ExternalLink className={cn("h-3 w-3 shrink-0 mt-0.5", preset.color)} />
                  <div className="flex flex-col">
                    <span className="text-xs font-medium">{preset.name}</span>
                    <span className="text-xs text-muted-foreground leading-tight">{preset.desc}</span>
                  </div>
                </button>
              ))}
            </div>
          </div>
          <div>
            <span className="text-xs font-medium text-muted-foreground mb-2 block">☁️ 云端搜索</span>
            <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5">
              {([
                { name: "Tavily", type: "tavily", url: "https://tavily.com", color: "text-blue-400" },
                { name: "Exa", type: "exa", url: "https://exa.ai", color: "text-emerald-400" },
                { name: "智谱搜索", type: "zhipu", url: "https://open.bigmodel.cn", color: "text-violet-400" },
                { name: "Bocha", type: "bocha", url: "https://bocha.ai", color: "text-amber-400" },
                { name: "Jina", type: "jina", url: "https://jina.ai", color: "text-cyan-400" },
              ]).map(preset => (
                <button
                  key={preset.name}
                  className="flex items-center gap-1.5 p-2 rounded-md border border-border/30 bg-background/20 hover:bg-background/40 transition-all text-left cursor-pointer"
                  onClick={() => {
                    onUpdateSearchProviderForm("api_type", preset.type);
                    onUpdateSearchProviderForm("name", preset.name);
                    openUrl(preset.url).catch(() => window.open(preset.url, "_blank"));
                  }}
                >
                  <ExternalLink className={cn("h-3 w-3 shrink-0", preset.color)} />
                  <span className="text-xs font-medium">{preset.name}</span>
                </button>
              ))}
            </div>
          </div>
          {searchProviders.length === 0 ? (
            <div className="text-xs text-muted-foreground text-center py-2">暂无搜索引擎配置，点击"新增"添加</div>
          ) : (
            <div className="flex flex-col gap-2">
              {searchProviders.map((sp) => (
                <div key={sp.id} className="flex justify-between items-center p-2 rounded-md border-b border-border">
                  <div className="flex items-center gap-2">
                    <Badge variant={sp.is_enabled ? "default" : "secondary"}>
                      {sp.api_type.toUpperCase()}
                    </Badge>
                    <span className="text-sm font-medium">{sp.name}</span>
                    {sp.api_address && (
                      <span className="text-xs text-muted-foreground"><code>{sp.api_address}</code></span>
                    )}
                  </div>
                  <div className="flex gap-1.5">
                    <Button size="sm" variant="outline" onClick={() => onEditSearchProvider(sp)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteSearchProvider(sp.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
      {/* Search Provider Modal */}
      {showSearchProviderModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card p-6 w-full max-w-[480px] mx-4 max-h-[80vh] overflow-y-auto">
            <h3 className="text-lg font-semibold mb-4">
              {_editingSearchProvider ? "编辑搜索引擎" : "新增搜索引擎"}
            </h3>
            <div className="flex flex-col gap-3">
              <div className="space-y-1.5">
                <Label>名称</Label>
                <Input value={searchProviderForm.name} onChange={(e) => onUpdateSearchProviderForm("name", e.target.value)} placeholder="例如: SearXNG" />
              </div>
              <div className="space-y-1.5">
                <Label>类型</Label>
                <div className="flex flex-wrap gap-1.5">
                  {(["google", "bing", "duckduckgo", "searxng", "brave", "tavily", "exa", "zhipu", "bocha", "jina"] as const).map((t) => (
                    <button
                      key={t}
                      onClick={() => onUpdateSearchProviderForm("api_type", t)}
                      className={cn(
                        "px-2.5 py-1.5 rounded-md border text-xs",
                        searchProviderForm.api_type === t ? "border-primary bg-primary/10" : "border-border"
                      )}
                    >
                      {t.toUpperCase()}
                    </button>
                  ))}
                </div>
              </div>
              <div className="space-y-1.5">
                <Label>{searchProviderForm.api_type === "searxng" ? "SearXNG 地址" : "API 地址"}</Label>
                <Input value={searchProviderForm.api_address} onChange={(e) => onUpdateSearchProviderForm("api_address", e.target.value)} placeholder={searchProviderForm.api_type === "searxng" ? "http://localhost:8080" : "https://api.example.com"} />
                {searchProviderForm.api_type === "searxng" && (
                  <span className="text-xs text-muted-foreground">自托管 SearXNG 实例地址，通常为 Docker 部署的 localhost:8080</span>
                )}
              </div>
              {/* SearXNG: no API key needed, show Basic Auth instead */}
              {searchProviderForm.api_type === "searxng" ? (
                <div className="space-y-1.5">
                  <Label>HTTP Basic Auth（可选）</Label>
                  <div className="flex gap-2">
                    <Input value={searchProviderForm.api_key || ""} onChange={(e) => onUpdateSearchProviderForm("api_key", e.target.value)} placeholder="用户名" className="flex-1" />
                    <Input type="password" value={(searchProviderForm as Record<string, unknown>).basicAuthPassword as string || ""} onChange={(e) => onUpdateSearchProviderForm("basicAuthPassword" as keyof typeof searchProviderForm, e.target.value as never)} placeholder="密码" className="flex-1" />
                  </div>
                  <span className="text-xs text-muted-foreground">适用于远程部署的 SearXNG 实例（RFC 7617 Basic 认证）</span>
                </div>
              ) : (
                <div className="space-y-1.5">
                  <Label>API Key（可选）</Label>
                  <Input type="password" value={searchProviderForm.api_key} onChange={(e) => onUpdateSearchProviderForm("api_key", e.target.value)} placeholder="留空则无需认证" />
                </div>
              )}
              <div className="flex items-center gap-2">
                <Switch checked={searchProviderForm.is_enabled} onCheckedChange={(v) => onUpdateSearchProviderForm("is_enabled", v)} id="sp_enabled" />
                <Label htmlFor="sp_enabled">启用</Label>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="ghost" onClick={onCloseSearchProviderModal}>取消</Button>
              <Button variant="outline" onClick={async () => {
                try {
                  // Save first so the provider exists for testing
                  await onSaveSearchProvider();
                  const { search: searchFn } = await import("@/lib/tauri-api").then(m => m.searchApi);
                  const results = await searchFn("hello world", searchProviderForm.id, 3);
                  toast.success(`连通成功！返回 ${results.length} 条结果`);
                } catch (e) {
                  toast.error("连通失败：" + String(e));
                }
              }}>
                <Search className="h-4 w-4" /> 测试连接
              </Button>
              <Button onClick={async () => {
                try {
                  await onSaveSearchProvider();
                  toast.success("搜索引擎保存成功！");
                } catch (e) {
                  toast.error("保存失败：" + String(e));
                }
              }}>
                <Save className="h-4 w-4" /> 保存
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── MCP Servers Sub-Tab ─────────────────────────────────

export function McpSubTab({
  mcpServers,
  onOpenMcpModal,
  onDeleteMcpServer,
  showMcpModal,
  editingMcpServer,
  mcpForm,
  onCloseMcpModal,
  onUpdateMcpForm,
  onSaveMcpServer,
}: SettingsTabProps) {
  const [agentStates, setAgentStates] = useState<AgentMcpState[]>([]);
  const [syncBusy, setSyncBusy] = useState("");
  const loadAgentStates = () => mcpSyncApi.getAgentStates().then(setAgentStates).catch(() => {});
  useEffect(() => { loadAgentStates(); }, []);

  const agentLabel = (agent: string) => (agent === "claude_code" ? "Claude" : "Codex");
  const isSynced = (serverName: string, agent: string) =>
    agentStates.find((state) => state.agent === agent)?.server_names.includes(serverName) ?? false;

  const syncServer = async (server: McpServer) => {
    setSyncBusy(server.id);
    try {
      const reports = await mcpSyncApi.syncToAgents(["claude_code", "codex"], [server.id]);
      await loadAgentStates();
      const skipped = reports.flatMap((report) => report.skipped);
      toast.success(`已同步「${server.name}」到 Claude / Codex${skipped.length ? `（部分跳过：${skipped.join("；")}）` : ""}`);
    } catch (error) {
      toast.error(`同步失败：${error}`);
    } finally {
      setSyncBusy("");
    }
  };

  const unsyncServer = async (server: McpServer, agent: string) => {
    setSyncBusy(server.id);
    try {
      await mcpSyncApi.removeFromAgent(agent, server.name);
      await loadAgentStates();
      toast.success(`已从 ${agentLabel(agent)} 撤销「${server.name}」`);
    } catch (error) {
      toast.error(`撤销失败：${error}`);
    } finally {
      setSyncBusy("");
    }
  };

  return (
    <div className="flex flex-col gap-4 max-w-4xl mx-auto">
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <div>
            <CardTitle className="text-sm">🔌 MCP 服务器管理</CardTitle>
            <p className="mt-1 text-xs text-muted-foreground">配一次，点「同步」即可写入 Claude Code 和 Codex 的原生配置（写前自动备份，可单独撤销）。</p>
          </div>
          <Button size="sm" variant="outline" onClick={() => onOpenMcpModal()}>
            <Plus className="h-3 w-3" /> 新增
          </Button>
        </CardHeader>
        <CardContent>
          {mcpServers.length === 0 ? (
            <div className="py-2.5 text-center text-muted-foreground text-xs">
              暂无 MCP 服务器配置。点击"新增"添加。
            </div>
          ) : (
            <div className="flex flex-col gap-2.5">
              {mcpServers.map((srv) => (
                <div
                  key={srv.id}
                  className="flex justify-between items-center p-2.5 border-b border-border rounded-md"
                >
                  <div className="flex items-center gap-2">
                    <Badge variant={srv.server_type === "stdio" ? "default" : "secondary"}>
                      {srv.server_type.toUpperCase()}
                    </Badge>
                    <span className="font-semibold text-sm">{srv.name}</span>
                    {srv.server_type === "stdio" && (
                      <span className="text-xs text-muted-foreground">
                        <code>{srv.command}</code>
                      </span>
                    )}
                    {srv.server_type === "sse" && (
                      <span className="text-xs text-muted-foreground">
                        <code>{srv.url}</code>
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-1.5">
                    {["claude_code", "codex"].map((agent) => {
                      const synced = isSynced(srv.name, agent);
                      return (
                        <button
                          key={agent}
                          onClick={() => synced && unsyncServer(srv, agent)}
                          disabled={syncBusy === srv.id || !synced}
                          title={synced ? `已同步到 ${agentLabel(agent)}（点击撤销）` : `未同步到 ${agentLabel(agent)}`}
                          className={cn(
                            "rounded border px-1.5 py-0.5 text-[10px]",
                            synced ? "border-success/40 text-success" : "border-border text-muted-foreground opacity-60",
                            synced && "cursor-pointer hover:bg-success/10"
                          )}
                        >
                          {agentLabel(agent)} {synced ? "✓" : "—"}
                        </button>
                      );
                    })}
                    <Button size="sm" variant="outline" disabled={syncBusy === srv.id} onClick={() => syncServer(srv)} title="同步到 Claude Code 和 Codex">
                      <Plug className="h-3 w-3" /> 同步
                    </Button>
                    <Badge variant={srv.is_enabled ? "default" : "secondary"}>
                      {srv.is_enabled ? "启用" : "禁用"}
                    </Badge>
                    <Button size="sm" variant="outline" onClick={() => onOpenMcpModal(srv)}>
                      <Edit className="h-3 w-3" />
                    </Button>
                    <Button size="sm" variant="outline" onClick={() => onDeleteMcpServer(srv.id)}>
                      <Trash2 className="h-3 w-3 text-destructive" />
                    </Button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* ── MCP Market (Discover) ──────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Store className="h-4 w-4" /> 🛒 发现 MCP 服务器
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            浏览 MCP 市场以发现和安装新的 MCP 服务器，扩展 AI 的工具能力。
          </span>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
            {([
              { name: "Smithery", desc: "MCP 服务器市场与注册中心", url: "https://smithery.ai", color: "text-blue-400" },
              { name: "MCP.so", desc: "MCP 服务器目录与搜索", url: "https://mcp.so", color: "text-emerald-400" },
              { name: "Glama", desc: "MCP 服务器与 AI 工具发现", url: "https://glama.ai/mcp/servers", color: "text-purple-400" },
              { name: "MCP Hub", desc: "官方 MCP 服务器集合", url: "https://github.com/modelcontextprotocol/servers", color: "text-orange-400" },
            ]).map(market => (
              <button
                key={market.name}
                className="flex items-center gap-2.5 p-3 rounded-lg border border-border/50 bg-background/30 hover:bg-background/60 transition-all text-left cursor-pointer"
                onClick={() => {
                  openUrl(market.url).catch(() => window.open(market.url, "_blank"));
                }}
              >
                <ExternalLink className={cn("h-4 w-4 shrink-0", market.color)} />
                <div className="min-w-0">
                  <span className="text-sm font-medium block">{market.name}</span>
                  <span className="text-xs text-muted-foreground">{market.desc}</span>
                </div>
              </button>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* ── MCP Providers ──────────────────────────────── */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Key className="h-4 w-4" /> 🔑 MCP 供应商
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <span className="text-xs text-muted-foreground">
            通过 API Token 从 MCP 供应商批量获取和安装 MCP 服务器。
          </span>
          <div className="flex flex-col gap-2.5">
            {([
              { name: "通义百炼", desc: "阿里云百炼平台 MCP 服务", url: "https://bailian.console.aliyun.com/" },
              { name: "ModelScope", desc: "魔搭社区 MCP 工具", url: "https://modelscope.cn/" },
              { name: "蓝耘", desc: "蓝耘科技 MCP 服务", url: "https://cloud.lanyun.net/" },
              { name: "302.AI", desc: "302.AI MCP 网关", url: "https://302.ai/" },
              { name: "MCP Router", desc: "MCP 路由代理服务", url: "https://mcprouter.com/" },
            ]).map(provider => (
              <div key={provider.name} className="flex justify-between items-center p-2.5 border border-border/30 rounded-md bg-background/20">
                <div className="min-w-0">
                  <span className="text-sm font-medium">{provider.name}</span>
                  <span className="text-xs text-muted-foreground ml-2">{provider.desc}</span>
                </div>
                <Button
                  size="sm"
                  variant="outline"
                  className="text-xs gap-1"
                  onClick={() => {
                    openUrl(provider.url).catch(() => window.open(provider.url, "_blank"));
                  }}
                >
                  <ExternalLink className="h-3 w-3" /> 官网
                </Button>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* MCP Server Modal */}
      {showMcpModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="glass-card p-6 w-full max-w-[480px] mx-4 max-h-[80vh] overflow-y-auto">
            <h3 className="text-lg font-semibold mb-4">
              {editingMcpServer ? "编辑 MCP 服务器" : "新增 MCP 服务器"}
            </h3>
            <div className="flex flex-col gap-3">
              <div className="space-y-1.5">
                <Label>名称</Label>
                <Input value={mcpForm.name} onChange={(e) => onUpdateMcpForm("name", e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label>类型</Label>
                <div className="flex gap-2">
                  {(["stdio", "sse"] as const).map((t) => (
                    <button
                      key={t}
                      onClick={() => onUpdateMcpForm("server_type", t)}
                      className={cn(
                        "flex-1 p-2 rounded-md border text-sm",
                        mcpForm.server_type === t
                          ? "border-primary bg-primary/10"
                          : "border-border"
                      )}
                    >
                      {t === "stdio" ? "STDIO (命令行)" : "SSE (HTTP)"}
                    </button>
                  ))}
                </div>
              </div>
              {mcpForm.server_type === "stdio" && (
                <>
                  <div className="space-y-1.5">
                    <Label>启动命令</Label>
                    <Input value={mcpForm.command} onChange={(e) => onUpdateMcpForm("command", e.target.value)} placeholder="npx" />
                  </div>
                  <div className="space-y-1.5">
                    <Label>参数 (JSON 数组)</Label>
                    <Input value={mcpForm.args} onChange={(e) => onUpdateMcpForm("args", e.target.value)} placeholder='["-y", "@modelcontextprotocol/server"]' />
                  </div>
                  <div className="space-y-1.5">
                    <Label>环境变量 (JSON 对象)</Label>
                    <Input value={mcpForm.env} onChange={(e) => onUpdateMcpForm("env", e.target.value)} placeholder='{"KEY": "value"}' />
                  </div>
                </>
              )}
              {mcpForm.server_type === "sse" && (
                <div className="space-y-1.5">
                  <Label>服务器 URL</Label>
                  <Input value={mcpForm.url} onChange={(e) => onUpdateMcpForm("url", e.target.value)} placeholder="http://localhost:3001/sse" />
                </div>
              )}
              <div className="flex items-center gap-2">
                <Switch checked={mcpForm.is_enabled} onCheckedChange={(v) => onUpdateMcpForm("is_enabled", v)} id="mcp_enabled" />
                <Label htmlFor="mcp_enabled">启用</Label>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <Button variant="ghost" onClick={onCloseMcpModal}>取消</Button>
              <Button onClick={async () => {
                try {
                  await onSaveMcpServer();
                  toast.success("MCP 服务器保存成功！");
                } catch (e) {
                  toast.error("保存失败：" + String(e));
                }
              }}>
                <Save className="h-4 w-4" /> 保存
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Backup Sub-Tab ──────────────────────────────────────

function BackupSubTab({
  backupTableInfo,
  backupSelectedTables,
  isBackupExporting,
  isBackupImporting,
  lastImportResult,
  onToggleBackupTable,
  onSelectAllBackupTables,
  onDeselectAllBackupTables,
  onExportBackup,
  onImportBackup,
}: SettingsTabProps) {
  const [importJson, setImportJson] = useState("");

  const handleExport = async () => {
    const json = await onExportBackup();
    if (json) {
      // Create a downloadable blob
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `omnix-backup-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success("备份导出成功！");
    } else {
      toast.error("导出失败");
    }
  };

  const handleImport = async () => {
    if (!importJson.trim()) {
      toast.error("请粘贴或选择备份 JSON 内容");
      return;
    }
    const result = await onImportBackup(importJson);
    if (result) {
      toast.success(`恢复成功！共 ${result.total_rows} 行数据。`);
    } else {
      toast.error("恢复失败，请检查 JSON 格式。");
    }
  };

  const handleFileSelect = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file) {
        const text = await file.text();
        setImportJson(text);
      }
    };
    input.click();
  };

  return (
    <div className="flex flex-col gap-4 max-w-4xl mx-auto">
      {/* Export */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Download className="h-4 w-4" /> 📦 数据导出
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex justify-between items-center mb-1">
            <span className="text-xs text-muted-foreground">选择要导出的数据表：</span>
            <div className="flex gap-1">
              <Button size="sm" variant="ghost" onClick={onSelectAllBackupTables}>全选</Button>
              <Button size="sm" variant="ghost" onClick={onDeselectAllBackupTables}>全不选</Button>
            </div>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-1.5 max-h-[240px] overflow-y-auto">
            {backupTableInfo.map((t) => (
              <label key={t.table_name} className="flex items-center gap-2 text-xs p-1.5 rounded hover:bg-muted/50 cursor-pointer">
                <Checkbox
                  checked={backupSelectedTables.has(t.table_name)}
                  onCheckedChange={() => onToggleBackupTable(t.table_name)}
                />
                <span className="flex-1 truncate">{t.table_name}</span>
                <Badge variant="secondary" className="text-xs">{t.row_count}</Badge>
              </label>
            ))}
          </div>
          <Button className="w-full" onClick={handleExport} disabled={isBackupExporting || backupSelectedTables.size === 0}>
            {isBackupExporting ? "导出中…" : <><Download className="h-4 w-4" /> 导出备份</>}
          </Button>
        </CardContent>
      </Card>

      {/* Import */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Upload className="h-4 w-4" /> 📥 数据恢复
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="text-xs text-warning bg-warning/10 p-2 rounded">
            ⚠️ 恢复操作将覆盖选中表的现有数据，请谨慎操作！
          </div>
          <Button variant="outline" onClick={handleFileSelect}>
            选择备份文件 (.json)
          </Button>
          <textarea
            className="w-full h-32 text-xs p-2 rounded-md border border-border bg-muted/30 font-mono resize-y"
            placeholder="或直接粘贴备份 JSON 内容…"
            value={importJson}
            onChange={(e) => setImportJson(e.target.value)}
          />
          <Button
            className="w-full"
            onClick={handleImport}
            disabled={isBackupImporting || !importJson.trim()}
          >
            {isBackupImporting ? "恢复中…" : <><Upload className="h-4 w-4" /> 恢复数据</>}
          </Button>
          {lastImportResult && (
            <div className="text-xs bg-success/10 text-success p-2 rounded">
              ✅ 恢复完成：{lastImportResult.tables_restored.map(([t, c]) => `${t}(${c}行)`).join(", ")}，共 {lastImportResult.total_rows} 行
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
