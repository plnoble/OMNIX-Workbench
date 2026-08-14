/**
 * DashboardTab — 开发环境诊断控制面板
 *
 * Shows: status overview cards, env diagnostics, software update, remote access
 */

import { useEffect, useState } from "react";
import { usePolling } from "@/hooks/usePolling";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Wifi, Cpu, Bot, Wrench, RefreshCw, Copy, Smartphone, Rocket } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { DEFAULT_PROXY_PORT } from "@/lib/constants";
import QRCode from "qrcode";
import { remoteApi, settingsApi, type RemoteClientInfo } from "@/lib/tauri-api";
import { toast } from "@/components/ui/sonner";
import type { DetectedAgent, RemoteAccessInfo } from "@/types";

interface DashboardTabProps {
  activeSessionsCount: number;
  detectedAgents: DetectedAgent[];
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
  envDiagnostics,
  repairLogs,
  repairingTool,
  remoteInfo,
  onRunDiagnostics,
  onRepairTool,
  onLoadRemoteAccess,
}: DashboardTabProps) {
  // Remote phone access: toggle LAN binding + restart proxy.
  const [remoteEnabled, setRemoteEnabled] = useState(false);
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [qr, setQr] = useState("");
  const [appVersion, setAppVersion] = useState("");
  const [remoteClients, setRemoteClients] = useState<RemoteClientInfo[]>([]);
  useEffect(() => { getVersion().then(setAppVersion).catch(() => {}); }, []);
  useEffect(() => {
    settingsApi.get("remote_access_enabled").then((v) => setRemoteEnabled(v === "true")).catch(() => {});
  }, []);
  // Poll recently connected devices while remote access is on.
  useEffect(() => {
    if (!remoteEnabled) setRemoteClients([]);
  }, [remoteEnabled]);
  usePolling(
    () => remoteApi.clients().then(setRemoteClients).catch(() => {}),
    15_000,
    remoteEnabled,
  );

  const rotateToken = async () => {
    if (!window.confirm("轮换令牌：所有已配对的手机会立刻被踢下线，需要重新扫码。确定？")) return;
    try {
      await remoteApi.rotateToken();
      onLoadRemoteAccess();
      toast.success("令牌已轮换", { description: "已配对设备全部失效，请重新扫码配对。" });
    } catch (e) {
      toast.error("轮换失败", { description: String(e) });
    }
  };
  useEffect(() => {
    if (remoteEnabled && remoteInfo?.url) {
      QRCode.toDataURL(remoteInfo.url, { width: 200, margin: 1 }).then(setQr).catch(() => setQr(""));
    } else {
      setQr("");
    }
  }, [remoteEnabled, remoteInfo?.url]);

  /**
   * 配对码会过期（5 分钟），所以屏幕上这个二维码得自己保鲜——不然用户盯着一个
   * 早就失效的码去扫，只会看到「配对已失效」。到期前 30 秒换一张。
   */
  //
  // 走 usePolling 还有一层意思：界面看不见时**不再继续发新码**。以前哪怕窗口
  // 最小化在托盘里，它也每 4 分半铸一个新的一次性配对码。
  const codeTtl = remoteInfo?.code_ttl_secs ?? 300;
  usePolling(
    () => onLoadRemoteAccess(),
    Math.max(30, codeTtl - 30) * 1000,
    remoteEnabled,
  );

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
      {/* Status Overview Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        <Card>
          <CardContent className="p-4">
            <span className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Wifi className="h-3 w-3" /> 中转代理端口
            </span>
            <span className="text-lg font-bold block mt-1">127.0.0.1:{DEFAULT_PROXY_PORT}</span>
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

      {/* token / 成本活动不在这里——它的唯一去处是「监控 → 用量」。诊断页只管
          环境是否装好、能不能连上，不重复看板。 */}

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
            <Wrench className="h-4 w-4" /> 运行依赖检查
          </CardTitle>
          <Button size="sm" onClick={onRunDiagnostics}>
            <RefreshCw className="h-3 w-3" /> 检查
          </Button>
        </CardHeader>
        <CardContent>
          {Object.keys(envDiagnostics).length === 0 ? (
            <p className="text-xs text-muted-foreground m-0">
              检查 OMNIX 自己要用的三个外部命令行工具：<code className="text-foreground">rg</code>（全文搜索 / 知识库切片）、<code className="text-foreground">git</code>（worktree / 工作区检查点 / diff）、<code className="text-foreground">node</code>（npm 系智能体的安装）。
              CLI 智能体的检测、版本、更新与安装全在「智能体」页，这里不重复。
            </p>
          ) : (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              {Object.entries(envDiagnostics).map(([tool, version]) => {
                const isInstalled = version && !version.toLowerCase().includes("not found");
                return (
                  <div
                    key={tool}
                    className="flex min-w-0 justify-between items-center gap-2 p-3 rounded-lg bg-muted/5 border border-border"
                  >
                    <div className="min-w-0">
                      <span className="text-sm font-semibold block">{tool}</span>
                      <Badge
                        variant={isInstalled ? "success" : "destructive"}
                        className="max-w-full truncate"
                        title={isInstalled ? version : "未检测到安装"}
                      >
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
          {remoteEnabled && (
            <div className="mb-3 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
              ⚠️ 高风险：服务已绑定 0.0.0.0（监听地址 {remoteInfo?.ip || "本机局域网 IP"}:{DEFAULT_PROXY_PORT}），
              同一网络内已配对的设备可访问你的会话与模型网关。仅在可信网络开启；怀疑设备丢失时立即轮换令牌。
            </div>
          )}
          {remoteEnabled ? (
            remoteInfo ? (
              <div className="flex flex-col gap-2 text-sm">
                <div className="flex items-center gap-2">
                  <span className="shrink-0 text-muted-foreground">配对链接:</span>
                  <code className="min-w-0 flex-1 break-all text-foreground">{remoteInfo.url}</code>
                  <button
                    onClick={() => navigator.clipboard.writeText(remoteInfo.url).then(() => toast.success("已复制配对链接"), () => toast.error("复制失败"))}
                    className="shrink-0 rounded p-1 text-muted-foreground hover:bg-muted/30 hover:text-foreground"
                    title="复制链接"
                  >
                    <Copy className="h-3.5 w-3.5" />
                  </button>
                </div>
                <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <span>
                    局域网 IP <code className="text-foreground">{remoteInfo.ip}</code> · 网关令牌{" "}
                    <code className="text-foreground">{remoteInfo.token.slice(0, 12)}…</code>
                  </span>
                  <Button size="sm" variant="outline" className="h-6 px-2 text-xs" onClick={() => void rotateToken()}>
                    <RefreshCw className="h-3 w-3" /> 轮换令牌
                  </Button>
                  <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" onClick={onLoadRemoteAccess}>
                    换个配对码
                  </Button>
                </div>
                {qr && (
                  <div className="mt-1 flex items-center gap-3">
                    <img src={qr} alt="扫码配对" className="h-32 w-32 rounded-md bg-white p-1" />
                    <span className="text-xs leading-5 text-muted-foreground">
                      手机扫这个二维码完成配对（同一 Wi-Fi）。
                      <br />
                      链接里是<strong>一次性配对码</strong>：{Math.round(codeTtl / 60)} 分钟内有效、扫一次即废，
                      屏幕上这张会自动换新。
                      <br />
                      配对成功后手机拿到的是一枚 HttpOnly Cookie，之后的请求不再把任何令牌放进网址里。
                    </span>
                  </div>
                )}
                <div className="mt-1">
                  <div className="mb-1 text-xs font-medium text-muted-foreground">已连接设备（本次运行内）</div>
                  {remoteClients.length === 0 ? (
                    <p className="m-0 text-xs text-muted-foreground/70">还没有设备配对成功。</p>
                  ) : (
                    <div className="flex flex-col gap-1">
                      {remoteClients.map((client) => {
                        const mins = Math.max(0, Math.round(Date.now() / 1000 - client.last_seen) / 60);
                        const ago = mins < 1 ? "刚刚" : mins < 60 ? `${Math.round(mins)} 分钟前` : `${Math.round(mins / 60)} 小时前`;
                        return (
                          <div key={client.ip} className="flex items-center gap-2 text-xs">
                            <code className="text-foreground">{client.ip}</code>
                            <span className="text-muted-foreground">{ago}活跃</span>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </div>
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
