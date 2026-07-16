/**
 * ExcelTab — 表格工作台（P2）。
 *
 * 与演示（结构化 JSON）不同，表格天然以真实 .xlsx 文件为源：所有读写都通过
 * OfficeCLI（公式即时求值），AI 指令被翻成受校验的批量命令后才落盘，预览是
 * 同一文件的确定性 HTML 渲染。也兼任 docx/pptx 的只读统一预览（工作区产物）。
 */
import { useCallback, useEffect, useState } from "react";
import { FileSpreadsheet, FileUp, Loader2, Plus, RefreshCw, Sparkles, Table2 } from "lucide-react";
import { toast } from "sonner";

import { officeApi, modelApi, settingsApi } from "@/lib/tauri-api";
import { shellApi } from "@/lib/tauri-api";
import type { PlatformModel } from "@/types";

/** Office 概览页的「近期表格」数据源：最近打开/新建的文件路径（去重，留 8 条）。 */
async function recordRecentOfficeFile(path: string) {
  try {
    const raw = await settingsApi.get("office_recent_files");
    const list: string[] = raw ? JSON.parse(raw) : [];
    const next = [path, ...list.filter((p) => p !== path)].slice(0, 8);
    await settingsApi.set("office_recent_files", JSON.stringify(next));
  } catch {
    /* best-effort */
  }
}

export function ExcelTab() {
  const [filePath, setFilePath] = useState("");
  const [previewHtml, setPreviewHtml] = useState("");
  const [previewOnly, setPreviewOnly] = useState(false);
  const [instruction, setInstruction] = useState("");
  const [models, setModels] = useState<PlatformModel[]>([]);
  const [chatModel, setChatModel] = useState("");
  const [busy, setBusy] = useState<"" | "open" | "ai" | "csv">("");

  useEffect(() => {
    modelApi
      .getActive()
      .then((list) => {
        const usable = list.filter(
          (m) =>
            !m.model_name.toLowerCase().includes("embedding") &&
            !m.model_name.toLowerCase().includes("rerank"),
        );
        setModels(usable);
        if (usable.length > 0) setChatModel(`${usable[0].platform_id}:${usable[0].model_name}`);
      })
      .catch(() => {});
  }, []);

  const refresh = useCallback(async (path: string) => {
    try {
      setPreviewHtml(await officeApi.previewHtml(path));
    } catch (e) {
      toast.error(`预览失败：${e}`);
    }
  }, []);

  const openFile = async () => {
    setBusy("open");
    try {
      const path = await shellApi.pickFile();
      if (!path) return;
      const lower = path.toLowerCase();
      if (!(lower.endsWith(".xlsx") || lower.endsWith(".docx") || lower.endsWith(".pptx"))) {
        toast.error("支持 .xlsx（可编辑）以及 .docx / .pptx（只读预览）");
        return;
      }
      setFilePath(path);
      setPreviewOnly(!lower.endsWith(".xlsx"));
      void recordRecentOfficeFile(path);
      await refresh(path);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy("");
    }
  };

  const newWorkbook = async () => {
    const title = window.prompt("新工作簿名称");
    if (!title?.trim()) return;
    try {
      const path = await officeApi.excelNew(title.trim());
      setFilePath(path);
      setPreviewOnly(false);
      void recordRecentOfficeFile(path);
      await refresh(path);
      toast.success(`已创建：${path}`);
    } catch (e) {
      toast.error(String(e));
    }
  };

  const runAi = async () => {
    if (!filePath || previewOnly) return;
    if (!chatModel) {
      toast.error("请先在「模型中心」启用一个对话模型");
      return;
    }
    if (!instruction.trim()) {
      toast.error("先输入指令，例如：B 列求和到 B10，并把表头加粗");
      return;
    }
    setBusy("ai");
    try {
      const report = await officeApi.excelAiEdit(filePath, instruction.trim(), chatModel);
      setInstruction("");
      await refresh(filePath);
      toast.success("已执行", { description: report.split("\n").pop() });
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy("");
    }
  };

  const importCsv = async () => {
    if (!filePath || previewOnly) return;
    setBusy("csv");
    try {
      const csv = await shellApi.pickFile();
      if (!csv) return;
      if (!/\.(csv|tsv)$/i.test(csv)) {
        toast.error("请选择 .csv / .tsv 文件");
        return;
      }
      await officeApi.excelImportCsv(filePath, csv);
      await refresh(filePath);
      toast.success("CSV 已导入 Sheet1");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy("");
    }
  };

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex items-center gap-2 border-b border-border px-6 py-4">
        <Table2 className="h-5 w-5 text-primary" />
        <div>
          <div className="text-lg font-semibold">表格</div>
          <p className="text-xs text-muted-foreground">
            真实 .xlsx 为源，AI 指令 → 受校验的批量操作 → 公式即时求值；也可只读预览 docx / pptx。
          </p>
        </div>
        <div className="ml-auto flex items-center gap-1.5">
          {models.length > 0 && (
            <select
              className="h-8 max-w-44 rounded-md border border-border bg-background px-1.5 text-xs"
              value={chatModel}
              onChange={(e) => setChatModel(e.target.value)}
            >
              {models.map((m) => (
                <option key={`${m.platform_id}:${m.model_name}`} value={`${m.platform_id}:${m.model_name}`}>
                  {m.model_name}
                </option>
              ))}
            </select>
          )}
          <button
            onClick={() => void newWorkbook()}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs hover:bg-muted/40"
          >
            <Plus className="h-3.5 w-3.5" /> 新建
          </button>
          <button
            onClick={() => void openFile()}
            disabled={busy === "open"}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs hover:bg-muted/40"
          >
            <FileSpreadsheet className="h-3.5 w-3.5" /> 打开
          </button>
          {filePath && !previewOnly && (
            <button
              onClick={() => void importCsv()}
              disabled={busy === "csv"}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs hover:bg-muted/40"
            >
              <FileUp className="h-3.5 w-3.5" /> CSV
            </button>
          )}
          {filePath && (
            <button
              onClick={() => void refresh(filePath)}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs hover:bg-muted/40"
              title="重新渲染预览"
            >
              <RefreshCw className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* AI instruction bar */}
      {filePath && !previewOnly && (
        <div className="flex items-center gap-2 border-b border-border px-6 py-2">
          <Sparkles className="h-4 w-4 shrink-0 text-accent" />
          <input
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) void runAi();
            }}
            placeholder='对这份表格下指令，例如："加一列毛利率 = (C-B)/C，百分比显示，表头加粗"'
            className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 text-sm focus:outline-none"
          />
          <button
            onClick={() => void runAi()}
            disabled={busy === "ai"}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-4 text-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {busy === "ai" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
            执行
          </button>
        </div>
      )}

      <div className="min-h-0 flex-1 p-4">
        {filePath ? (
          <div className="flex h-full flex-col gap-2">
            <div className="truncate text-xs text-muted-foreground" title={filePath}>
              {previewOnly ? "只读预览：" : ""}
              {filePath}
            </div>
            <iframe
              title="office-preview"
              className="min-h-0 flex-1 rounded-xl border border-border bg-white"
              sandbox=""
              srcDoc={previewHtml}
            />
          </div>
        ) : (
          <div className="flex h-full items-center justify-center rounded-xl border border-dashed border-border text-sm text-muted-foreground">
            新建一个工作簿，或打开 .xlsx（可编辑）/ .docx / .pptx（只读预览）
          </div>
        )}
      </div>
    </div>
  );
}
