import { useMemo } from "react";

import type { AgentRun } from "@/types";

/**
 * TeamGraph — a deterministic layered (topological) view of a Team run's Worker
 * dependency DAG, colored by live status. Unlike a force-directed graph, the
 * left-to-right layering makes the execution order and blocking relationships
 * obvious at a glance. Backed entirely by `team_get_run_detail`'s worker data.
 */

interface TeamGraphProps {
  workers: AgentRun[];
  selectedId?: string;
  onSelect?: (assignmentId: string) => void;
}

const STATUS_COLOR: Record<string, string> = {
  completed: "#10b981",
  running: "#3b82f6",
  retrying: "#f59e0b",
  queued: "#6b7280",
  pending: "#6b7280",
  awaiting_approval: "#f59e0b",
  blocked: "#f97316",
  failed: "#ef4444",
  validation_failed: "#ef4444",
  cancelled: "#6b7280",
};

const NODE_W = 172;
const NODE_H = 46;
const COL_GAP = 56;
const ROW_GAP = 22;
const PAD = 16;

export function TeamGraph({ workers, selectedId, onSelect }: TeamGraphProps) {
  const layout = useMemo(() => {
    const byAssignment = new Map(workers.map((w) => [w.assignment_id, w]));

    // Longest-path depth so a node always sits to the right of every dependency.
    const depthCache = new Map<string, number>();
    const computeDepth = (id: string, stack: Set<string>): number => {
      if (depthCache.has(id)) return depthCache.get(id)!;
      if (stack.has(id)) return 0; // defensive: ignore cycles (backend rejects them)
      stack.add(id);
      const worker = byAssignment.get(id);
      const deps = worker?.dependencies.filter((d) => byAssignment.has(d)) ?? [];
      const depth = deps.length === 0 ? 0 : 1 + Math.max(...deps.map((d) => computeDepth(d, stack)));
      stack.delete(id);
      depthCache.set(id, depth);
      return depth;
    };

    const layers = new Map<number, AgentRun[]>();
    for (const worker of workers) {
      const depth = computeDepth(worker.assignment_id, new Set());
      const list = layers.get(depth) ?? [];
      list.push(worker);
      layers.set(depth, list);
    }

    const pos = new Map<string, { x: number; y: number }>();
    let maxRows = 0;
    const maxDepth = Math.max(0, ...layers.keys());
    for (let depth = 0; depth <= maxDepth; depth++) {
      const list = layers.get(depth) ?? [];
      maxRows = Math.max(maxRows, list.length);
      list.forEach((worker, row) => {
        pos.set(worker.assignment_id, {
          x: PAD + depth * (NODE_W + COL_GAP),
          y: PAD + row * (NODE_H + ROW_GAP),
        });
      });
    }

    const edges: { from: string; to: string }[] = [];
    for (const worker of workers) {
      for (const dep of worker.dependencies) {
        if (byAssignment.has(dep)) edges.push({ from: dep, to: worker.assignment_id });
      }
    }

    const width = PAD * 2 + (maxDepth + 1) * NODE_W + maxDepth * COL_GAP;
    const height = PAD * 2 + Math.max(1, maxRows) * NODE_H + Math.max(0, maxRows - 1) * ROW_GAP;
    return { pos, edges, width, height };
  }, [workers]);

  if (workers.length === 0) return null;

  return (
    <div className="overflow-auto rounded-lg border border-border glass-surface p-2">
      <svg width={layout.width} height={layout.height} className="min-w-full">
        <defs>
          <marker id="team-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#6b7280" />
          </marker>
        </defs>
        {layout.edges.map((edge, index) => {
          const from = layout.pos.get(edge.from);
          const to = layout.pos.get(edge.to);
          if (!from || !to) return null;
          const x1 = from.x + NODE_W;
          const y1 = from.y + NODE_H / 2;
          const x2 = to.x;
          const y2 = to.y + NODE_H / 2;
          const mx = (x1 + x2) / 2;
          return (
            <path
              key={index}
              d={`M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`}
              fill="none"
              stroke="#6b7280"
              strokeOpacity={0.5}
              strokeWidth={1.5}
              markerEnd="url(#team-arrow)"
            />
          );
        })}
        {workers.map((worker) => {
          const p = layout.pos.get(worker.assignment_id);
          if (!p) return null;
          const color = STATUS_COLOR[worker.status] ?? "#6b7280";
          const isSelected = selectedId === worker.assignment_id;
          const pulse = worker.status === "running" || worker.status === "retrying";
          return (
            <g
              key={worker.id}
              transform={`translate(${p.x}, ${p.y})`}
              className="cursor-pointer"
              onClick={() => onSelect?.(worker.assignment_id)}
            >
              <rect
                width={NODE_W}
                height={NODE_H}
                rx={8}
                fill="var(--color-background, #0a0b10)"
                stroke={color}
                strokeWidth={isSelected ? 2.5 : 1.5}
              />
              <rect x={0} y={0} width={4} height={NODE_H} rx={2} fill={color}>
                {pulse && <animate attributeName="opacity" values="1;0.3;1" dur="1.2s" repeatCount="indefinite" />}
              </rect>
              <text x={12} y={19} fontSize={12} fontWeight={600} fill="var(--color-foreground, #f8fafc)">
                {truncate(`${worker.assignment_id} · ${worker.agent_name}`, 22)}
              </text>
              <text x={12} y={35} fontSize={10.5} fill={color}>
                {worker.status}
                {worker.retry_count > 0 ? ` · 重试 ${worker.retry_count}/${worker.max_retries}` : ""}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function truncate(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}
