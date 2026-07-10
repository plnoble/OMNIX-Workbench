/**
 * DashboardTab — 开发环境诊断控制面板
 *
 * Shows: tip carousel, status overview cards, top models, env diagnostics, remote access
 */

import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Lightbulb, Wifi, Cpu, Bot, Wrench, RefreshCw, Copy, Smartphone, Rocket } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { OMNIX_TIPS, DEFAULT_PROXY_PORT } from "@/lib/constants";
import { TokenActivityPanel } from "@/components/TokenActivityPanel";
import QRCode from "qrcode";
import { remoteApi, settingsApi } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";
import type { DetectedAgent, RemoteAccessInfo } from "@/types";

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

  // Remote phone access (AionUi-style): toggle LAN binding + restart proxy.
  const [remoteEnabled, setRemoteEnabled] = useState(false);
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [qr, setQr] = useState("");
  const [appVersion, setAppVersion] = useState("");
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);
  useEffect(() => {
    settingsApi.get("remote_access_enabled").then((v) => setRemoteEnabled(v === "true")).catch(() => {});
  }, []);
  useEffect(() => {
    if (remoteEnabled && remoteInfo?.url) {
      QRCode.toDataURL(remoteInfo.url, { width: 200, margin: 1 }).then(setQr).catch(() => setQr(""));
    } else {
      setQr("");
    }
  }, [remoteEnabled, remoteInfo?.url]);

  const toggleRemote = async (enabled: boolean) => {
    if (enabled && !window.confirm("启用远程访问：OMNIX 会把服务绑定到局域网(0.0.0.0)，同一网络内、持有令牌的设备可访问你的会话。确定开启？")) return;
    setRemoteBusy(true);
    try {
      await remoteApi.setAccess(enabled);
      setRemoteEnabled(enabled);
      // Re-fetch the URL/token so the LAN address shows after re-binding.
      setTimeout(() => onLoadRemoteAccess(), 1000);
      toast.success(enabled ? "已启用远程访问" : "已关闭远程访问");
    } catch (e) {
      toast.error("切换失败", { description: String(e) });
    } finally {
      setRemoteBusy(false);
    }
  };

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

      {/* Token activity & cost (R4) — surfaces request_logs usage with cost + daily chart */}
      <TokenActivityPanel />

      {/* Software update — check for a newer signed release from GitHub */}
      <Card>
        <CardHeader className="flex-row justify-between items-center mb-4">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Rocket className="h-4 w-4" /> 软件更新
          </CardTitle>
          <Button size="sm" variant="outline" onClick={() => window.dispatchEvent(new Event("omnix:check-updates"))}>
            <RefreshCw className="h-3 w-3" /> 检查更新
          </Button>
        </CardHeader>
        <CardContent>
          <p className="m-0 text-xs text-muted-foreground">
            当前版本 <code className="text-foreground">v{appVersion || "…"}</code>。有新版本时会自动弹窗提示，也可随时点右上角手动检查。更新从 GitHub 发布，下载后自动安装并重启。
          </p>
        </CardContent>
      </Card>

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
            <Smartphone className="h-4 w-4" /> 手机远程访问
          </CardTitle>
          <div className="flex items-center gap-2">
            <span className={`text-xs ${remoteEnabled ? "text-success" : "text-muted-foreground"}`}>{remoteEnabled ? "已启用" : "已关闭"}</span>
            <Switch checked={remoteEnabled} disabled={remoteBusy} onCheckedChange={(v) => void toggleRemote(v)} />
          </div>
        </CardHeader>
        <CardContent>
          <p className="mb-3 text-xs text-muted-foreground">
            开启后，手机用浏览器打开下面的链接即可<strong>查看并继续</strong>你电脑上的 Agent 对话。同一 Wi-Fi 直接可用；异地需你自己用 Tailscale / 内网穿透 / 端口转发打通（OMNIX 不内置穿透）。
          </p>
          {remoteEnabled ? (
            remoteInfo ? (
              <div className="flex flex-col gap-2 text-sm">
                <div className="flex items-center gap-2">
                  <span className="shrink-0 text-muted-foreground">手机访问链接:</span>
                  <code className="min-w-0 flex-1 break-all text-foreground">{remoteInfo.url}</code>
                  <button
                    onClick={() => navigator.clipboard.writeText(remoteInfo.url).then(() => toast.success("已复制链接"), () => toast.error("复制失败"))}
                    className="shrink-0 rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
                    title="复制链接"
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </button>
                </div>
                <div className="text-xs text-muted-foreground">
                  局域网 IP <code className="text-foreground">{remoteInfo.ip}</code> · 令牌 <code className="text-foreground">{remoteInfo.token}</code>
                </div>
                {qr && (
                  <div className="mt-1 flex items-center gap-3">
                    <img src={qr} alt="扫码访问" className="h-32 w-32 rounded-md bg-white p-1" />
                    <span className="text-xs text-muted-foreground">手机扫这个二维码直接打开（同一 Wi-Fi）。</span>
                  </div>
                )}
              </div>
            ) : (
              <Button size="sm" variant="outline" onClick={onLoadRemoteAccess}>
                <RefreshCw className="h-3 w-3" /> 获取链接
              </Button>
            )
          ) : (
            <p className="m-0 text-xs text-muted-foreground">打开右上角开关以启用手机远程访问。</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
