/**
 * useTranslation — Translation feature state management
 *
 * Manages:
 * - Bidirectional translation (preferred ↔ alter language)
 * - Language detection via LLM
 * - Translation settings persistence
 * - Translation history
 */

import { useState, useCallback } from "react";
import { translationApi, settingsApi } from "@/lib/tauri-api";
import {
  BUILTIN_LANGUAGES,
  TRANSLATE_PROMPT,
  pickBidirectionalTarget,
  getLanguageByCode,
} from "@/lib/translate-constants";
import type { TranslateHistoryEntry } from "@/types";

export interface UseTranslationReturn {
  // Translation state
  isTranslating: boolean;
  translatedText: string;
  detectedLang: string;
  translateError: string | null;

  // Settings
  preferredLang: string;
  alterLang: string;
  translateModel: string;
  autoDetect: boolean;
  customPrompt: string;

  // Languages
  languages: typeof BUILTIN_LANGUAGES;

  // History
  translationHistory: TranslateHistoryEntry[];

  // Actions
  translate: (text: string, targetLang?: string) => Promise<string | null>;
  detectLanguage: (text: string) => Promise<string>;
  loadTranslationSettings: () => Promise<void>;
  saveTranslationSettings: (updates: Partial<{
    preferredLang: string;
    alterLang: string;
    translateModel: string;
    autoDetect: boolean;
    customPrompt: string;
  }>) => Promise<void>;
  loadHistory: () => Promise<void>;
  deleteHistoryItem: (id: string) => Promise<void>;
  clearHistory: () => Promise<void>;
}

export function useTranslation(): UseTranslationReturn {
  // ── Translation state ─────────────────────────────
  const [isTranslating, setIsTranslating] = useState(false);
  const [translatedText, setTranslatedText] = useState("");
  const [detectedLang, setDetectedLang] = useState("unknown");
  const [translateError, setTranslateError] = useState<string | null>(null);

  // ── Settings state ────────────────────────────────
  const [preferredLang, setPreferredLang] = useState("zh-cn");
  const [alterLang, setAlterLang] = useState("en-us");
  const [translateModel, setTranslateModel] = useState("");
  const [autoDetect, setAutoDetect] = useState(true);
  const [customPrompt, setCustomPrompt] = useState("");

  // ── History state ─────────────────────────────────
  const [translationHistory, setTranslationHistory] = useState<TranslateHistoryEntry[]>([]);

  // ── Translate ──────────────────────────────────────

  const translate = useCallback(async (
    text: string,
    explicitTarget?: string,
  ): Promise<string | null> => {
    if (!text.trim()) return null;
    setIsTranslating(true);
    setTranslateError(null);
    setTranslatedText("");

    try {
      // 1. Detect source language if auto-detect is on
      let sourceLang = "unknown";
      if (autoDetect) {
        sourceLang = await detectLanguageInternal(text);
        setDetectedLang(sourceLang);
      }

      // 2. Determine target language (bidirectional smart swap)
      const targetLang = explicitTarget || pickBidirectionalTarget(
        sourceLang,
        preferredLang,
        alterLang,
      );

      // 3. Build the prompt
      const targetLangName = getLanguageByCode(targetLang)?.value || targetLang;
      const promptTemplate = customPrompt || TRANSLATE_PROMPT;
      const prompt = promptTemplate
        .replace(/\{\{target_language\}\}/g, targetLangName)
        .replace(/\{\{text\}\}/g, text);

      // 4. Call the backend translate command
      const result = await translationApi.translate({
        text,
        targetLang,
        sourceLang,
        chatModel: translateModel || undefined,
        prompt,
      });

      setTranslatedText(result.translatedText);
      return result.translatedText;
    } catch (e) {
      const msg = typeof e === "string" ? e : String(e);
      setTranslateError(msg);
      console.error("[useTranslation] Translate failed:", msg);
      return null;
    } finally {
      setIsTranslating(false);
    }
  }, [preferredLang, alterLang, translateModel, autoDetect, customPrompt]);

  // ── Language Detection ────────────────────────────

  const detectLanguageInternal = useCallback(async (text: string): Promise<string> => {
    try {
      const lang = await translationApi.detectLanguage({
        text,
        chatModel: translateModel || undefined,
      });
      return lang || "unknown";
    } catch {
      // Fallback: simple heuristic for CJK detection
      const cjk = text.match(/[一-鿿]/g);
      const hiragana = text.match(/[぀-ゟ]/g);
      const hangul = text.match(/[가-힯]/g);
      const total = text.replace(/\s/g, "").length || 1;
      if (cjk && cjk.length / total > 0.3) return "zh-cn";
      if (hiragana && hiragana.length / total > 0.1) return "ja-jp";
      if (hangul && hangul.length / total > 0.1) return "ko-kr";
      return "unknown";
    }
  }, [translateModel]);

  const detectLanguage = useCallback(async (text: string): Promise<string> => {
    const lang = await detectLanguageInternal(text);
    setDetectedLang(lang);
    return lang;
  }, [detectLanguageInternal]);

  // ── Settings ───────────────────────────────────────

  const loadTranslationSettings = useCallback(async () => {
    try {
      const [pref, alt, model, detect, prompt] = await Promise.all([
        settingsApi.get("translate_preferred_lang"),
        settingsApi.get("translate_alter_lang"),
        settingsApi.get("translate_model"),
        settingsApi.get("translate_auto_detect"),
        settingsApi.get("translate_prompt"),
      ]);
      if (pref) setPreferredLang(pref);
      if (alt) setAlterLang(alt);
      if (model) setTranslateModel(model);
      if (detect === "false") setAutoDetect(false);
      if (prompt) setCustomPrompt(prompt);
    } catch (e) {
      console.error("[useTranslation] Failed to load settings:", e);
    }
  }, []);

  const saveTranslationSettings = useCallback(async (updates: Partial<{
    preferredLang: string;
    alterLang: string;
    translateModel: string;
    autoDetect: boolean;
    customPrompt: string;
  }>) => {
    try {
      if (updates.preferredLang !== undefined) {
        await settingsApi.set("translate_preferred_lang", updates.preferredLang);
        setPreferredLang(updates.preferredLang);
      }
      if (updates.alterLang !== undefined) {
        await settingsApi.set("translate_alter_lang", updates.alterLang);
        setAlterLang(updates.alterLang);
      }
      if (updates.translateModel !== undefined) {
        await settingsApi.set("translate_model", updates.translateModel);
        setTranslateModel(updates.translateModel);
      }
      if (updates.autoDetect !== undefined) {
        await settingsApi.set("translate_auto_detect", String(updates.autoDetect));
        setAutoDetect(updates.autoDetect);
      }
      if (updates.customPrompt !== undefined) {
        await settingsApi.set("translate_prompt", updates.customPrompt);
        setCustomPrompt(updates.customPrompt);
      }
    } catch (e) {
      console.error("[useTranslation] Failed to save settings:", e);
      throw e;
    }
  }, []);

  // ── History ────────────────────────────────────────

  const loadHistory = useCallback(async () => {
    try {
      const list = await translationApi.getHistory(50);
      setTranslationHistory(list);
    } catch (e) {
      console.error("[useTranslation] Failed to load history:", e);
    }
  }, []);

  const deleteHistoryItem = useCallback(async (id: string) => {
    try {
      await translationApi.deleteHistoryItem(id);
      setTranslationHistory((prev) => prev.filter((h) => h.id !== id));
    } catch (e) {
      console.error("[useTranslation] Failed to delete history item:", e);
    }
  }, []);

  const clearHistory = useCallback(async () => {
    try {
      await translationApi.clearHistory();
      setTranslationHistory([]);
    } catch (e) {
      console.error("[useTranslation] Failed to clear history:", e);
    }
  }, []);

  return {
    isTranslating,
    translatedText,
    detectedLang,
    translateError,
    preferredLang,
    alterLang,
    translateModel,
    autoDetect,
    customPrompt,
    languages: BUILTIN_LANGUAGES,
    translationHistory,
    translate,
    detectLanguage,
    loadTranslationSettings,
    saveTranslationSettings,
    loadHistory,
    deleteHistoryItem,
    clearHistory,
  };
}
