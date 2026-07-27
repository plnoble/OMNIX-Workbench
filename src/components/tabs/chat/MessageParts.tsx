/** Split from ChatTab.tsx — pure move, no behavior change. */
import { useEffect, useMemo, useState } from "react";
import { Brain, ChevronDown, ChevronRight } from "lucide-react";
import { DecisionBlock } from "@/components/DecisionBlock";
import { parseDecisionParts } from "@/lib/decisionBlock";
import type { DecisionSpec } from "@/lib/decisionBlock";
import { mediaApi } from "@/lib/tauri-api";

export function AttachmentStrip({ metadataJson }: { metadataJson?: string | null }) {
  const meta = useMemo(() => {
    if (!metadataJson) return null;
    try {
      return JSON.parse(metadataJson) as {
        attachment_previews?: string[];
        attachments?: string[];
      };
    } catch {
      return null;
    }
  }, [metadataJson]);
  const [loaded, setLoaded] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    const paths = meta?.attachments;
    if (!paths || paths.length === 0) {
      setLoaded([]);
      return;
    }
    Promise.all(paths.map((path) => mediaApi.readAttachment(path).catch(() => "")))
      .then((urls) => {
        if (!cancelled) setLoaded(urls.filter(Boolean));
      });
    return () => { cancelled = true; };
  }, [meta]);

  const previews = meta?.attachment_previews?.length ? meta.attachment_previews : loaded;
  if (!previews || previews.length === 0) return null;
  return (
    <div className="mb-2 flex flex-wrap gap-2">
      {previews.map((src, index) => (
        <img
          key={index}
          src={src}
          alt={`附件 ${index + 1}`}
          className="max-h-40 max-w-48 rounded-md border border-border object-contain"
        />
      ))}
    </div>
  );
}

export function MessageContent({
  content,
  onDecide,
}: {
  content: string;
  onDecide?: (spec: DecisionSpec, chosen: string[], note: string) => void;
}) {
  const parts: Array<{ type: "text" | "think"; content: string }> = [];
  let remaining = content;

  while (remaining.length > 0) {
    const thinkStart = remaining.indexOf("<think>");
    if (thinkStart === -1) {
      parts.push({ type: "text", content: remaining });
      break;
    }
    if (thinkStart > 0) parts.push({ type: "text", content: remaining.slice(0, thinkStart) });
    const thinkEnd = remaining.indexOf("</think>", thinkStart);
    if (thinkEnd === -1) {
      parts.push({ type: "think", content: remaining.slice(thinkStart + 7) });
      break;
    }
    parts.push({ type: "think", content: remaining.slice(thinkStart + 7, thinkEnd) });
    remaining = remaining.slice(thinkEnd + 8);
  }

  return (
    <>
      {parts.map((part, index) => {
        if (part.type === "think") return <ThinkBlock key={index} content={part.content} />;
        // 方案抉择框 (#2): render omnix-decision fences as selectable cards.
        return parseDecisionParts(part.content).map((sub, subIndex) =>
          sub.type === "decision" ? (
            <DecisionBlock
              key={`${index}-${subIndex}`}
              spec={sub.spec}
              onDecide={onDecide ? (chosen, note) => onDecide(sub.spec, chosen, note) : undefined}
            />
          ) : (
            <span key={`${index}-${subIndex}`}>{sub.content}</span>
          ),
        );
      })}
    </>
  );
}

export function ThinkBlock({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const trimmed = content.trim();
  if (!trimmed) return null;

  return (
    <div className="my-2 rounded-md border border-primary/20 bg-primary/5">
      <button
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs text-primary"
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        <Brain className="h-3 w-3" />
        推理过程
        <span className="text-primary/60">{trimmed.length} 字符</span>
      </button>
      {expanded && <pre className="px-3 pb-3 text-xs whitespace-pre-wrap text-primary/80">{trimmed}</pre>}
    </div>
  );
}

