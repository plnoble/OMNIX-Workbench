/** Split from SettingsTab.tsx — pure move, no behavior change. */
import { useState } from "react";
import { useBackupStore } from "@/store/AppStore";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import { Download, Upload } from "lucide-react";
import { toast } from "@/components/ui/sonner";
import { StorageLocationsCard } from "@/components/StorageLocationsCard";

export function BackupSubTab() {
  const backup = useBackupStore();
  const {
    tableInfo: backupTableInfo,
    selectedTables: backupSelectedTables,
    isExporting: isBackupExporting,
    isImporting: isBackupImporting,
    lastImportResult,
    toggleTableSelection: onToggleBackupTable,
    selectAllTables: onSelectAllBackupTables,
    deselectAllTables: onDeselectAllBackupTables,
    exportBackup: onExportBackup,
    importBackup: onImportBackup,
  } = backup;
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
      {/* Storage locations (R1 存储位置中心) */}
      <StorageLocationsCard />

      {/* Export */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm flex items-center gap-2">
            <Download className="h-4 w-4" /> 📦 数据导出
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex justify-between items-center mb-1">
            <span className="text-xs text-muted-foreground">选择要导出的数据表（不含 API Key / OAuth 令牌，恢复后需在「模型」和「授权中心」重填）：</span>
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

