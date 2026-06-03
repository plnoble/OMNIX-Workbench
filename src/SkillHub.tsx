import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SkillTopology } from "./SkillTopology";
import { Layers, Sparkles, BookOpen, Download } from "lucide-react";

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
      alert("技能内容更新成功！");
    } catch (e) {
      alert("保存失败：" + e);
    } finally {
      setIsSaving(false);
    }
  };

  const handleToggleActive = async (name: string, currentActive: boolean) => {
    try {
      await invoke("toggle_skill_active", { name, isActive: !currentActive });
      await loadSkills();
    } catch (e) {
      alert("更新状态失败：" + e);
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
      alert("更新 Profile 失败：" + e);
    }
  };

  // Add selected skill to furnace pot
  const addToFurnace = () => {
    if (!selectedSkillName) return;
    if (furnacePot.includes(selectedSkillName)) {
      alert("该技能已在融合炉中！");
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
      alert("请至少选择 2 个技能投入融合炉！");
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
      alert("融合失败：" + e);
    } finally {
      setIsFusing(false);
    }
  };

  // Write fused skill back to local library
  const handleAcceptFusion = async () => {
    if (!fusedResult) return;
    if (!fusedSaveName.trim()) {
      alert("请输入融合生成的技能 ID！");
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

      alert("融合超级技能资产已成功写入技能库！已重建血统拓扑。");
      setFurnacePot([]);
      setFusedResult(null);
      setShowDiff(false);
      setSelectedSkillName(fusedSaveName);
      setSelectedProfile("Core");
      await loadSkills();
    } catch (e) {
      alert("写入本地库失败：" + e);
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
      alert(`市场技能 ${item.name} 下载安装成功！已成功加载进本地拓扑网络。`);
      await loadSkills();
    } catch (e) {
      alert("安装市场技能失败：" + e);
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
      <div className="diff-view" style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "16px", marginTop: "16px" }}>
        <div style={{ background: "rgba(255, 0, 0, 0.03)", border: "1px solid rgba(239, 68, 68, 0.2)", borderRadius: "8px", padding: "12px", maxHeight: "300px", overflowY: "auto" }}>
          <h4 style={{ color: "#ef4444", marginBottom: "8px", fontSize: "12px" }}>原始片段基底 ({firstSkillName})</h4>
          <pre style={{ margin: 0, fontSize: "11px", fontFamily: "var(--font-mono)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>
            {originalLines.map((l, i) => (
              <div key={i} style={{ padding: "1px 0" }}>{l}</div>
            ))}
            <div style={{ fontStyle: "italic", opacity: 0.5 }}>... (原始 Markdown 正文共计加载完毕)</div>
          </pre>
        </div>
        <div style={{ background: "rgba(16, 185, 129, 0.03)", border: "1px solid rgba(16, 185, 129, 0.2)", borderRadius: "8px", padding: "12px", maxHeight: "300px", overflowY: "auto" }}>
          <h4 style={{ color: "#10b981", marginBottom: "8px", fontSize: "12px" }}>二阶融合演进草案</h4>
          <pre style={{ margin: 0, fontSize: "11px", fontFamily: "var(--font-mono)", color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>
            {fusedLines.map((l, i) => {
              const isHeader = l.startsWith("#");
              const isChecklist = l.includes("- [ ]") || l.includes("- [x]");
              const color = isHeader ? "#8b5cf6" : isChecklist ? "#10b981" : "inherit";
              return (
                <div key={i} style={{ padding: "1px 0", color }}>
                  {l.startsWith("+") || l.startsWith("➕") ? (
                    <span style={{ color: "#10b981", fontWeight: "bold" }}>{l}</span>
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
    <div className="skill-hub-layout" style={{ display: "grid", gridTemplateColumns: "260px 1fr", gap: "20px", height: "calc(100vh - 120px)" }}>
      
      {/* Left panel: list of skills */}
      <div className="card" style={{ display: "flex", flexDirection: "column", height: "100%", padding: "16px", minWidth: 0 }}>
        <h3 className="card-title" style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "14px" }}>
          <BookOpen size={16} color="var(--color-secondary)" />
          技能列表 ({filteredSkills.length})
        </h3>
        
        {/* Search */}
        <input 
          type="text" 
          className="form-input" 
          placeholder="搜索技能名称..." 
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
          style={{ marginBottom: "12px", fontSize: "13px", padding: "6px 10px" }}
        />

        {/* Category selector */}
        <div style={{ marginBottom: "12px" }}>
          <label style={{ fontSize: "11px", color: "var(--text-secondary)", display: "block", marginBottom: "4px" }}>分类过滤</label>
          <select 
            className="form-input" 
            value={categoryFilter} 
            onChange={(e) => setCategoryFilter(e.target.value)}
            style={{ fontSize: "12px", padding: "4px 8px", background: "rgba(0,0,0,0.3)" }}
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
        <div style={{ flexGrow: 1, overflowY: "auto", display: "flex", flexDirection: "column", gap: "8px", paddingRight: "4px" }}>
          {filteredSkills.map((sk) => (
            <div 
              key={sk.name}
              className={`nav-item ${selectedSkillName === sk.name ? "active" : ""}`}
              onClick={() => {
                setSelectedSkillName(sk.name);
                setSelectedProfile(sk.profile);
              }}
              style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "flex-start",
                padding: "8px 12px",
                borderRadius: "8px",
                background: selectedSkillName === sk.name ? "rgba(6, 182, 212, 0.08)" : "rgba(255,255,255,0.01)",
                border: selectedSkillName === sk.name ? "1px solid var(--color-secondary)" : "1px solid transparent"
              }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", width: "100%", alignItems: "center", marginBottom: "4px" }}>
                <span style={{ fontWeight: 500, fontSize: "13px" }}>{sk.name}</span>
                <span 
                  style={{
                    display: "inline-block",
                    width: "6px",
                    height: "6px",
                    borderRadius: "50%",
                    backgroundColor: sk.is_active ? "var(--color-success)" : "rgba(255,255,255,0.15)"
                  }}
                />
              </div>
              <span style={{ fontSize: "11px", color: "var(--text-secondary)", overflow: "hidden", textOverflow: "ellipsis", display: "-webkit-box", WebkitLineClamp: 1, WebkitBoxOrient: "vertical" }}>
                {sk.description}
              </span>
            </div>
          ))}
          {filteredSkills.length === 0 && (
            <div style={{ textAlign: "center", padding: "20px 0", color: "var(--text-muted)", fontSize: "12px" }}>
              未找到匹配的技能
            </div>
          )}
        </div>
      </div>

      {/* Right workspace: split visual editor & topology graph */}
      <div style={{ display: "flex", flexDirection: "column", height: "100%", minWidth: 0, gap: "16px" }}>
        
        {selectedSkill ? (
          <div className="card" style={{ display: "flex", flexDirection: "column", flexGrow: 1, padding: "16px", minHeight: 0 }}>
            {/* Header section */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", borderBottom: "1px solid var(--border-color)", paddingBottom: "12px", marginBottom: "16px" }}>
              <div>
                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                  <h2 style={{ fontSize: "18px", fontWeight: 600, color: "var(--text-primary)" }}>{selectedSkill.name}</h2>
                  <span className="workspace-badge" style={{ fontSize: "10px", padding: "2px 6px" }}>{getCategory(selectedSkill.name)}</span>
                </div>
                <p style={{ color: "var(--text-secondary)", fontSize: "12px", marginTop: "4px" }}>{selectedSkill.description}</p>
              </div>

              <div style={{ display: "flex", gap: "10px", alignItems: "center" }}>
                {/* Active switch */}
                <div style={{ display: "flex", alignItems: "center", gap: "8px", background: "rgba(255,255,255,0.02)", padding: "4px 10px", borderRadius: "20px", border: "1px solid var(--border-color)" }}>
                  <span style={{ fontSize: "12px", color: "var(--text-secondary)" }}>激活状态:</span>
                  <input 
                    type="checkbox" 
                    checked={selectedSkill.is_active}
                    onChange={() => handleToggleActive(selectedSkill.name, selectedSkill.is_active)}
                    style={{ cursor: "pointer", width: "14px", height: "14px" }}
                  />
                </div>

                {/* Profile switch */}
                <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                  <span style={{ fontSize: "12px", color: "var(--text-secondary)" }}>当前变体:</span>
                  <select 
                    className="form-input" 
                    value={selectedProfile}
                    onChange={(e) => {
                      const newProf = e.target.value;
                      setSelectedProfile(newProf);
                      handleUpdateProfile(selectedSkill.name, newProf);
                    }}
                    style={{ fontSize: "12px", padding: "4px 8px", background: "rgba(0,0,0,0.3)" }}
                  >
                    <option value="Minimal">Minimal (精简版)</option>
                    <option value="Core">Core (核心版)</option>
                    <option value="Comprehensive">Comprehensive (完整版)</option>
                  </select>
                </div>

                <button className="btn btn-secondary" onClick={addToFurnace} style={{ padding: "6px 12px", fontSize: "12px" }}>
                  🔥 放入融合炉
                </button>
              </div>
            </div>

            {/* Split workspace: Editor & Graph */}
            <div style={{ display: "grid", gridTemplateColumns: "1.2fr 1fr", gap: "16px", flexGrow: 1, minHeight: 0 }}>
              
              {/* Left side: Code content editor */}
              <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                  <span style={{ fontSize: "12px", fontWeight: 500, color: "var(--text-secondary)", display: "flex", alignItems: "center", gap: "4px" }}>
                    <Layers size={13} />
                    Markdown 规则代码 ({selectedProfile})
                  </span>
                  
                  <div style={{ display: "flex", gap: "8px" }}>
                    {isEditing ? (
                      <>
                        <button 
                          className="btn btn-secondary" 
                          onClick={() => loadSkillContent(selectedSkill.name, selectedProfile)}
                          style={{ padding: "2px 8px", fontSize: "11px", height: "24px" }}
                        >
                          取消
                        </button>
                        <button 
                          className="btn" 
                          onClick={handleSaveContent}
                          disabled={isSaving}
                          style={{ padding: "2px 8px", fontSize: "11px", height: "24px" }}
                        >
                          {isSaving ? "保存中..." : "💾 保存"}
                        </button>
                      </>
                    ) : (
                      <button 
                        className="btn btn-secondary" 
                        onClick={() => setIsEditing(true)}
                        style={{ padding: "2px 8px", fontSize: "11px", height: "24px" }}
                      >
                        编辑
                      </button>
                    )}
                  </div>
                </div>

                <div style={{ flexGrow: 1, position: "relative", minHeight: 0 }}>
                  {isEditing ? (
                    <textarea
                      className="form-input"
                      value={skillContent}
                      onChange={(e) => setSkillContent(e.target.value)}
                      style={{
                        width: "100%",
                        height: "100%",
                        fontFamily: "var(--font-mono)",
                        fontSize: "12px",
                        background: "rgba(0,0,0,0.4)",
                        resize: "none",
                        color: "var(--text-primary)",
                        padding: "12px",
                        lineHeight: 1.5,
                        boxShadow: "inset 0 0 10px rgba(0,0,0,0.5)"
                      }}
                    />
                  ) : (
                    <div 
                      style={{
                        width: "100%",
                        height: "100%",
                        overflowY: "auto",
                        background: "rgba(0,0,0,0.25)",
                        border: "1px solid var(--border-color)",
                        borderRadius: "8px",
                        padding: "12px",
                        fontFamily: "var(--font-mono)",
                        fontSize: "12px",
                        whiteSpace: "pre-wrap",
                        color: "var(--text-secondary)",
                        lineHeight: 1.5
                      }}
                    >
                      {skillContent}
                    </div>
                  )}
                </div>
              </div>

              {/* Right side: Force network */}
              <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
                <span style={{ fontSize: "12px", fontWeight: 500, color: "var(--text-secondary)", marginBottom: "8px", display: "flex", alignItems: "center", gap: "4px" }}>
                  <Sparkles size={13} />
                  力导向关联拓扑图 (OBSIDIAN STYLE)
                </span>
                <div style={{ flexGrow: 1, minHeight: 0 }}>
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
          <div className="card" style={{ display: "flex", alignItems: "center", justifyContent: "center", flexGrow: 1, color: "var(--text-muted)" }}>
            点按左侧列表加载技能
          </div>
        )}

        {/* Fusion Furnace Drawer */}
        <div className="card" style={{ padding: "16px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <div>
              <h3 className="card-title" style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "14px", margin: 0 }}>
                <Sparkles size={16} color="var(--color-warning)" />
                技能融合炉 (Fusion Furnace)
              </h3>
              <p style={{ color: "var(--text-secondary)", fontSize: "11px", marginTop: "2px" }}>
                至少将 2 个零散或冲突技能投入融合，AI 会进行知识蒸馏并合并冲突。
              </p>
            </div>
            
            {furnacePot.length >= 2 && (
              <button 
                className="btn" 
                onClick={handleIgniteFusion} 
                disabled={isFusing}
                style={{ background: "var(--accent-gradient)", border: "none", boxShadow: "var(--accent-glow)" }}
              >
                {isFusing ? "🔥 智能蒸馏融合中..." : "Ignite 点火融合"}
              </button>
            )}
          </div>

          {/* Selected slots */}
          <div style={{ display: "flex", flexWrap: "wrap", gap: "8px", marginTop: "12px" }}>
            {furnacePot.map((name) => (
              <div 
                key={name}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  background: "rgba(245, 158, 11, 0.08)",
                  border: "1px solid rgba(245, 158, 11, 0.3)",
                  padding: "4px 10px",
                  borderRadius: "16px",
                  fontSize: "12px",
                  color: "#fbbf24"
                }}
              >
                <span>🔥 {name}</span>
                <span 
                  onClick={() => removeFromFurnace(name)}
                  style={{ cursor: "pointer", fontWeight: "bold", marginLeft: "4px", opacity: 0.7 }}
                >
                  ✕
                </span>
              </div>
            ))}
            {furnacePot.length === 0 && (
              <span style={{ fontSize: "12px", color: "var(--text-muted)", fontStyle: "italic" }}>插槽为空 (点击右上角“放入融合炉”)</span>
            )}
          </div>

          {/* Render visual diff on complete */}
          {showDiff && fusedResult && (
            <div style={{ borderTop: "1px solid var(--border-color)", marginTop: "16px", paddingTop: "16px" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
                <div>
                  <h4 style={{ color: "var(--color-success)", fontSize: "13px", fontWeight: "bold" }}>AI 融合成功！请对比审核差异</h4>
                  <p style={{ fontSize: "11px", color: "var(--text-secondary)", marginTop: "2px" }}>{fusedResult.explanation}</p>
                </div>

                <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                  <input 
                    type="text" 
                    className="form-input" 
                    value={fusedSaveName} 
                    onChange={(e) => setFusedSaveName(e.target.value)}
                    placeholder="超级技能 ID (例如: file_operations)"
                    style={{ fontSize: "12px", padding: "4px 8px", width: "160px", background: "rgba(0,0,0,0.3)" }}
                  />
                  <button className="btn" onClick={handleAcceptFusion} style={{ padding: "4px 12px", fontSize: "12px" }}>
                    接受并写入本地库
                  </button>
                  <button className="btn btn-secondary" onClick={() => setShowDiff(false)} style={{ padding: "4px 12px", fontSize: "12px" }}>
                    放弃
                  </button>
                </div>
              </div>

              {renderSplitDiff()}
            </div>
          )}
        </div>

        {/* Skill Marketplace */}
        <div className="card" style={{ padding: "16px" }}>
          <h3 className="card-title" style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "14px" }}>
            <Download size={16} color="var(--color-secondary)" />
            公共技能市场 (Skill Marketplace)
          </h3>
          <p style={{ color: "var(--text-secondary)", fontSize: "11px", marginTop: "2px", marginBottom: "12px" }}>
            由 OMNIX 官方和社区维护的成熟技能包，点击即可一键下载并自动导入至您当前的本地开发库中。
          </p>

          <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "12px" }}>
            {MARKETPLACE_SKILLS.map((item) => (
              <div 
                key={item.name}
                style={{
                  background: "rgba(255, 255, 255, 0.02)",
                  border: "1px solid var(--border-color)",
                  borderRadius: "8px",
                  padding: "12px",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between"
                }}
              >
                <div>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "6px" }}>
                    <span style={{ fontWeight: 600, fontSize: "12px" }}>{item.name}</span>
                    <span style={{ fontSize: "10px", color: "var(--text-secondary)", background: "rgba(0,0,0,0.25)", padding: "1px 6px", borderRadius: "10px" }}>{item.category}</span>
                  </div>
                  <p style={{ color: "var(--text-secondary)", fontSize: "11px", lineHeight: 1.4, margin: 0 }}>{item.description}</p>
                </div>

                <button 
                  className="btn btn-secondary" 
                  onClick={() => handleDownloadMarketSkill(item)}
                  style={{
                    marginTop: "12px",
                    width: "100%",
                    padding: "4px 8px",
                    fontSize: "11px",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    gap: "4px"
                  }}
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
