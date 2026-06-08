import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SkillTopology } from "./SkillTopology";
import { Layers, Sparkles, BookOpen, Download } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";

interface Skill {
  name: string;
  description: string;
  file_path: string;
  profile: string;
  is_active: boolean;
  dependencies: string[];
  updated_at: string;
}

// Mock marketplace skills
const MARKETPLACE_SKILLS = [
  {
    name: "web_scraper",
    description: "高并发网页爬虫，支持动态 JavaScript 渲染、代理池配置与抗防爬限制。",
    category: "智能搜索",
    dependencies: ["file_reader", "file_writer"],
    content: "### Role & Identity\n你是一个专业的网络爬虫与数据提取专家...\n\n### Core Knowledge\n- 掌握 Playwright / Puppeteer 模拟无头浏览器操作。\n- 支持代理 IP 自动轮询与自定义请求 Header 规避阻断。\n\n### Step-by-Step Workflow\n1. 初始化浏览器引擎。\n2. 请求目标页面并等待动态内容渲染。\n3. 解析 DOM 节点并提取结构化 JSON 数据。\n4. 调用 file_writer 写入本地。\n\n### Quality Checklist\n- [ ] 是否设置了延迟防封禁限制？\n- [ ] 是否正确处理了验证码拦截？"
  },
  {
    name: "docker_builder",
    description: "自动化构建多阶段 Docker 容器镜像，并优化运行体积与安全性检查。",
    category: "测试部署",
    dependencies: ["file_reader"],
    content: "### Role & Identity\n你是一个 Docker 容器化构建专家，专注于极简轻量级镜像设计...\n\n### Core Knowledge\n- 掌握 Dockerfile 多阶段构建 (Multi-stage build) 语法。\n- 熟悉 alpine 等超轻量底层镜像与非 root 安全运行限制。\n\n### Step-by-Step Workflow\n1. 读取项目 package/cargo 文件结构。\n2. 自动生成多阶段 Dockerfile 配置文件。\n3. 编译应用，将制品拷贝至最小运行镜像中。\n\n### Quality Checklist\n- [ ] 镜像大小是否控制在 50MB 以内？\n- [ ] 容器内是否禁止了 root 提权？"
  },
  {
    name: "pptx_exporter",
    description: "将代码重构与系统设计事实，自动化渲染导出为精美的商业 PPTX 幻灯片汇报演示包。",
    category: "文档办公",
    dependencies: ["file_reader", "file_writer"],
    content: "### Role & Identity\n你是一个系统设计与商业演示导出助手，负责将技术文档转化为精美的幻灯片汇报...\n\n### Core Knowledge\n- 熟悉 PptxGenJS 或是 python-pptx 格式输出。\n- 掌握结构化商业演示版式：目录页、架构图页、收益对比页等。\n\n### Step-by-Step Workflow\n1. 读取要汇报的技术概要或系统设计文本。\n2. 拆解为分章节的幻灯片大纲。\n3. 构建 PPTX 的版面坐标与主题配色样式。\n4. 输出二进制流并写入为 .pptx 文件。\n\n### Quality Checklist\n- [ ] 每张 PPT 的字数是否精简？\n- [ ] 主题配色是否与 OMNIX 暗色系协调？"
  }
];

export const SkillHub: React.FC = () => {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedSkillName, setSelectedSkillName] = useState<string | null>(null);
  const [selectedProfile, setSelectedProfile] = useState<string>("Core");
  const [skillContent, setSkillContent] = useState<string>("");
  const [isEditing, setIsEditing] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);

  // Filter states
  const [searchKeyword, setSearchKeyword] = useState<string>("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");

  // Fusion Furnace states
  const [furnacePot, setFurnacePot] = useState<string[]>([]);
  const [isFusing, setIsFusing] = useState<boolean>(false);
  const [fusedResult, setFusedResult] = useState<{
    name: string;
    description: string;
    fused_code: string;
    explanation: string;
  } | null>(null);
  const [showDiff, setShowDiff] = useState<boolean>(false);
  const [fusedSaveName, setFusedSaveName] = useState<string>("");

  useEffect(() => {
    loadSkills();
  }, []);

  useEffect(() => {
    if (selectedSkillName) {
      loadSkillContent(selectedSkillName, selectedProfile);
    } else {
      setSkillContent("");
    }
  }, [selectedSkillName, selectedProfile]);

  const loadSkills = async () => {
    try {
      const all: Skill[] = await invoke("get_all_skills");
      setSkills(all);
      if (all.length > 0 && !selectedSkillName) {
        setSelectedSkillName(all[0].name);
        setSelectedProfile(all[0].profile);
      }
    } catch (e) {
      console.error("Failed to load skills:", e);
    }
  };

  const loadSkillContent = async (name: string, profile: string) => {
    try {
      const content: string = await invoke("get_skill_content", { name, profile });
      setSkillContent(content);
      setIsEditing(false);
    } catch (e) {
      console.error("Failed to load skill content:", e);
      setSkillContent("## 无法读取技能内容\n文件或 Profile 可能已损坏。");
    }
  };

  const handleSaveContent = async () => {
    if (!selectedSkillName) return;
    setIsSaving(true);
    try {
      await invoke("save_skill_content", {
        name: selectedSkillName,
        profile: selectedProfile,
        content: skillContent
      });
      setIsEditing(false);
      await loadSkills();
      toast.success("技能内容更新成功！");
    } catch (e) {
      toast.error("保存失败：" + e);
    } finally {
      setIsSaving(false);
    }
  };

  const handleToggleActive = async (name: string, currentActive: boolean) => {
    try {
      await invoke("toggle_skill_active", { name, isActive: !currentActive });
      await loadSkills();
    } catch (e) {
      toast.error("更新状态失败：" + e);
    }
  };

  const handleUpdateProfile = async (name: string, newProfile: string) => {
    try {
      await invoke("update_skill_profile", { name, profile: newProfile });
      if (name === selectedSkillName) {
        setSelectedProfile(newProfile);
      }
      await loadSkills();
    } catch (e) {
      toast.error("更新 Profile 失败：" + e);
    }
  };

  // Add selected skill to furnace pot
  const addToFurnace = () => {
    if (!selectedSkillName) return;
    if (furnacePot.includes(selectedSkillName)) {
      toast.warning("该技能已在融合炉中！");
      return;
    }
    setFurnacePot([...furnacePot, selectedSkillName]);
  };

  const removeFromFurnace = (name: string) => {
    setFurnacePot(furnacePot.filter((n) => n !== name));
  };

  // Run LLM skill fusion
  const handleIgniteFusion = async () => {
    if (furnacePot.length < 2) {
      toast.warning("请至少选择 2 个技能投入融合炉！");
      return;
    }
    setIsFusing(true);
    setFusedResult(null);
    setShowDiff(false);
    try {
      const res: any = await invoke("fuse_skills_api", { skills: furnacePot });
      setFusedResult(res);
      setFusedSaveName(`${furnacePot[0]}_fused`);
      setShowDiff(true);
    } catch (e) {
      toast.error("融合失败：" + e);
    } finally {
      setIsFusing(false);
    }
  };

  // Write fused skill back to local library
  const handleAcceptFusion = async () => {
    if (!fusedResult) return;
    if (!fusedSaveName.trim()) {
      toast.warning("请输入融合生成的技能 ID！");
      return;
    }
    try {
      const mergedDeps = Array.from(
        new Set(
          skills
            .filter((s) => furnacePot.includes(s.name))
            .flatMap((s) => s.dependencies)
        )
      );

      await invoke("create_skill", {
        name: fusedSaveName.replace(/\s+/g, "_"),
        description: fusedResult.description,
        profile: "Core",
        dependencies: mergedDeps,
        content: fusedResult.fused_code
      });

      toast.success("融合超级技能资产已成功写入技能库！已重建血统拓扑。");
      setFurnacePot([]);
      setFusedResult(null);
      setShowDiff(false);
      setSelectedSkillName(fusedSaveName);
      setSelectedProfile("Core");
      await loadSkills();
    } catch (e) {
      toast.error("写入本地库失败：" + e);
    }
  };

  // Install custom skill from marketplace
  const handleDownloadMarketSkill = async (item: typeof MARKETPLACE_SKILLS[0]) => {
    try {
      await invoke("create_skill", {
        name: item.name,
        description: item.description,
        profile: "Core",
        dependencies: item.dependencies,
        content: item.content
      });
      toast.success(`市场技能 ${item.name} 下载安装成功！已成功加载进本地拓扑网络。`);
      await loadSkills();
    } catch (e) {
      toast.error("安装市场技能失败：" + e);
    }
  };

  // Helper to determine skill category for lists
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

  // Client side filtering
  const filteredSkills = skills.filter((s) => {
    const matchesSearch = s.name.toLowerCase().includes(searchKeyword.toLowerCase()) ||
                          s.description.toLowerCase().includes(searchKeyword.toLowerCase());

    const cat = getCategory(s.name);
    const matchesCategory = categoryFilter === "all" || cat === categoryFilter;

    return matchesSearch && matchesCategory;
  });

  const selectedSkill = skills.find((s) => s.name === selectedSkillName);

  // Line-by-line Diff Visualizer
  const renderSplitDiff = () => {
    if (!fusedResult || furnacePot.length === 0) return null;

    // We compare with the first skill in furnace pot as "original"
    const firstSkillName = furnacePot[0];

    const originalLines = [
      `### Skill: ${firstSkillName}`,
      `Description: ${skills.find((s) => s.name === firstSkillName)?.description || ""}`,
      "",
      "// (本处显示首选原始技能以供重叠比对)",
      ""
    ];

    const fusedLines = fusedResult.fused_code.split("\n");

    return (
      <div className="grid grid-cols-2 gap-4 mt-4">
        <div className="bg-red-500/3 border border-red-500/20 rounded-lg p-3 max-h-[300px] overflow-y-auto">
          <h4 className="text-red-500 mb-2 text-xs">原始片段基底 ({firstSkillName})</h4>
          <pre className="m-0 text-xs font-mono text-secondary-foreground whitespace-pre-wrap">
            {originalLines.map((l, i) => (
              <div key={i} className="py-px">{l}</div>
            ))}
            <div className="italic opacity-50">... (原始 Markdown 正文共计加载完毕)</div>
          </pre>
        </div>
        <div className="bg-emerald-500/3 border border-emerald-500/20 rounded-lg p-3 max-h-[300px] overflow-y-auto">
          <h4 className="text-emerald-500 mb-2 text-xs">二阶融合演进草案</h4>
          <pre className="m-0 text-xs font-mono text-secondary-foreground whitespace-pre-wrap">
            {fusedLines.map((l, i) => {
              const isHeader = l.startsWith("#");
              const isChecklist = l.includes("- [ ]") || l.includes("- [x]");
              return (
                <div key={i} className={cn("py-px", isHeader && "text-purple-500", isChecklist && "text-emerald-500", !isHeader && !isChecklist && "inherit")}>
                  {l.startsWith("+") || l.startsWith("➕") ? (
                    <span className="text-emerald-500 font-bold">{l}</span>
                  ) : l}
                </div>
              );
            })}
          </pre>
        </div>
      </div>
    );
  };

  return (
    <div className="skill-hub-layout grid grid-cols-[260px_1fr] gap-5 h-[calc(100vh-120px)]">

      {/* Left panel: list of skills */}
      <div className="card flex flex-col h-full p-4 min-w-0">
        <h3 className="card-title flex items-center gap-1.5 text-sm">
          <BookOpen size={16} color="var(--color-secondary)" />
          技能列表 ({filteredSkills.length})
        </h3>

        {/* Search */}
        <input
          type="text"
          className="form-input mb-3 text-sm py-1.5 px-2.5"
          placeholder="搜索技能名称..."
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
        />

        {/* Category selector */}
        <div className="mb-3">
          <label className="text-xs text-secondary-foreground block mb-1">分类过滤</label>
          <select
            className="form-input text-xs py-1 px-2 bg-black/30"
            value={categoryFilter}
            onChange={(e) => setCategoryFilter(e.target.value)}
          >
            <option value="all">全部分类</option>
            <option value="文件操作">文件操作</option>
            <option value="版本控制">版本控制</option>
            <option value="静态分析">静态分析</option>
            <option value="智能检索">智能检索</option>
            <option value="自定义技能">自定义技能</option>
          </select>
        </div>

        {/* Scrollable list */}
        <div className="flex-1 overflow-y-auto flex flex-col gap-2 pr-1">
          {filteredSkills.map((sk) => (
            <div
              key={sk.name}
              className={cn(
                "nav-item flex flex-col items-start py-2 px-3 rounded-lg border",
                selectedSkillName === sk.name ? "active bg-cyan-500/8 border-[var(--color-secondary)]" : "bg-white/1 border-transparent"
              )}
              onClick={() => {
                setSelectedSkillName(sk.name);
                setSelectedProfile(sk.profile);
              }}
            >
              <div className="flex justify-between w-full items-center mb-1">
                <span className="font-medium text-sm">{sk.name}</span>
                <span
                  className={cn(
                    "inline-block w-1.5 h-1.5 rounded-full",
                    sk.is_active ? "bg-[var(--color-success)]" : "bg-white/15"
                  )}
                />
              </div>
              <span className="text-xs text-secondary-foreground overflow-hidden text-ellipsis line-clamp-1">
                {sk.description}
              </span>
            </div>
          ))}
          {filteredSkills.length === 0 && (
            <div className="text-center py-5 text-muted-foreground text-xs">
              未找到匹配的技能
            </div>
          )}
        </div>
      </div>

      {/* Right workspace: split visual editor & topology graph */}
      <div className="flex flex-col h-full min-w-0 gap-4 overflow-y-auto pr-1.5">

        {selectedSkill ? (
          <div className="card flex flex-col flex-1 p-4 min-h-[550px]">
            {/* Header section */}
            <div className="flex justify-between items-start border-b border-border pb-3 mb-4">
              <div>
                <div className="flex items-center gap-2">
                  <h2 className="text-lg font-semibold text-[var(--text-primary)]">{selectedSkill.name}</h2>
                  <span className="workspace-badge text-[10px] py-0.5 px-1.5">{getCategory(selectedSkill.name)}</span>
                </div>
                <p className="text-secondary-foreground text-xs mt-1">{selectedSkill.description}</p>
              </div>

              <div className="flex gap-2.5 items-center">
                {/* Active switch */}
                <div className="flex items-center gap-2 bg-white/2 py-1 px-2.5 rounded-full border border-border">
                  <span className="text-xs text-secondary-foreground">激活状态:</span>
                  <input
                    type="checkbox"
                    checked={selectedSkill.is_active}
                    onChange={() => handleToggleActive(selectedSkill.name, selectedSkill.is_active)}
                    className="cursor-pointer w-3.5 h-3.5"
                  />
                </div>

                {/* Profile switch */}
                <div className="flex items-center gap-1.5">
                  <span className="text-xs text-secondary-foreground">当前变体:</span>
                  <select
                    className="form-input text-xs py-1 px-2 bg-black/30"
                    value={selectedProfile}
                    onChange={(e) => {
                      const newProf = e.target.value;
                      setSelectedProfile(newProf);
                      handleUpdateProfile(selectedSkill.name, newProf);
                    }}
                  >
                    <option value="Minimal">Minimal (精简版)</option>
                    <option value="Core">Core (核心版)</option>
                    <option value="Comprehensive">Comprehensive (完整版)</option>
                  </select>
                </div>

                <button className="btn btn-secondary py-1.5 px-3 text-xs" onClick={addToFurnace}>
                  🔥 放入融合炉
                </button>
              </div>
            </div>

            {/* Split workspace: Editor & Graph */}
            <div className="grid grid-cols-[1.2fr_1fr] gap-4 flex-1 min-h-0">

              {/* Left side: Code content editor */}
              <div className="flex flex-col h-full">
                <div className="flex justify-between items-center mb-2">
                  <span className="text-xs font-medium text-secondary-foreground flex items-center gap-1">
                    <Layers size={13} />
                    Markdown 规则代码 ({selectedProfile})
                  </span>

                  <div className="flex gap-2">
                    {isEditing ? (
                      <>
                        <button
                          className="btn btn-secondary py-0.5 px-2 text-[11px] h-6"
                          onClick={() => loadSkillContent(selectedSkill.name, selectedProfile)}
                        >
                          取消
                        </button>
                        <button
                          className="btn py-0.5 px-2 text-[11px] h-6"
                          onClick={handleSaveContent}
                          disabled={isSaving}
                        >
                          {isSaving ? "保存中..." : "💾 保存"}
                        </button>
                      </>
                    ) : (
                      <button
                        className="btn btn-secondary py-0.5 px-2 text-[11px] h-6"
                        onClick={() => setIsEditing(true)}
                      >
                        编辑
                      </button>
                    )}
                  </div>
                </div>

                <div className="flex-1 relative min-h-0">
                  {isEditing ? (
                    <textarea
                      className="form-input w-full h-full font-mono text-xs bg-black/40 resize-none text-[var(--text-primary)] p-3 leading-normal shadow-[inset_0_0_10px_rgba(0,0,0,0.5)]"
                      value={skillContent}
                      onChange={(e) => setSkillContent(e.target.value)}
                    />
                  ) : (
                    <div
                      className="w-full h-full overflow-y-auto bg-black/25 border border-border rounded-lg p-3 font-mono text-xs whitespace-pre-wrap text-muted-foreground leading-normal"
                    >
                      {skillContent}
                    </div>
                  )}
                </div>
              </div>

              {/* Right side: Force network */}
              <div className="flex flex-col h-full">
                <span className="text-xs font-medium text-secondary-foreground mb-2 flex items-center gap-1">
                  <Sparkles size={13} />
                  力导向关联拓扑图 (OBSIDIAN STYLE)
                </span>
                <div className="flex-1 min-h-0">
                  <SkillTopology
                    skills={skills}
                    selectedSkill={selectedSkillName}
                    onSelectSkill={(name) => {
                      setSelectedSkillName(name);
                    }}
                  />
                </div>
              </div>

            </div>
          </div>
        ) : (
          <div className="card flex items-center justify-center flex-1 text-muted-foreground">
            点按左侧列表加载技能
          </div>
        )}

        {/* Fusion Furnace Drawer */}
        <div className="card p-4">
          <div className="flex justify-between items-center">
            <div>
              <h3 className="card-title flex items-center gap-1.5 text-sm m-0">
                <Sparkles size={16} color="var(--color-warning)" />
                技能融合炉 (Fusion Furnace)
              </h3>
              <p className="text-secondary-foreground text-[11px] mt-0.5">
                至少将 2 个零散或冲突技能投入融合，AI 会进行知识蒸馏并合并冲突。
              </p>
            </div>

            {furnacePot.length >= 2 && (
              <button
                className="btn border-none"
                onClick={handleIgniteFusion}
                disabled={isFusing}
                style={{ background: "var(--accent-gradient)", boxShadow: "var(--accent-glow)" }} // TODO: migrate to Tailwind — CSS variable references
              >
                {isFusing ? "🔥 智能蒸馏融合中..." : "Ignite 点火融合"}
              </button>
            )}
          </div>

          {/* Selected slots */}
          <div className="flex flex-wrap gap-2 mt-3">
            {furnacePot.map((name) => (
              <div
                key={name}
                className="flex items-center gap-1.5 bg-amber-500/8 border border-amber-500/30 py-1 px-2.5 rounded-2xl text-xs text-amber-400"
              >
                <span>🔥 {name}</span>
                <span
                  className="cursor-pointer font-bold ml-1 opacity-70"
                  onClick={() => removeFromFurnace(name)}
                >
                  ✕
                </span>
              </div>
            ))}
            {furnacePot.length === 0 && (
              <span className="text-xs text-muted-foreground italic">插槽为空 (点击右上角"放入融合炉")</span>
            )}
          </div>

          {/* Render visual diff on complete */}
          {showDiff && fusedResult && (
            <div className="border-t border-border mt-4 pt-4">
              <div className="flex justify-between items-center mb-3">
                <div>
                  <h4 className="text-emerald-500 text-sm font-bold">AI 融合成功！请对比审核差异</h4>
                  <p className="text-[11px] text-secondary-foreground mt-0.5">{fusedResult.explanation}</p>
                </div>

                <div className="flex gap-2 items-center">
                  <input
                    type="text"
                    className="form-input text-xs py-1 px-2 w-40 bg-black/30"
                    value={fusedSaveName}
                    onChange={(e) => setFusedSaveName(e.target.value)}
                    placeholder="超级技能 ID (例如: file_operations)"
                  />
                  <button className="btn py-1 px-3 text-xs" onClick={handleAcceptFusion}>
                    接受并写入本地库
                  </button>
                  <button className="btn btn-secondary py-1 px-3 text-xs" onClick={() => setShowDiff(false)}>
                    放弃
                  </button>
                </div>
              </div>

              {renderSplitDiff()}
            </div>
          )}
        </div>

        {/* Skill Marketplace */}
        <div className="card p-4">
          <h3 className="card-title flex items-center gap-1.5 text-sm">
            <Download size={16} color="var(--color-secondary)" />
            公共技能市场 (Skill Marketplace)
          </h3>
          <p className="text-secondary-foreground text-[11px] mt-0.5 mb-3">
            由 OMNIX 官方和社区维护的成熟技能包，点击即可一键下载并自动导入至您当前的本地开发库中。
          </p>

          <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
            {MARKETPLACE_SKILLS.map((item) => (
              <div
                key={item.name}
                className="bg-white/2 border border-border rounded-lg p-3 flex flex-col justify-between"
              >
                <div>
                  <div className="flex justify-between items-center mb-1.5">
                    <span className="font-semibold text-xs">{item.name}</span>
                    <span className="text-[10px] text-muted-foreground bg-black/25 py-px px-1.5 rounded-[10px]">{item.category}</span>
                  </div>
                  <p className="text-secondary-foreground text-[11px] leading-snug m-0">{item.description}</p>
                </div>

                <button
                  className="btn btn-secondary mt-3 w-full py-1 px-2 text-[11px] flex items-center justify-center gap-1"
                  onClick={() => handleDownloadMarketSkill(item)}
                >
                  <Download size={12} />
                  一键导入
                </button>
              </div>
            ))}
          </div>
        </div>

      </div>

    </div>
  );
};
