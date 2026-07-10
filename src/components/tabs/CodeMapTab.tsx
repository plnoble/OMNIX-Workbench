/**
 * CodeMapTab — 代码地图 (Understand-Anything inspired, lightweight layer).
 *
 * Analyzes a project into a file dependency graph (via the existing
 * `architectureApi.build`, which extracts imports for JS/TS/Rust/Python/Go) and
 * renders it as an interactive d3-force graph: pan/zoom, drag, click a file to
 * inspect and open it. File-and-import level only — no agent pipeline.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpen, Loader2, Network, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/sonner";
import { cn } from "@/lib/utils";
import { architectureApi, shellApi, type ArchitectureGraph, type GraphNode } from "@/lib/tauri-api";

const LAYER_COLOR: Record<string, string> = {
  api: "#e0724a", service: "#d4a95a", data: "#5aa0d4", ui: "#8a6fd4",
  utility: "#5ac2a0", config: "#a0a0a0", test: "#d45a8a", infrastructure: "#6a8fd4", unknown: "#888",
};

interface SimNode extends GraphNode { x?: number; y?: number; fx?: number | null; fy?: number | null; degree: number; }
interface SimLink { source: string | SimNode; target: string | SimNode; }

const MAX_NODES = 400;

export function CodeMapTab() {
  const [graph, setGraph] = useState<ArchitectureGraph | null>(null);
  const [projectPath, setProjectPath] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<GraphNode | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);

  const analyze = useCallback(async (path: string) => {
    if (!path) return;
    setLoading(true);
    setSelected(null);
    try {
      setGraph(await architectureApi.build(path));
    } catch (error) {
      toast.error(`分析失败：${String(error)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  const pickProject = async () => {
    const dir = await shellApi.pickDirectory().catch(() => null);
    if (dir) { setProjectPath(dir); void analyze(dir); }
  };

  // Render the d3-force graph whenever the graph data changes.
  useEffect(() => {
    if (!graph || !svgRef.current) return;
    const svgEl = svgRef.current;
    const width = svgEl.clientWidth || 900;
    const height = svgEl.clientHeight || 600;

    // Import edges = the dependency graph; keep only file→file links.
    const fileIds = new Set(graph.nodes.filter((n) => n.node_type !== "directory").map((n) => n.id));
    const links: SimLink[] = graph.edges
      .filter((e) => e.edge_type === "imports" && fileIds.has(e.source) && fileIds.has(e.target))
      .map((e) => ({ source: e.source, target: e.target }));

    // Degree per node; keep the most-connected + largest files (cap for perf).
    const degree = new Map<string, number>();
    for (const l of links) {
      degree.set(l.source as string, (degree.get(l.source as string) ?? 0) + 1);
      degree.set(l.target as string, (degree.get(l.target as string) ?? 0) + 1);
    }
    let nodes: SimNode[] = graph.nodes
      .filter((n) => n.node_type !== "directory")
      .map((n) => ({ ...n, degree: degree.get(n.id) ?? 0 }));
    // Prefer connected nodes; cap the count.
    nodes = nodes
      .sort((a, b) => b.degree - a.degree || b.line_count - a.line_count)
      .slice(0, MAX_NODES);
    const keep = new Set(nodes.map((n) => n.id));
    const shownLinks = links.filter((l) => keep.has(l.source as string) && keep.has(l.target as string));

    const svg = d3.select(svgEl);
    svg.selectAll("*").remove();
    const root = svg.append("g");

    svg.call(
      d3.zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.1, 4])
        .on("zoom", (event) => root.attr("transform", event.transform)) as never,
    );

    const link = root.append("g")
      .attr("stroke", "currentColor").attr("stroke-opacity", 0.18)
      .selectAll("line").data(shownLinks).join("line").attr("stroke-width", 1);

    const node = root.append("g")
      .selectAll("circle").data(nodes).join("circle")
      .attr("r", (d) => 3 + Math.sqrt(d.line_count) * 0.5 + d.degree * 0.4)
      .attr("fill", (d) => LAYER_COLOR[d.layer] ?? LAYER_COLOR.unknown)
      .attr("stroke", "var(--background)").attr("stroke-width", 1)
      .style("cursor", "pointer")
      .on("click", (_event, d) => setSelected(d));
    node.append("title").text((d) => `${d.path}\n${d.line_count} 行 · ${d.layer}`);

    const sim = d3.forceSimulation<SimNode>(nodes)
      .force("link", d3.forceLink<SimNode, SimLink>(shownLinks).id((d) => d.id).distance(50).strength(0.4))
      .force("charge", d3.forceManyBody().strength(-60))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collide", d3.forceCollide<SimNode>().radius((d) => 5 + Math.sqrt(d.line_count) * 0.5))
      .on("tick", () => {
        link
          .attr("x1", (d) => (d.source as SimNode).x!).attr("y1", (d) => (d.source as SimNode).y!)
          .attr("x2", (d) => (d.target as SimNode).x!).attr("y2", (d) => (d.target as SimNode).y!);
        node.attr("cx", (d) => d.x!).attr("cy", (d) => d.y!);
      });

    node.call(
      d3.drag<SVGCircleElement, SimNode>()
        .on("start", (event, d) => { if (!event.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
        .on("drag", (event, d) => { d.fx = event.x; d.fy = event.y; })
        .on("end", (event, d) => { if (!event.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }) as never,
    );

    return () => { sim.stop(); };
  }, [graph]);

  useEffect(() => {
    // Try the most recently used chat workspace-like default: none; user picks.
  }, []);

  return (
    <div className="flex h-full flex-1 flex-col overflow-hidden bg-background">
      <div className="flex items-center justify-between gap-3 border-b border-border px-6 py-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-lg font-semibold">
            <Network className="h-5 w-5 text-primary" /> 代码地图
          </div>
          <p className="mt-1 truncate text-sm text-muted-foreground">
            {graph ? `${graph.project_name} · ${graph.stats.total_files} 文件 · ${graph.stats.node_count} 节点 / ${graph.stats.edge_count} 依赖` : "把项目分析成文件依赖图，力导图交互探索（文件+import 层，近似）。"}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {projectPath && (
            <Button size="sm" variant="outline" disabled={loading} onClick={() => void analyze(projectPath)}>
              <RefreshCw className={cn("h-4 w-4", loading && "animate-spin")} /> 重新分析
            </Button>
          )}
          <Button size="sm" onClick={() => void pickProject()} disabled={loading}>
            <FolderOpen className="h-4 w-4" /> 选择项目
          </Button>
        </div>
      </div>

      <div className="relative flex min-h-0 flex-1">
        {loading && (
          <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/70">
            <Loader2 className="h-6 w-6 animate-spin text-primary" />
          </div>
        )}
        {!graph && !loading ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
            <Network className="h-12 w-12 opacity-30" />
            <p className="text-sm">选择一个项目文件夹，生成文件依赖图。</p>
          </div>
        ) : (
          <>
            <svg ref={svgRef} className="min-h-0 flex-1 text-muted-foreground" style={{ width: "100%", height: "100%" }} />
            {selected && (
              <div className="w-72 shrink-0 overflow-y-auto border-l border-border p-4">
                <div className="mb-1 flex items-center gap-2">
                  <span className="h-3 w-3 rounded-full" style={{ background: LAYER_COLOR[selected.layer] ?? LAYER_COLOR.unknown }} />
                  <span className="truncate text-sm font-semibold" title={selected.name}>{selected.name}</span>
                </div>
                <div className="mb-3 break-all text-xs text-muted-foreground">{selected.path}</div>
                <dl className="space-y-1 text-xs">
                  <div className="flex justify-between"><dt className="text-muted-foreground">语言</dt><dd>{selected.language ?? "—"}</dd></div>
                  <div className="flex justify-between"><dt className="text-muted-foreground">层</dt><dd>{selected.layer}</dd></div>
                  <div className="flex justify-between"><dt className="text-muted-foreground">行数</dt><dd>{selected.line_count}</dd></div>
                </dl>
                <Button
                  size="sm" variant="outline" className="mt-3 w-full"
                  onClick={() => graph && void openPath(`${graph.project_path.replace(/[\\/]$/, "")}/${selected.path}`).catch((e) => toast.error(String(e)))}
                >
                  用系统应用打开
                </Button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
