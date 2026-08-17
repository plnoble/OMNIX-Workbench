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

export interface UsePreviewReturn {
  showPreviewPane: boolean;
  previewFiles: PreviewFileEntry[];
  selectedPreviewFile: string;
  previewType: PreviewType;
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
    setPreviewImageBase64("");


    try {
      const preview = await workspaceApi.readFile(chatWorkspace, file);
      if (ext === "html" && preview.kind === "text") {
        // HTML 走**源码 + 沙箱 iframe**，不走 URL。
        //
        // 这里以前把 iframe 的 src 指向 `http://localhost:1421/preview/<ws>/<file>`
        // ——而网关**根本没有这条路由，也没有 fallback**，点开任何 .html 都是 404
        // 白屏。（HTTP 路由版的「幽灵调用」，commandWiring 那三道守卫只覆盖 Tauri
        // IPC，抓不到这类。）
        //
        // 补路由是另一种修法，但更危险：网关开了手机远程访问就绑 0.0.0.0，从网关
        // origin 提供 agent 生成的 HTML，会让那份 HTML 和 `/remote` **同源**——
        // 它可以带着面板会话 Cookie 去打 `/api/remote/*`。改用 readFile（那条命令
        // 的路径穿越已经硬化过）+ srcDoc + sandbox，既不新开 HTTP 面，也把脚本
        // 关在盒子里。
        //
        // 代价：HTML 里的相对资源引用（`<img src="./a.png">`）解析不了。生成类
        // 报告基本是自包含的，这个取舍划算。
        setPreviewType("html");
        setPreviewTextContent(
          preview.truncated ? `${preview.content}\n<!-- 内容过长，已截断 -->` : preview.content,
        );
      } else if (preview.kind === "image") {
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
    previewType, previewTextContent, previewImageBase64,
    setShowPreviewPane, loadPreviewFiles, selectPreviewFile, loadGitDiff,
  };
}
