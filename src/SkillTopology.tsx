import React, { useEffect, useRef } from "react";
import * as d3 from "d3";

export interface SkillNode {
  name: string;
  category: string;
  is_active: boolean;
  profile: string;
}

export interface SkillDependency {
  source: string;
  target: string;
}

interface SkillTopologyProps {
  skills: {
    name: string;
    description: string;
    profile: string;
    is_active: boolean;
    dependencies: string[];
  }[];
  selectedSkill: string | null;
  onSelectSkill: (name: string) => void;
}

const getCategory = (name: string) => {
  switch (name) {
    case "file_reader":
    case "file_writer":
      return "文件操作";
    case "git_manager":
      return "版本控制";
    case "code_reviewer":
    case "ast_analyzer":
      return "静态分析";
    case "hybrid_searcher":
      return "智能检索";
    default:
      return "自定义技能";
  }
};

const getCategoryColor = (category: string) => {
  switch (category) {
    case "文件操作":
      return "#06b6d4"; // Cyan
    case "版本控制":
      return "#10b981"; // Green
    case "静态分析":
      return "#f59e0b"; // Orange
    case "智能检索":
      return "#8b5cf6"; // Purple
    default:
      return "#ec4899"; // Pink
  }
};

export const SkillTopology: React.FC<SkillTopologyProps> = ({
  skills,
  selectedSkill,
  onSelectSkill,
}) => {
  const svgRef = useRef<SVGSVGElement | null>(null);

  useEffect(() => {
    if (!svgRef.current || skills.length === 0) return;

    // Clear previous drawing
    const svgElement = d3.select(svgRef.current);
    svgElement.selectAll("*").remove();

    const width = svgRef.current.clientWidth || 400;
    const height = svgRef.current.clientHeight || 300;

    // Define marker for arrowheads
    svgElement
      .append("defs")
      .append("marker")
      .attr("id", "arrowhead")
      .attr("viewBox", "0 -5 10 10")
      .attr("refX", 22) // Positioning the arrow slightly outside the node
      .attr("refY", 0)
      .attr("orient", "auto")
      .attr("markerWidth", 6)
      .attr("markerHeight", 6)
      .attr("xoverflow", "visible")
      .append("svg:path")
      .attr("d", "M 0,-5 L 10 ,0 L 0,5")
      .attr("fill", "rgba(255, 255, 255, 0.25)")
      .style("stroke", "none");

    // Copy arrays to prevent in-place mutation of props by D3
    const nodes = skills.map((s) => ({
      id: s.name,
      name: s.name,
      category: getCategory(s.name),
      is_active: s.is_active,
      profile: s.profile,
      x: 0,
      y: 0,
    }));

    const links: { source: string; target: string }[] = [];
    skills.forEach((s) => {
      s.dependencies.forEach((dep) => {
        // Ensure source exists in nodes list to avoid D3 simulation crash
        if (skills.some((sk) => sk.name === dep)) {
          links.push({
            source: dep,
            target: s.name,
          });
        }
      });
    });

    const g = svgElement.append("g");

    // Enable zoom and pan
    const zoomBehavior = d3
      .zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.5, 3])
      .on("zoom", (event) => {
        g.attr("transform", event.transform);
      });

    svgElement.call(zoomBehavior);

    // Physics force simulation
    const simulation = d3
      .forceSimulation<any>(nodes)
      .force(
        "link",
        d3
          .forceLink<any, any>(links)
          .id((d) => d.id)
          .distance(90)
      )
      .force("charge", d3.forceManyBody().strength(-150))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collide", d3.forceCollide().radius(25));

    // Render connection lines
    const link = g
      .append("g")
      .selectAll("line")
      .data(links)
      .enter()
      .append("line")
      .attr("stroke", "rgba(255, 255, 255, 0.15)")
      .attr("stroke-width", 2)
      .attr("marker-end", "url(#arrowhead)");

    // Render node groups
    const node = g
      .append("g")
      .selectAll("g")
      .data(nodes)
      .enter()
      .append("g")
      .attr("class", "node-group")
      .style("cursor", "pointer")
      .on("click", (event, d) => {
        event.stopPropagation();
        onSelectSkill(d.id);
      })
      .call(
        d3
          .drag<any, any>()
          .on("start", dragstarted)
          .on("drag", dragged)
          .on("end", dragended)
      );

    // Glowing circle shadow for active state
    node
      .append("circle")
      .attr("r", 12)
      .attr("fill", (d) => getCategoryColor(d.category))
      .attr("opacity", (d) => (d.is_active ? 0.85 : 0.3))
      .style("filter", (d) =>
        d.is_active
          ? `drop-shadow(0px 0px 6px ${getCategoryColor(d.category)})`
          : "none"
      );

    // Highlight selected node with border ring
    node
      .append("circle")
      .attr("r", 17)
      .attr("fill", "none")
      .attr("stroke", (d) =>
        d.id === selectedSkill ? getCategoryColor(d.category) : "none"
      )
      .attr("stroke-width", 2)
      .attr("stroke-dasharray", "4,2")
      .attr("opacity", 0.8);

    // Inner core dot
    node
      .append("circle")
      .attr("r", 4)
      .attr("fill", "#fff")
      .attr("opacity", (d) => (d.is_active ? 1 : 0.4));

    // Text labels
    node
      .append("text")
      .text((d) => d.name)
      .attr("dx", 18)
      .attr("dy", 4)
      .attr("fill", (d) => (d.id === selectedSkill ? "#fff" : "rgba(255, 255, 255, 0.75)"))
      .attr("font-weight", (d) => (d.id === selectedSkill ? "bold" : "normal"))
      .attr("font-size", "11px")
      .attr("font-family", "system-ui, sans-serif");

    // Physical simulation tick
    simulation.on("tick", () => {
      link
        .attr("x1", (d: any) => d.source.x)
        .attr("y1", (d: any) => d.source.y)
        .attr("x2", (d: any) => d.target.x)
        .attr("y2", (d: any) => d.target.y);

      node.attr("transform", (d: any) => `translate(${d.x}, ${d.y})`);
    });

    // Drag helper methods
    function dragstarted(event: any, d: any) {
      if (!event.active) simulation.alphaTarget(0.3).restart();
      d.fx = d.x;
      d.fy = d.y;
    }

    function dragged(event: any, d: any) {
      d.fx = event.x;
      d.fy = event.y;
    }

    function dragended(event: any, d: any) {
      if (!event.active) simulation.alphaTarget(0);
      d.fx = null;
      d.fy = null;
    }

    return () => {
      simulation.stop();
    };
  }, [skills, selectedSkill]);

  return (
    <div className="w-full h-full relative">
      <svg
        ref={svgRef}
        className="w-full h-full bg-black/20 rounded-lg border border-border"
      />
      <div
        className="absolute bottom-2 right-2 text-[10px] text-muted-foreground bg-black/40 px-1.5 py-0.5 rounded pointer-events-none"
      >
        滚轮缩放 / 拖拽平移 & 节点
      </div>
    </div>
  );
};
