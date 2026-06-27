/**
 * useSelection — Selection Assistant state management
 *
 * Manages:
 * - Selection capture (UIA + clipboard hybrid)
 * - Global shortcut registration
 * - Capture settings (shortcut, mode, show-on-capture)
 * - Selection history
 */

import { useState, useCallback } from "react";
import { selectionApi, settingsApi } from "@/lib/tauri-api";
import type { SelectionHistoryEntry } from "@/types";

export interface UseSelectionReturn {
  // Capture state
  isCapturing: boolean;
  lastCapture: string | null;
  captureError: string | null;

  // Settings
  selectionShortcut: string;
  captureMode: "hybrid" | "uia_only" | "clipboard_only";
  showOnCapture: boolean;
  preserveClipboard: boolean;
  autoCaptureEnabled: boolean;
  blacklist: string[];

  // History
  selectionHistory: SelectionHistoryEntry[];

  // Actions
  captureAndShow: () => Promise<void>;
  captureTextOnly: () => Promise<string | null>;
  loadSelectionSettings: () => Promise<void>;
  saveSelectionSettings: (updates: Partial<{
    shortcut: string;
    captureMode: "hybrid" | "uia_only" | "clipboard_only";
    showOnCapture: boolean;
    preserveClipboard: boolean;
    autoCaptureEnabled: boolean;
    blacklist: string[];
  }>) => Promise<void>;
  loadHistory: () => Promise<void>;
  deleteHistoryItem: (id: string) => Promise<void>;
  clearHistory: () => Promise<void>;
}

export function useSelection(): UseSelectionReturn {
  // ── Capture state ─────────────────────────────────
  const [isCapturing, setIsCapturing] = useState(false);
  const [lastCapture, setLastCapture] = useState<string | null>(null);
  const [captureError, setCaptureError] = useState<string | null>(null);

  // ── Settings state ────────────────────────────────
  const [selectionShortcut, setSelectionShortcut] = useState("Ctrl+Alt+C");
  const [captureMode, setCaptureMode] = useState<"hybrid" | "uia_only" | "clipboard_only">("hybrid");
  const [showOnCapture, setShowOnCapture] = useState(true);
  const [preserveClipboard, setPreserveClipboard] = useState(false);
  const [autoCaptureEnabled, setAutoCaptureEnabled] = useState(true);
  const [blacklist, setBlacklist] = useState<string[]>([]);

  // ── History state ─────────────────────────────────
  const [selectionHistory, setSelectionHistory] = useState<SelectionHistoryEntry[]>([]);

  // ── Capture actions ───────────────────────────────

  const captureAndShow = useCallback(async () => {
    setIsCapturing(true);
    setCaptureError(null);
    try {
      await selectionApi.captureAndShow();
      // The backend emits qa-preset-text → QuickAssistant handles the rest
      setLastCapture("sent-to-qa");
    } catch (e) {
      const msg = typeof e === "string" ? e : String(e);
      setCaptureError(msg);
      console.error("[useSelection] captureAndShow failed:", msg);
    } finally {
      setIsCapturing(false);
    }
  }, []);

  const captureTextOnly = useCallback(async (): Promise<string | null> => {
    setIsCapturing(true);
    setCaptureError(null);
    try {
      const text = await selectionApi.getText();
      setLastCapture(text);
      return text;
    } catch (e) {
      const msg = typeof e === "string" ? e : String(e);
      setCaptureError(msg);
      console.error("[useSelection] captureTextOnly failed:", msg);
      return null;
    } finally {
      setIsCapturing(false);
    }
  }, []);

  // ── Settings actions ──────────────────────────────

  const loadSelectionSettings = useCallback(async () => {
    try {
      const [shortcut, mode, show, preserve, autoCapture, blacklistValue] = await Promise.all([
        settingsApi.get("selection_assistant_shortcut"),
        settingsApi.get("selection_assistant_capture_mode"),
        settingsApi.get("selection_assistant_show_on_capture"),
        settingsApi.get("selection_assistant_preserve_clipboard"),
        settingsApi.get("selection_assistant_auto_capture"),
        settingsApi.get("selection_assistant_blacklist"),
      ]);
      if (shortcut) setSelectionShortcut(shortcut);
      if (mode === "uia_only" || mode === "clipboard_only" || mode === "hybrid") {
        setCaptureMode(mode);
      }
      const shouldAutoCapture = show !== "false";
      if (!shouldAutoCapture) setShowOnCapture(false);
      if (preserve === "true") setPreserveClipboard(true);
      // Default ON (always-on like Cherry Studio): unset → enabled; only an
      // explicit "false" disables it. Toggle remains in 划词助手 settings.
      const autoCaptureIsEnabled = autoCapture !== "false";
      setAutoCaptureEnabled(autoCaptureIsEnabled);
      if (blacklistValue) {
        try {
          const parsed = JSON.parse(blacklistValue);
          if (Array.isArray(parsed)) setBlacklist(parsed.filter((item): item is string => typeof item === "string"));
        } catch {
          setBlacklist(blacklistValue.split(",").map((item) => item.trim()).filter(Boolean));
        }
      }

      // Auto-capture is opt-in; showing the window after a manual capture is separate.
      if (autoCaptureIsEnabled) {
        try {
          await selectionApi.toggleAutoCapture(true);
          console.log("[useSelection] Auto-capture monitor started on app launch");
        } catch (e) {
          console.error("[useSelection] Failed to start auto-capture monitor:", e);
        }
      }
    } catch (e) {
      console.error("[useSelection] Failed to load settings:", e);
    }
  }, []);

  const saveSelectionSettings = useCallback(async (updates: Partial<{
    shortcut: string;
    captureMode: "hybrid" | "uia_only" | "clipboard_only";
    showOnCapture: boolean;
    preserveClipboard: boolean;
    autoCaptureEnabled: boolean;
    blacklist: string[];
  }>) => {
    try {
      if (updates.shortcut !== undefined) {
        await settingsApi.set("selection_assistant_shortcut", updates.shortcut);
        setSelectionShortcut(updates.shortcut);
      }
      if (updates.captureMode !== undefined) {
        await settingsApi.set("selection_assistant_capture_mode", updates.captureMode);
        setCaptureMode(updates.captureMode);
      }
      if (updates.showOnCapture !== undefined) {
        await settingsApi.set("selection_assistant_show_on_capture", String(updates.showOnCapture));
        setShowOnCapture(updates.showOnCapture);
      }
      if (updates.preserveClipboard !== undefined) {
        await settingsApi.set("selection_assistant_preserve_clipboard", String(updates.preserveClipboard));
        setPreserveClipboard(updates.preserveClipboard);
      }
      if (updates.autoCaptureEnabled !== undefined) {
        await settingsApi.set("selection_assistant_auto_capture", String(updates.autoCaptureEnabled));
        await selectionApi.toggleAutoCapture(updates.autoCaptureEnabled);
        setAutoCaptureEnabled(updates.autoCaptureEnabled);
      }
      if (updates.blacklist !== undefined) {
        const normalized = Array.from(new Set(updates.blacklist.map((item) => item.trim()).filter(Boolean)));
        await settingsApi.set("selection_assistant_blacklist", JSON.stringify(normalized));
        setBlacklist(normalized);
      }
    } catch (e) {
      console.error("[useSelection] Failed to save settings:", e);
      throw e;
    }
  }, []);

  // ── History actions ───────────────────────────────

  const loadHistory = useCallback(async () => {
    try {
      const list = await selectionApi.getHistory(50);
      setSelectionHistory(list);
    } catch (e) {
      console.error("[useSelection] Failed to load history:", e);
    }
  }, []);

  const deleteHistoryItem = useCallback(async (id: string) => {
    try {
      await selectionApi.deleteHistoryItem(id);
      setSelectionHistory((prev) => prev.filter((h) => h.id !== id));
    } catch (e) {
      console.error("[useSelection] Failed to delete history item:", e);
    }
  }, []);

  const clearHistory = useCallback(async () => {
    try {
      await selectionApi.clearHistory();
      setSelectionHistory([]);
    } catch (e) {
      console.error("[useSelection] Failed to clear history:", e);
    }
  }, []);

  return {
    isCapturing,
    lastCapture,
    captureError,
    selectionShortcut,
    captureMode,
    showOnCapture,
    preserveClipboard,
    autoCaptureEnabled,
    blacklist,
    selectionHistory,
    captureAndShow,
    captureTextOnly,
    loadSelectionSettings,
    saveSelectionSettings,
    loadHistory,
    deleteHistoryItem,
    clearHistory,
  };
}
