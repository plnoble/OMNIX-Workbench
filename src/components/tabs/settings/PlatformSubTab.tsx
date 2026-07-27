/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useEffect, useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Activity, Brain, Code, Edit, Eye, Layers, Maximize2, Mic, Plus, RefreshCw, Star, Trash2, Wrench, Zap } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { settingsApi } from "@/lib/tauri-api";
import type { PlatformModel } from "@/types";
import type { PlatformSubTabProps } from "./types";

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

/** 悬浮状态坞开关（默认关、不开机自启，即时生效）。自管状态，不走保存流程。 */
