/**
 * DashboardTab — 开发环境诊断控制面板
 *
 * Shows: tip carousel, status overview cards, top models, env diagnostics, remote access
 */

import { useState, useEffect } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Lightbulb, Wifi, Cpu, Bot, Wrench, Globe, RefreshCw, BarChart3 } from "lucide-react";
import { OMNIX_TIPS, DEFAULT_PROXY_PORT } from "@/lib/constants";
import { requestLogApi } from "@/lib/tauri-api";
import type { DetectedAgent, RemoteAccessInfo } from "@/types";
import type { ModelUsage } from "@/lib/tauri-api";

interface DashboardTabProps {
  activeSessionsCount: number;
  detectedAgents: DetectedAgent[];
  tipIndex: number;
  envDiagnostics: Record<string, string>;
  repairLogs: string;
  repairingTool: string;
  remoteInfo: RemoteAccessInfo | null;
  onRunDiagnostics: () => void;
  onRepairTool: (name: string) => void;
  onLoadRemoteAccess: () => void;
}

export function DashboardTab({
  activeSessionsCount,
  detectedAgents,
  tipIndex,
  envDiagnostics,
  repairLogs,
  repairingTool,
  remoteInfo,
  onRunDiagnostics,
  onRepairTool,
  onLoadRemoteAccess,
}: DashboardTabProps) {
  const tip = OMNIX_TIPS[tipIndex];

  // Model usage TOP 5 (AingDesk inspired)
  const [topModels, setTopModels] = useState<ModelUsage[]>([]);
  useEffect(() => {
    requestLogApi.getStats()
      .then(stats => setTopModels(stats.top_models.slice(0, 5)))
      .catch(() => {});
  }, []);

  const maxCount = topModels.length > 0 ? Math.max(...topModels.map(m => m.request_count)) : 1;

  return (
    <div className="p-6 overflow-y-auto w-full flex flex-col gap-5">
      {/* Tip Card */}
      <Card className="bg-gradient-to-br from-purple-500/[0.08] to-blue-500/[0.08] border-purple-500/20">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-purple-400">
            <Lightbulb className="h-4 w-4" />
            智能开发贴士
          </CardTitle>
        </CardHeader>
        <CardContent>
          <span className="text-sm font-semibold text-foreground block mb-1">
            {tip?.title}
          </span>
          <p className="text-xs text-muted-foreground leading-relaxed m-0">
            {tip?.desc}
          </p>
        </CardContent>
      </Card>

      {/* Status Overview Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        <Card>
          <CardContent className="p-4">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Wifi className="h-3 w-3" /> 中转代理端口
            </span>
            <span className="text-2xl font-bold block mt-1">:{DEFAULT_PROXY_PORT}</span>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Cpu className="h-3 w-3" /> 活跃进程
            </span>
            <span className="text-2xl font-bold block mt-1">{activeSessionsCount} 个</span>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Bot className="h-3 w-3" /> 可用智能体
            </span>
            <span className="text-2xl font-bold block mt-1">
              {detectedAgents.filter((a) => a.status === "installed").length} / {detectedAgents.length}
            </span>
          </CardContent>
        </Card>
      </div>

      {/* Model Usage TOP 5 (AingDesk inspired) */}
      {topModels.length > 0 && (
        <Card>
          <CardHeader className="flex-row justify-between items-center mb-2">
            <CardTitle className="flex items-center gap-2 text-sm">
              <BarChart3 className="h-4 w-4" /> 常用模型 TOP 5
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-2">
              {topModels.map((m, i) => (
                <div key={m.model} className="flex items-center gap-3">
                  <span className="text-xs text-muted-foreground w-4 text-right">{i + 1}</span>
                  <div className="flex-1">
                    <div className="flex items-center justify-between mb-1">
                      <span className="text-xs font-medium truncate max-w-[200px]">{m.model}</span>
                      <span className="text-xs text-muted-foreground">
                        {m.request_count} 次 · {m.total_tokens.toLocaleString()} tokens
                      </span>
                    </div>
                    <div className="h-1.5 bg-muted/20 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-gradient-to-r from-cyan-500 to-blue-500 rounded-full transition-all"
                        style={{ width: `${(m.request_count / maxCount) * 100}%` }}
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Env Diagnostics */}
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Wrench className="h-4 w-4" /> 开发环境一键诊断
          </CardTitle>
          <Button size="sm" onClick={onRunDiagnostics}>
            <RefreshCw className="h-3 w-3" /> 运行诊断
          </Button>
        </CardHeader>
        <CardContent>
          {Object.keys(envDiagnostics).length === 0 ? (
            <p className="text-xs text-muted-foreground m-0">
              点击诊断按钮以获取本机的 Node.js、Git、Ripgrep 以及各个 CLI 智能体的安装信息。
            </p>
          ) : (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              {Object.entries(envDiagnostics).map(([tool, version]) => {
                const isInstalled = version && !version.toLowerCase().includes("not found");
                return (
                  <div
                    key={tool}
                    className="flex justify-between items-center p-3 rounded-lg bg-muted/5 border border-border"
                  >
                    <div>
                      <span className="text-sm font-semibold block">{tool}</span>
                      <Badge variant={isInstalled ? "success" : "destructive"}>
                        {isInstalled ? version : "未检测到安装"}
                      </Badge>
                    </div>
                    {!isInstalled && (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={() => onRepairTool(tool)}
                        disabled={repairingTool === tool}
                      >
                        {repairingTool === tool ? "修复中..." : "🔧 一键修复"}
                      </Button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
          {repairLogs && (
            <pre className="mt-4 p-3 bg-black text-lime-400 text-xs rounded-lg max-h-[150px] overflow-y-auto font-mono">
              {repairLogs}
            </pre>
          )}
        </CardContent>
      </Card>

      {/* Remote Access */}
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Globe className="h-4 w-4" /> 远程跨设备调试
          </CardTitle>
          <Button size="sm" variant="outline" onClick={onLoadRemoteAccess}>
            <RefreshCw className="h-3 w-3" /> 获取远程链接
          </Button>
        </CardHeader>
        <CardContent>
          {remoteInfo ? (
            <div className="flex flex-col gap-2 text-sm">
              <div>
                <span className="text-muted-foreground">局域网地址:</span>{" "}
                <code className="text-foreground">{remoteInfo.ip}</code>
              </div>
              <div>
                <span className="text-muted-foreground">身份凭证 Token:</span>{" "}
                <code className="text-foreground">{remoteInfo.token}</code>
              </div>
              <div>
                <span className="text-muted-foreground">完整网页控制端 URL:</span>{" "}
                <code className="text-foreground break-all">{remoteInfo.url}</code>
              </div>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground m-0">点击上方按钮获取远程调试链接</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
