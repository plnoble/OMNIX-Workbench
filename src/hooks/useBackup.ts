/**
 * useBackup — Data backup and restore operations
 *
 * Manages: table info, export/import, selected tables for selective backup
 */

import { useState, useCallback } from "react";
import { backupApi } from "@/lib/tauri-api";
import type { BackupTableInfo, ImportResult } from "@/types";

export interface UseBackupReturn {
  tableInfo: BackupTableInfo[];
  selectedTables: Set<string>;
  isExporting: boolean;
  isImporting: boolean;
  lastImportResult: ImportResult | null;

  loadBackupInfo: () => Promise<void>;
  toggleTableSelection: (tableName: string) => void;
  selectAllTables: () => void;
  deselectAllTables: () => void;
  exportBackup: () => Promise<string | null>;
  importBackup: (jsonStr: string) => Promise<ImportResult | null>;
}

export function useBackup(): UseBackupReturn {
  const [tableInfo, setTableInfo] = useState<BackupTableInfo[]>([]);
  const [selectedTables, setSelectedTables] = useState<Set<string>>(new Set());
  const [isExporting, setIsExporting] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [lastImportResult, setLastImportResult] = useState<ImportResult | null>(null);

  const loadBackupInfo = useCallback(async () => {
    try {
      const info = await backupApi.getInfo();
      setTableInfo(info);
      // Default: select all tables
      setSelectedTables(new Set(info.map((t) => t.table_name)));
    } catch (e) {
      console.error("[useBackup] Failed to load backup info:", e);
    }
  }, []);

  const toggleTableSelection = useCallback((tableName: string) => {
    setSelectedTables((prev) => {
      const next = new Set(prev);
      if (next.has(tableName)) next.delete(tableName);
      else next.add(tableName);
      return next;
    });
  }, []);

  const selectAllTables = useCallback(() => {
    setSelectedTables(new Set(tableInfo.map((t) => t.table_name)));
  }, [tableInfo]);

  const deselectAllTables = useCallback(() => {
    setSelectedTables(new Set());
  }, []);

  const exportBackup = useCallback(async (): Promise<string | null> => {
    setIsExporting(true);
    try {
      const tables = selectedTables.size > 0 ? Array.from(selectedTables) : undefined;
      const json = await backupApi.exportBackup(tables);
      return json;
    } catch (e) {
      console.error("[useBackup] Export failed:", e);
      return null;
    } finally {
      setIsExporting(false);
    }
  }, [selectedTables]);

  const importBackup = useCallback(async (jsonStr: string): Promise<ImportResult | null> => {
    setIsImporting(true);
    try {
      const tables = selectedTables.size > 0 ? Array.from(selectedTables) : undefined;
      const result = await backupApi.importBackup(jsonStr, tables);
      setLastImportResult(result);
      return result;
    } catch (e) {
      console.error("[useBackup] Import failed:", e);
      return null;
    } finally {
      setIsImporting(false);
    }
  }, [selectedTables]);

  return {
    tableInfo, selectedTables, isExporting, isImporting, lastImportResult,
    loadBackupInfo, toggleTableSelection, selectAllTables, deselectAllTables,
    exportBackup, importBackup,
  };
}
