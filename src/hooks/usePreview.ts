/**
 * usePreview — Live preview pane for workspace files
 *
 * 文件读取统一走 `workspaceApi.readFile`（read_workspace_file）：带目录穿越
 * 校验、认得 image/pdf/binary、有大小上限。曾经这里挂的是另外两个命令，参数
 * 名和后端对不上，读取一律失败，错误又被吞进 console，于是选中任何文件都只
 * 是一片空白。
 */

import { useState, useCallback } from "react";
import { previewApi, workspaceApi, type PreviewFileEntry } from "@/lib/tauri-api";
import type { PreviewType } from "@/types";
import { DEFAULT_PROXY_PORT } from "@/lib/constants";

export interface UsePreviewReturn {
  showPreviewPane: boolean;
  previewFiles: PreviewFileEntry[];
  selectedPreviewFile: string;
  previewType: PreviewType;
  previewHtmlUrl: string;
  previewTextContent: string;
  previewImageBase64: string;

  setShowPreviewPane: (v: boolean) => void;
  loadPreviewFiles: () => Promise<void>;
  /** 传工作区内的相对路径（`previewFiles[].relative`）。 */
  selectPreviewFile: (file: string) => Promise<void>;
  loadGitDiff: () => Promise<void>;
}

export function usePreview(chatWorkspace: string): UsePreviewReturn {
  const [showPreviewPane, setShowPreviewPane] = useState(false);
  const [previewFiles, setPreviewFiles] = useState<PreviewFileEntry[]>([]);
  const [selectedPreviewFile, setSelectedPreviewFile] = useState("");
  const [previewType, setPreviewType] = useState<PreviewType>("markdown");
  const [previewHtmlUrl, setPreviewHtmlUrl] = useState("");
  const [previewTextContent, setPreviewTextContent] = useState("");
  const [previewImageBase64, setPreviewImageBase64] = useState("");

  const loadPreviewFiles = useCallback(async () => {
    if (!chatWorkspace || chatWorkspace === "direct") return;
    try {
      setPreviewFiles(await previewApi.listFiles(chatWorkspace));
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
      setPreviewHtmlUrl(
        `http://localhost:${DEFAULT_PROXY_PORT}/preview/${encodeURIComponent(chatWorkspace)}/${encodeURIComponent(file)}`
      );
      return;
    }

    try {
      const preview = await workspaceApi.readFile(chatWorkspace, file);
      if (preview.kind === "image") {
        setPreviewType("image");
        // 后端给的是完整 data: URL，直接喂 <img src>。
        setPreviewImageBase64(preview.content);
      } else if (preview.kind === "binary") {
        setPreviewType("markdown");
        setPreviewTextContent(`（二进制文件，不做预览：${file}）`);
      } else {
        setPreviewType("markdown");
        setPreviewTextContent(preview.truncated ? `${preview.content}\n\n…（内容过长，已截断）` : preview.content);
      }
    } catch (e) {
      // 读不出来要说出来——以前只 console.error，面板留白，看上去像没反应。
      console.error("[usePreview] Failed to read file:", e);
      setPreviewType("markdown");
      setPreviewTextContent(`读取失败：${String(e)}`);
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
      setPreviewTextContent(`读取失败：${String(e)}`);
    }
  }, [chatWorkspace]);

  return {
    showPreviewPane, previewFiles, selectedPreviewFile,
    previewType, previewHtmlUrl, previewTextContent, previewImageBase64,
    setShowPreviewPane, loadPreviewFiles, selectPreviewFile, loadGitDiff,
  };
}
