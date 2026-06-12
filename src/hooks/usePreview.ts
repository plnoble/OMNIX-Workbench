/**
 * usePreview — Live preview pane for workspace files
 */

import { useState, useCallback } from "react";
import { previewApi } from "@/lib/tauri-api";
import type { PreviewType } from "@/types";
import { DEFAULT_PROXY_PORT } from "@/lib/constants";

export interface UsePreviewReturn {
  showPreviewPane: boolean;
  previewFiles: string[];
  selectedPreviewFile: string;
  previewType: PreviewType;
  previewHtmlUrl: string;
  previewTextContent: string;
  previewImageBase64: string;

  setShowPreviewPane: (v: boolean) => void;
  loadPreviewFiles: () => Promise<void>;
  selectPreviewFile: (file: string) => Promise<void>;
  loadGitDiff: () => Promise<void>;
}

export function usePreview(chatWorkspace: string): UsePreviewReturn {
  const [showPreviewPane, setShowPreviewPane] = useState(false);
  const [previewFiles, setPreviewFiles] = useState<string[]>([]);
  const [selectedPreviewFile, setSelectedPreviewFile] = useState("");
  const [previewType, setPreviewType] = useState<PreviewType>("markdown");
  const [previewHtmlUrl, setPreviewHtmlUrl] = useState("");
  const [previewTextContent, setPreviewTextContent] = useState("");
  const [previewImageBase64, setPreviewImageBase64] = useState("");

  const loadPreviewFiles = useCallback(async () => {
    if (!chatWorkspace || chatWorkspace === "direct") return;
    try {
      const files = await previewApi.listFiles(chatWorkspace);
      setPreviewFiles(files);
    } catch (e) {
      console.error("[usePreview] Failed to load files:", e);
    }
  }, [chatWorkspace]);

  const selectPreviewFile = useCallback(async (file: string) => {
    setSelectedPreviewFile(file);
    const ext = file.split(".").pop()?.toLowerCase();

    // Reset contents
    setPreviewTextContent("");
    setPreviewHtmlUrl("");
    setPreviewImageBase64("");

    if (ext === "html") {
      setPreviewType("html");
      const url = `http://localhost:${DEFAULT_PROXY_PORT}/preview/${encodeURIComponent(chatWorkspace)}/${encodeURIComponent(file)}`;
      setPreviewHtmlUrl(url);
    } else if (ext && ["png", "jpg", "jpeg", "gif", "svg"].includes(ext)) {
      setPreviewType("image");
      try {
        const base64 = await previewApi.readFileAsBase64({ workspacePath: chatWorkspace, fileName: file });
        setPreviewImageBase64(base64);
      } catch (e) {
        console.error("[usePreview] Failed to read image:", e);
      }
    } else {
      setPreviewType("markdown");
      try {
        const text = await previewApi.readFileContent({ workspacePath: chatWorkspace, fileName: file });
        setPreviewTextContent(text);
      } catch (e) {
        console.error("[usePreview] Failed to read file:", e);
      }
    }
  }, [chatWorkspace]);

  const loadGitDiff = useCallback(async () => {
    if (!chatWorkspace || chatWorkspace === "direct") return;
    setSelectedPreviewFile("Git Diff");
    setPreviewType("diff");
    try {
      const diffText = await previewApi.getGitDiff(chatWorkspace);
      setPreviewTextContent(diffText);
    } catch (e) {
      console.error("[usePreview] Failed to get git diff:", e);
    }
  }, [chatWorkspace]);

  return {
    showPreviewPane, previewFiles, selectedPreviewFile,
    previewType, previewHtmlUrl, previewTextContent, previewImageBase64,
    setShowPreviewPane, loadPreviewFiles, selectPreviewFile, loadGitDiff,
  };
}
