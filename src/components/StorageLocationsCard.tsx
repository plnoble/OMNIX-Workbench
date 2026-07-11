/**
 * StorageLocationsCard — 存储位置中心 (R1)。
 *
 * 备份/导出/技能中央库/Agent 安装目录都可以指到任意盘（比如 D:），
 * 不再默认塞满 C 盘的 ~/.omnix。技能中央库走「迁移」（搬文件+改索引）。
 */
import { useCallback, useEffect, useState } from "react";
import { FolderOpen, HardDrive, Loader2, RotateCcw } from "lucide-react";
import { toast } from "sonner";

import { shellApi, storageApi, type StorageLocation } from "@/lib/tauri-api";

export function StorageLocationsCard() {
  const [locations, setLocations] = useState<StorageLocation[]>([]);
  const [busyKey, setBusyKey] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLocations(await storageApi.getConfig());
    } catch (e) {
      toast.error(`读取存储配置失败：${e}`);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const pickAndSet = async (loc: StorageLocation) => {
    const dir = await shellApi.pickDirectory();
    if (!dir) return;
    setBusyKey(loc.key);
    try {
      if (loc.key === "storage_skills_dir") {
        if (
          !window.confirm(
            `把技能中央库从\n${loc.path}\n迁移到\n${dir}\n\n会搬移全部技能文件并更新索引，完成后旧目录删除。继续？`,
          )
        )
          return;
        const r = await storageApi.migrateSkillsStore(dir);
        toast.success(`技能库已迁移：${r.moved} 个技能 → ${r.new_dir}`);
      } else {
        await storageApi.setDir(loc.key, dir);
        toast.success(`${loc.label} 已改为 ${dir}`);
        if (loc.key === "sandbox_dir") {
          toast.info("新装的 Agent 会装到新目录；已装的不受影响，可在「智能体→安装管理」里统一重装");
        }
      }
      void load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyKey(null);
    }
  };

  const resetDefault = async (loc: StorageLocation) => {
    if (loc.key === "storage_skills_dir") {
      if (!window.confirm(`把技能中央库迁移回默认位置\n${loc.default_path}\n？`)) return;
      setBusyKey(loc.key);
      try {
        const r = await storageApi.migrateSkillsStore(loc.default_path);
        toast.success(`已迁回默认位置（${r.moved} 个技能）`);
        void load();
      } catch (e) {
        toast.error(String(e));
      } finally {
        setBusyKey(null);
      }
      return;
    }
    setBusyKey(loc.key);
    try {
      await storageApi.setDir(loc.key, "");
      toast.success(`${loc.label} 已恢复默认`);
      void load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusyKey(null);
    }
  };

  return (
    <div className="rounded-xl border border-border bg-card/40 p-5">
      <div className="flex items-center gap-2 text-sm font-semibold">
        <HardDrive className="h-4 w-4 text-primary" /> 存储位置
      </div>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">
        把会越攒越多的目录指到别的盘（如 D:），避免 C 盘越用越满。数据库与媒体库固定在 ~/.omnix。
      </p>
      <div className="mt-3 flex flex-col gap-2">
        {locations.map((loc) => (
          <div
            key={loc.key}
            className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-background/60 px-3 py-2"
          >
            <span className="w-28 shrink-0 text-sm font-medium">{loc.label}</span>
            <span
              className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground"
              title={loc.path}
            >
              {loc.path}
            </span>
            {!loc.is_default && (
              <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                自定义
              </span>
            )}
            <button
              onClick={() => void pickAndSet(loc)}
              disabled={busyKey !== null}
              className="inline-flex h-7 shrink-0 items-center gap-1 rounded-md border border-border px-2 text-xs hover:bg-muted/40 disabled:opacity-50"
            >
              {busyKey === loc.key ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <FolderOpen className="h-3 w-3" />
              )}
              {loc.key === "storage_skills_dir" ? "迁移…" : "选择…"}
            </button>
            {!loc.is_default && (
              <button
                onClick={() => void resetDefault(loc)}
                disabled={busyKey !== null}
                title="恢复默认位置"
                className="inline-flex h-7 shrink-0 items-center gap-1 rounded-md border border-border px-2 text-xs text-muted-foreground hover:bg-muted/40 disabled:opacity-50"
              >
                <RotateCcw className="h-3 w-3" /> 默认
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
