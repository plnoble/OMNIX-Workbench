/**
 * UpdateManager — in-app software updates (tauri-plugin-updater).
 *
 * Mounted once at the app root. On startup it silently checks the GitHub-hosted
 * update manifest; when a newer signed release exists it shows an update dialog
 * (notes + download progress + "update & restart"). "Later" defers to a small
 * floating pill so the reminder persists without nagging. A manual check is
 * triggered from anywhere via `window.dispatchEvent(new Event("omnix:check-updates"))`.
 */
import { useCallback, useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, Loader2, Rocket, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";

const DISMISS_KEY = "omnix_update_dismissed_version";

export function UpdateManager() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [showDialog, setShowDialog] = useState(false);
  const [deferred, setDeferred] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);

  const runCheck = useCallback(async (manual: boolean) => {
    try {
      const found = await check();
      if (found) {
        setUpdate(found);
        const skipped = localStorage.getItem(DISMISS_KEY);
        if (manual || skipped !== found.version) {
          setShowDialog(true);
          setDeferred(false);
        } else {
          setDeferred(true); // previously deferred this version → show pill only
        }
      } else {
        setUpdate(null);
        if (manual) toast.success("已是最新版本");
      }
    } catch (error) {
      // Startup checks fail silently (offline / no release yet); manual reports.
      if (manual) toast.error(`检查更新失败：${String(error)}`);
    }
  }, []);

  useEffect(() => { void runCheck(false); }, [runCheck]);

  useEffect(() => {
    const handler = () => void runCheck(true);
    window.addEventListener("omnix:check-updates", handler);
    return () => window.removeEventListener("omnix:check-updates", handler);
  }, [runCheck]);

  const install = async () => {
    if (!update) return;
    setInstalling(true);
    setProgress(0);
    try {
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setProgress(total ? Math.round((downloaded / total) * 100) : null);
            break;
          case "Finished":
            setProgress(100);
            break;
        }
      });
      toast.success("更新已安装，即将重启…");
      await relaunch();
    } catch (error) {
      toast.error(`更新失败：${String(error)}`);
      setInstalling(false);
    }
  };

  const later = () => {
    if (update) localStorage.setItem(DISMISS_KEY, update.version);
    setShowDialog(false);
    setDeferred(true);
  };

  if (!update) return null;

  return (
    <>
      {/* Deferred reminder pill */}
      {deferred && !showDialog && (
        <button
          onClick={() => setShowDialog(true)}
          className="fixed bottom-4 right-4 z-[900] flex items-center gap-2 rounded-full border border-primary/40 bg-primary/10 px-3 py-1.5 text-xs font-medium text-primary shadow-lg backdrop-blur hover:bg-primary/20"
        >
          <Rocket className="h-3.5 w-3.5" /> 新版本 v{update.version} 可更新
        </button>
      )}

      {/* Update dialog */}
      {showDialog && (
        <div className="fixed inset-0 z-[1000] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
          <div className="w-full max-w-md rounded-lg border border-border glass-surface p-5 shadow-xl">
            <div className="mb-2 flex items-center gap-2">
              <Rocket className="h-5 w-5 text-primary" />
              <h3 className="m-0 text-base font-semibold text-foreground">发现新版本 v{update.version}</h3>
            </div>
            <p className="mb-2 text-xs text-muted-foreground">
              当前 v{update.currentVersion}{update.date ? ` · 发布于 ${update.date.slice(0, 10)}` : ""}
            </p>
            {update.body && (
              <div className="mb-4 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-muted/10 p-3 text-xs leading-5 text-muted-foreground">
                {update.body}
              </div>
            )}

            {installing && (
              <div className="mb-4">
                <div className="mb-1 flex items-center justify-between text-xs text-muted-foreground">
                  <span className="flex items-center gap-1.5"><Loader2 className="h-3.5 w-3.5 animate-spin" /> 下载中…</span>
                  <span>{progress ?? 0}%</span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted/30">
                  <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress ?? 0}%` }} />
                </div>
              </div>
            )}

            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" disabled={installing} onClick={later}>
                <X className="h-4 w-4" /> 稍后
              </Button>
              <Button size="sm" disabled={installing} onClick={() => void install()}>
                <Download className="h-4 w-4" /> 立即更新并重启
              </Button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
