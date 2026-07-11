import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SkillTopology } from "./SkillTopology";
import { SkillPoolPanel } from "@/components/SkillPoolPanel";
import { Layers, Sparkles, BookOpen, Download, RefreshCw, Search, Star, Upload, ArrowRightLeft, HardDrive, CheckCircle2, XCircle, AlertCircle, FolderOpen, Wand2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { toast } from "@/components/ui/sonner";
import { modelApi, skillLibraryApi, skillSetApi, skillSyncApi, skillGeneratorApi, shellApi, skillDagApi, type WorkspaceFile, type SkillDraft, type ConflictPair } from "@/lib/tauri-api";
import type { PlatformModel } from "@/types";
import type {
  ToolStatus as SyncToolStatus,
  ScanReport,
  ScanItem,
  ConflictInfo,
  GitCloneResult,
  GitSkillCandidate,
  MarketSkill,
  MarketSkillPreview,
  SkillSet,
} from "@/lib/tauri-api";

interface Skill {
  name: string;
  description: string;
  file_path: string;
  profile: string;
  is_active: boolean;
  dependencies: string[];
  updated_at: string;
  // Skill Sync fields (P1 — DEC-018)
  source_type: string;
  source_ref: string | null;
  source_revision: string | null;
  central_path: string;
  content_hash: string | null;
  starred: boolean;
  category: string | null;
}

/** Tool adapter status for skill sync */
export interface ToolStatus {
  verification: "verified" | "experimental";
  verification_note: string;
  tool_id: string;
  display_name: string;
  is_installed: boolean;
  skill_base_path: string;
}

/** Skill sync target record */
export interface SkillTargetRecord {
  id: string;
  skill_id: string;
  tool: string;
  target_path: string;
  mode: string;
  status: string;
  last_error: string | null;
  synced_at: number | null;
}

/** Skill discovered in a tool's directory */
export interface DiscoveredSkill {
  name: string;
  path: string;
  tool: string;
  content_hash: string;
}

type SkillHubSection = "pool" | "library" | "generate" | "furnace" | "sets" | "market" | "sync";

const SKILL_HUB_SECTIONS: Array<{ id: SkillHubSection; label: string; desc: string }> = [
  { id: "pool", label: "技能池", desc: "收集→待定池→AI审核→正式池，网关直调零分发" },
  { id: "library", label: "技能库", desc: "查看、编辑、收藏和启停本地技能" },
  { id: "generate", label: "生成", desc: "扫描工作区，让模型生成技能草稿" },
  { id: "furnace", label: "熔炉", desc: "融合多个技能并审批写入" },
  { id: "sets", label: "组合", desc: "保存 Skill Set 并同步到工具" },
  { id: "market", label: "市场", desc: "搜索、预览和导入外部技能" },
  { id: "sync", label: "同步", desc: "扫描、Git 来源、漂移与冲突处理" },
];

export const SkillHub: React.FC = () => {
  const [section, setSection] = useState<SkillHubSection>("pool");
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedSkillName, setSelectedSkillName] = useState<string | null>(null);
  const [selectedProfile, setSelectedProfile] = useState<string>("Core");
  const [skillContent, setSkillContent] = useState<string>("");
  const [isEditing, setIsEditing] = useState<boolean>(false);
  const [isSaving, setIsSaving] = useState<boolean>(false);

  // Filter states
  const [searchKeyword, setSearchKeyword] = useState<string>("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [showStarredOnly, setShowStarredOnly] = useState(false);

  // Fusion Furnace states
  const [furnacePot, setFurnacePot] = useState<string[]>([]);
  // F-D: SkillDAG conflict awareness — warn when combined skills conflict.
  const [dagConflicts, setDagConflicts] = useState<ConflictPair[]>([]);
  useEffect(() => {
    if (furnacePot.length < 2) { setDagConflicts([]); return; }
    skillDagApi.checkSet(furnacePot).then((v) => setDagConflicts(v.conflicts)).catch(() => setDagConflicts([]));
  }, [furnacePot]);
  const [isFusing, setIsFusing] = useState<boolean>(false);
  const [fusedResult, setFusedResult] = useState<{
    draft_id: string;
    name: string;
    description: string;
    fused_code: string;
    explanation: string;
    conflicts: string[];
    status: string;
  } | null>(null);
  const [showDiff, setShowDiff] = useState<boolean>(false);
  const [fusedSaveName, setFusedSaveName] = useState<string>("");
  const [fusionModels, setFusionModels] = useState<PlatformModel[]>([]);
  const [selectedFusionModel, setSelectedFusionModel] = useState("");

  // ── Skill generation from workspace ──
  const [genWorkspace, setGenWorkspace] = useState("");
  const [genFiles, setGenFiles] = useState<WorkspaceFile[]>([]);
  const [genSelected, setGenSelected] = useState<Set<string>>(new Set());
  const [genName, setGenName] = useState("");
  const [genDraft, setGenDraft] = useState<SkillDraft | null>(null);
  const [genBusy, setGenBusy] = useState("");

  const pickGenWorkspace = async () => {
    const path = await shellApi.pickDirectory();
    if (!path) return;
    setGenWorkspace(path);
    setGenFiles([]);
    setGenSelected(new Set());
    setGenDraft(null);
    setGenBusy("scan");
    try {
      setGenFiles(await skillGeneratorApi.scanWorkspace(path));
    } catch (error) {
      toast.error(`扫描失败：${error}`);
    } finally {
      setGenBusy("");
    }
  };

  const toggleGenFile = (path: string) => {
    setGenSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  };

  const generateSkillDraft = async () => {
    if (!genName.trim() || genSelected.size === 0) {
      toast.warning("请填写技能名称并至少选择一个文件");
      return;
    }
    setGenBusy("generate");
    try {
      setGenDraft(await skillGeneratorApi.generate(genName.trim(), Array.from(genSelected), genWorkspace));
    } catch (error) {
      toast.error(`生成失败：${error}`);
    } finally {
      setGenBusy("");
    }
  };

  const saveGeneratedSkill = async () => {
    if (!genDraft) return;
    setGenBusy("save");
    try {
      const firstLine = genDraft.draft.split("\n").find((line) => line.trim()) ?? "";
      const description = firstLine.replace(/^#+\s*/, "").slice(0, 120) || `由工作区生成：${genDraft.name}`;
      await skillLibraryApi.create(genDraft.name, description, "Core", [], genDraft.draft);
      await loadSkills();
      toast.success(`已保存为本地技能：${genDraft.name}`);
      setGenDraft(null);
      setGenName("");
      setGenSelected(new Set());
    } catch (error) {
      toast.error(`保存失败：${error}`);
    } finally {
      setGenBusy("");
    }
  };

  // ── P3: Sync & Scanner states ──
  const [toolStatuses, setToolStatuses] = useState<SyncToolStatus[]>([]);
  const [scanReport, setScanReport] = useState<ScanReport | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncTargets, setSyncTargets] = useState<Record<string, string[]>>({}); // skillName -> toolIds
  const [showScanPanel, setShowScanPanel] = useState(false);

  // ── P5: Git Skill Source states ──
  const [gitRepoUrl, setGitRepoUrl] = useState("");
  const [gitBranch, setGitBranch] = useState("");
  const [isCloning, setIsCloning] = useState(false);
  const [gitCandidates, setGitCandidates] = useState<GitSkillCandidate[]>([]);
  const [gitCloneResult, setGitCloneResult] = useState<GitCloneResult | null>(null);
  const [showGitPanel, setShowGitPanel] = useState(false);
  const [conflicts, setConflicts] = useState<ConflictInfo[]>([]);
  const [conflictStrategy, setConflictStrategy] = useState<"skip" | "overwrite" | "rename">("overwrite");

  // Skill set states
  const [skillSets, setSkillSets] = useState<SkillSet[]>([]);
  const [skillSetName, setSkillSetName] = useState("");
  const [skillSetDescription, setSkillSetDescription] = useState("");
  const [skillSetTargets, setSkillSetTargets] = useState<string[]>([]);
  const [isSavingSkillSet, setIsSavingSkillSet] = useState(false);

  // Marketplace states
  const [marketQuery, setMarketQuery] = useState("");
  const [marketResults, setMarketResults] = useState<MarketSkill[]>([]);
  const [isSearchingMarket, setIsSearchingMarket] = useState(false);
  const [selectedMarketSkill, setSelectedMarketSkill] = useState<MarketSkill | null>(null);
  const [marketPreview, setMarketPreview] = useState<MarketSkillPreview | null>(null);
  const [isLoadingMarketPreview, setIsLoadingMarketPreview] = useState(false);
  const [marketImportConflict, setMarketImportConflict] = useState(false);

  useEffect(() => {
    loadSkills();
    loadToolStatuses();
    loadSkillSets();
    modelApi.getActive().then((models) => {
      setFusionModels(models);
      if (models[0]) setSelectedFusionModel(`${models[0].platform_id}:${models[0].model_name}`);
    }).catch(() => setFusionModels([]));
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

  // ── P3: Sync helpers ──

  const loadToolStatuses = async () => {
    try {
      const statuses = await skillSyncApi.getToolStatus();
      setToolStatuses(statuses);
    } catch (e) {
      console.error("Failed to load tool statuses:", e);
    }
  };

  const loadSkillSets = async () => {
    try {
      setSkillSets(await skillSetApi.list());
    } catch (e) {
      console.error("Failed to load skill sets:", e);
      setSkillSets([]);
    }
  };

  const handleScanDisk = async () => {
    setIsScanning(true);
    try {
      const report = await skillSyncApi.scanDiskSkills();
      setScanReport(report);
      setShowScanPanel(true);
      toast.success(`扫描完成：发现 ${report.total_found} 个技能`);
    } catch (e) {
      toast.error("扫描失败：" + e);
    } finally {
      setIsScanning(false);
    }
  };

  const handleImportUnmanaged = async (items: ScanItem[]) => {
    try {
      const count = await skillSyncApi.importUnmanaged(items);
      toast.success(`成功导入 ${count} 个技能`);
      await loadSkills();
      // Re-scan after import
      const report = await skillSyncApi.scanDiskSkills();
      setScanReport(report);
    } catch (e) {
      toast.error("导入失败：" + e);
    }
  };

  // G1: 一键统一技能库 — 扫描全盘 → 未管理技能纳入中央库 ~/.omnix/skills → 让所有已安装
  // agent 用软链共享同一份（Windows 无软链权限时自动回退复制）。三步串成一个闭环。
  const [isUnifying, setIsUnifying] = useState(false);
  const handleUnifyAll = async () => {
    setIsUnifying(true);
    try {
      // 1) Scan and import every unmanaged skill into the central library.
      const report = await skillSyncApi.scanDiskSkills();
      let imported = 0;
      if (report.unmanaged.length > 0) {
        imported = await skillSyncApi.importUnmanaged(report.unmanaged);
      }
      await loadSkills();
      // 2) Re-scan, then symlink every managed skill to all installed agents so
      //    they read one shared central copy.
      const rescan = await skillSyncApi.scanDiskSkills();
      setScanReport(rescan);
      setShowScanPanel(true);
      const names = Array.from(new Set([...rescan.managed, ...rescan.drifted].map((s) => s.name)));
      let synced = 0;
      let failed = 0;
      if (names.length > 0) {
        const results = await skillSyncApi.syncBatch(names, "symlink", "overwrite");
        for (const r of results) { synced += r.succeeded; failed += r.failed; }
      }
      toast.success(`已统一技能库：纳入 ${imported} 个 · 共享到各 agent ${synced} 处${failed ? `（${failed} 失败）` : ""}`);
    } catch (e) {
      toast.error("统一技能库失败：" + e);
    } finally {
      setIsUnifying(false);
    }
  };

  const handleToggleStarred = async (name: string) => {
    try {
      await skillSyncApi.toggleStarred(name);
      await loadSkills();
    } catch (e) {
      toast.error("切换收藏失败：" + e);
    }
  };

  const handleSyncSkill = async (skillName: string) => {
    const toolIds = syncTargets[skillName];
    if (!toolIds || toolIds.length === 0) {
      toast.warning("请先勾选要同步的工具");
      return;
    }

    // Check conflicts first
    setIsSyncing(true);
    try {
      const conflictsFound = await skillSyncApi.checkConflicts(skillName, toolIds);
      const realConflicts = conflictsFound.filter(c => c.exists && !c.is_identical);
      setConflicts(realConflicts);

      if (realConflicts.length > 0) {
        // Show conflict dialog — conflicts are already set in state, the panel renders automatically
        setIsSyncing(false);
        return;
      }

      // No conflicts — proceed
      await doSync(skillName, toolIds);
    } catch (e) {
      toast.error("同步失败：" + e);
      setIsSyncing(false);
    }
  };

  const doSync = async (skillName: string, toolIds: string[]) => {
    setIsSyncing(true);
    try {
      const result = await skillSyncApi.syncToMany(skillName, toolIds, "copy", conflictStrategy);
      if (result.failed > 0) {
        toast.warning(`同步完成：${result.succeeded} 成功, ${result.failed} 失败`);
      } else {
        toast.success(`已同步到 ${result.succeeded} 个工具`);
      }
      await loadToolStatuses();
    } catch (e) {
      toast.error("同步失败：" + e);
    } finally {
      setIsSyncing(false);
      setConflicts([]);
    }
  };

  const toggleSyncTarget = (skillName: string, toolId: string) => {
    setSyncTargets(prev => {
      const current = prev[skillName] || [];
      const next = current.includes(toolId)
        ? current.filter(id => id !== toolId)
        : [...current, toolId];
      return { ...prev, [skillName]: next };
    });
  };

  const toggleSkillSetTarget = (toolId: string) => {
    setSkillSetTargets((prev) =>
      prev.includes(toolId) ? prev.filter((id) => id !== toolId) : [...prev, toolId]
    );
  };

  const handleCreateSkillSet = async () => {
    const skillIds = furnacePot.length > 0 ? furnacePot : selectedSkillName ? [selectedSkillName] : [];
    if (!skillSetName.trim()) {
      toast.warning("请输入组合名称");
      return;
    }
    if (skillIds.length === 0) {
      toast.warning("请至少选择一个技能");
      return;
    }
    setIsSavingSkillSet(true);
    try {
      await skillSetApi.create(skillSetName.trim(), skillSetDescription.trim(), skillIds, skillSetTargets);
      toast.success("技能组合已保存");
      setSkillSetName("");
      setSkillSetDescription("");
      await loadSkillSets();
    } catch (e) {
      toast.error("保存技能组合失败：" + e);
    } finally {
      setIsSavingSkillSet(false);
    }
  };

  const handleDeleteSkillSet = async (id: string) => {
    try {
      await skillSetApi.delete(id);
      await loadSkillSets();
      toast.success("技能组合已删除");
    } catch (e) {
      toast.error("删除技能组合失败：" + e);
    }
  };

  const handleSyncSkillSet = async (set: SkillSet) => {
    const targets = set.sync_targets.length > 0 ? set.sync_targets : skillSetTargets;
    if (targets.length === 0) {
      toast.warning("请先选择同步目标");
      return;
    }
    setIsSyncing(true);
    try {
      await skillSetApi.syncToTools(set.id, targets, "copy", conflictStrategy);
      toast.success(`已同步组合：${set.name}`);
      await loadToolStatuses();
    } catch (e) {
      toast.error("同步技能组合失败：" + e);
    } finally {
      setIsSyncing(false);
    }
  };

  const handleSearchMarket = async () => {
    const query = marketQuery.trim();
    if (!query) {
      toast.warning("请输入搜索关键词");
      return;
    }
    setIsSearchingMarket(true);
    try {
      const results = await skillLibraryApi.searchMarket(query);
      setMarketResults(results);
      setSelectedMarketSkill(results[0] ?? null);
      setMarketPreview(null);
      setMarketImportConflict(false);
      if (results.length === 0) {
        toast.info("没有找到匹配技能");
      }
    } catch (e) {
      toast.error("搜索技能市场失败：" + e);
    } finally {
      setIsSearchingMarket(false);
    }
  };

  // ── P5: Git skill source helpers ──

  const handleCloneRepo = async () => {
    if (!gitRepoUrl.trim()) { toast.warning("请输入 Git 仓库地址"); return; }
    setIsCloning(true);
    setGitCandidates([]);
    setGitCloneResult(null);
    try {
      const result = await skillSyncApi.cloneRepo(gitRepoUrl, gitBranch || undefined);
      setGitCloneResult(result);
      // Auto-list candidates
      const candidates = await skillSyncApi.listRepoSkills(gitRepoUrl);
      setGitCandidates(candidates);
      setShowGitPanel(true);
      toast.success(`克隆成功！发现 ${candidates.length} 个技能`);
    } catch (e) {
      toast.error("克隆失败：" + e);
    } finally {
      setIsCloning(false);
    }
  };

  const handleImportGitSkill = async (candidate: GitSkillCandidate) => {
    if (!gitCloneResult) return;
    try {
      await skillSyncApi.importGitSkill(gitCloneResult.repo_url, candidate.name, gitCloneResult.revision);
      toast.success(`已导入 ${candidate.name}`);
      await loadSkills();
      // Refresh candidate list
      const candidates = await skillSyncApi.listRepoSkills(gitCloneResult.repo_url);
      setGitCandidates(candidates);
    } catch (e) {
      toast.error("导入失败：" + e);
    }
  };

  const handleCheckGitUpdates = async () => {
    try {
      const updates = await skillSyncApi.checkGitUpdates();
      const hasUpdates = updates.filter(u => u.has_update);
      if (hasUpdates.length === 0) {
        toast.success("所有 Git 技能均为最新版本");
      } else {
        toast.warning(`${hasUpdates.length} 个技能有更新`);
      }
    } catch (e) {
      toast.error("检查更新失败：" + e);
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
    if (!selectedFusionModel) {
      toast.warning("请先选择用于融合的模型");
      return;
    }
    setIsFusing(true);
    setFusedResult(null);
    setShowDiff(false);
    try {
      const res = await invoke<{
        draft_id: string;
        name: string;
        description: string;
        fused_code: string;
        explanation: string;
        conflicts: string[];
        status: string;
      }>("fuse_skills_api", { skills: furnacePot, modelId: selectedFusionModel });
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
      const approvedName = fusedSaveName.replace(/\s+/g, "_");
      await invoke("apply_skill_fusion_draft", {
        draftId: fusedResult.draft_id,
        approvedName,
      });

      toast.success("融合超级技能资产已成功写入技能库！已重建血统拓扑。");
      setFurnacePot([]);
      setFusedResult(null);
      setShowDiff(false);
      setSelectedSkillName(approvedName);
      setSelectedProfile("Core");
      await loadSkills();
    } catch (e) {
      toast.error("写入本地库失败：" + e);
    }
  };

  const handlePreviewMarketSkill = async (item: MarketSkill) => {
    setSelectedMarketSkill(item);
    setMarketPreview(null);
    setMarketImportConflict(false);
    setIsLoadingMarketPreview(true);
    try {
      setMarketPreview(await skillLibraryApi.previewMarket(item));
    } catch (e) {
      toast.error("读取真实 SKILL.md 失败：" + e);
    } finally {
      setIsLoadingMarketPreview(false);
    }
  };

  const handleRejectFusion = async () => {
    if (!fusedResult) return;
    try {
      await invoke("reject_skill_fusion_draft", { draftId: fusedResult.draft_id });
      setFusedResult(null);
      setShowDiff(false);
      toast.success("融合草案已拒绝，未写入技能库");
    } catch (error) {
      toast.error("拒绝草案失败：" + error);
    }
  };

  const handleImportMarketSkill = async (overwrite = false) => {
    if (!selectedMarketSkill || !marketPreview) return;
    try {
      const imported = await skillLibraryApi.importMarket(selectedMarketSkill, overwrite);
      setMarketImportConflict(false);
      toast.success(`已导入真实技能：${imported}`);
      await loadSkills();
    } catch (e) {
      const message = String(e);
      if (message.includes("已存在")) setMarketImportConflict(true);
      toast.error("导入市场技能失败：" + message);
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

    const cat = s.category || getCategory(s.name);
    const matchesCategory = categoryFilter === "all" || cat === categoryFilter;
    const matchesStarred = !showStarredOnly || s.starred;

    return matchesSearch && matchesCategory && matchesStarred;
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
    <div className="flex h-full min-h-0 w-full flex-col gap-4 overflow-hidden">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-1 pb-3">
        {SKILL_HUB_SECTIONS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={cn(
              "rounded-md border px-3 py-2 text-left text-sm transition-colors",
              section === item.id
                ? "border-primary/40 bg-primary/10 text-primary"
                : "border-border bg-card/40 text-muted-foreground hover:bg-muted/20"
            )}
            onClick={() => setSection(item.id)}
            title={item.desc}
          >
            {item.label}
          </button>
        ))}
      </div>

    <div className="skill-hub-layout grid min-h-0 w-full flex-1 grid-cols-1 gap-5 overflow-y-auto lg:grid-cols-[260px_minmax(0,1fr)] lg:overflow-hidden xl:grid-cols-[280px_minmax(0,1fr)]">

      {/* Left panel: list of skills + tool status */}
      <div className="card flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-4">
        <h3 className="card-title flex items-center gap-1.5 text-sm">
          <BookOpen size={16} color="var(--color-secondary)" />
          技能列表 ({filteredSkills.length})
        </h3>

        {/* Quick sync + scan + git buttons */}
        <div className="flex gap-1.5 mb-3 mt-2">
          <button
            className="btn btn-secondary py-1 px-2 text-xs flex items-center gap-1 flex-1"
            onClick={handleScanDisk}
            disabled={isScanning}
          >
            <HardDrive size={11} />
            {isScanning ? "扫描中..." : "扫描磁盘"}
          </button>
          <button
            className="btn btn-secondary py-1 px-2 text-xs flex items-center gap-1 flex-1"
            onClick={() => setShowGitPanel(!showGitPanel)}
          >
            <Upload size={11} />
            Git 源
          </button>
          <button
            className="btn btn-secondary py-1 px-2 text-xs flex items-center gap-1"
            onClick={loadToolStatuses}
            aria-label="刷新工具状态"
          >
            <RefreshCw size={11} />
          </button>
        </div>

        {/* P5: Git Source Panel (collapsible) */}
        {showGitPanel && (
          <div className="mb-3 p-3 bg-purple-500/5 border border-purple-500/15 rounded-lg">
            <h4 className="text-xs font-medium text-purple-400 mb-2">🌐 Git 技能源</h4>
            <input
              type="text"
              className="form-input text-xs py-1 px-2 mb-2 w-full"
              placeholder="https://github.com/user/skill-repo"
              value={gitRepoUrl}
              onChange={(e) => setGitRepoUrl(e.target.value)}
            />
            <div className="flex gap-2 mb-2">
              <input
                type="text"
                className="form-input text-xs py-1 px-2 flex-1"
                placeholder="分支 (默认 main)"
                value={gitBranch}
                onChange={(e) => setGitBranch(e.target.value)}
              />
              <button
                className="btn py-1 px-3 text-xs"
                onClick={handleCloneRepo}
                disabled={isCloning}
              >
                {isCloning ? "克隆中..." : "克隆"}
              </button>
            </div>

            {/* Candidates list */}
            {gitCandidates.length > 0 && (
              <div className="flex flex-col gap-1.5">
                <span className="text-xs text-secondary-foreground">发现 {gitCandidates.length} 个技能：</span>
                {gitCandidates.map(c => (
                  <div key={c.name} className="flex items-center justify-between bg-muted/10 border border-border rounded py-1 px-2">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium">{c.name}</span>
                      {c.already_imported && <span className="text-xs text-[var(--color-success)]">已导入</span>}
                    </div>
                    {!c.already_imported && (
                      <button
                        className="btn btn-secondary py-0.5 px-2 text-xs"
                        onClick={() => handleImportGitSkill(c)}
                      >
                        导入
                      </button>
                    )}
                  </div>
                ))}
              </div>
            )}

            {/* Check updates button for existing git skills */}
            <button
              className="btn btn-secondary py-0.5 px-2 text-xs mt-2 w-full"
              onClick={handleCheckGitUpdates}
            >
              <RefreshCw size={10} /> 检查 Git 技能更新
            </button>
          </div>
        )}

        {/* Search */}
        <input
          type="text"
          className="form-input mb-3 text-sm py-1.5 px-2.5"
          placeholder="搜索技能名称..."
          value={searchKeyword}
          onChange={(e) => setSearchKeyword(e.target.value)}
        />

        {/* Category selector + Starred filter */}
        <div className="mb-3">
          <div className="flex items-center gap-2 mb-1">
            <label className="text-xs text-secondary-foreground">分类过滤</label>
            <button
              className={cn(
                "flex items-center gap-1 text-xs py-0.5 px-1.5 rounded-full border ml-auto",
                showStarredOnly
                  ? "bg-amber-500/12 border-amber-500/40 text-amber-400"
                  : "bg-muted/10 border-border text-muted-foreground"
              )}
              onClick={() => setShowStarredOnly(!showStarredOnly)}
            >
              <Star size={10} fill={showStarredOnly ? "currentColor" : "none"} />
              收藏
            </button>
          </div>
          <select
            className="form-input text-xs py-1 px-2 bg-muted/15 w-full"
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
          {filteredSkills.map((sk) => {
            return (
            <div
              key={sk.name}
              className={cn(
                "nav-item flex flex-col items-start py-2 px-3 rounded-lg border",
                selectedSkillName === sk.name ? "active bg-cyan-500/8 border-[var(--color-secondary)]" : "bg-muted/5 border-transparent"
              )}
              onClick={() => {
                setSelectedSkillName(sk.name);
                setSelectedProfile(sk.profile);
              }}
            >
              <div className="flex justify-between w-full items-center mb-1">
                <div className="flex items-center gap-1.5">
                  <span
                    className={cn("cursor-pointer text-xs", sk.starred ? "text-amber-400" : "text-muted-foreground/30")}
                    onClick={(e) => { e.stopPropagation(); handleToggleStarred(sk.name); }}
                  >
                    <Star size={12} fill={sk.starred ? "currentColor" : "none"} />
                  </span>
                  <span className="font-medium text-sm">{sk.name}</span>
                </div>
                <div className="flex items-center gap-1.5">
                  {/* Source type badge */}
                  {sk.source_type === "git" && (
                    <span className="text-xs bg-purple-500/15 text-purple-400 py-px px-1.5 rounded-full">Git</span>
                  )}
                  <span
                    className={cn(
                      "inline-block w-1.5 h-1.5 rounded-full",
                      sk.is_active ? "bg-[var(--color-success)]" : "bg-muted/55"
                    )}
                  />
                </div>
              </div>
              <span className="text-xs text-secondary-foreground overflow-hidden text-ellipsis line-clamp-1">
                {sk.description}
              </span>
            </div>
            );
          })}
          {filteredSkills.length === 0 && (
            <div className="text-center py-5 text-muted-foreground text-xs">
              未找到匹配的技能
            </div>
          )}
        </div>
      </div>

      {/* Right workspace: split visual editor & topology graph */}
      <div className="flex h-full min-h-0 min-w-0 flex-col gap-4 overflow-y-auto pr-1.5 pb-4">

        {section === "pool" && <SkillPoolPanel />}

        {section === "library" && (selectedSkill ? (
          <div className="card flex min-h-[560px] flex-none flex-col overflow-hidden p-4">
            {/* Header section — responsive header that wraps action buttons on small windows */}
            <div className="flex flex-col gap-3 border-b border-border pb-3 mb-4">
              <div className="flex justify-between items-start gap-3 flex-wrap">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <h2 className="text-lg font-semibold text-foreground m-0 truncate">{selectedSkill.name}</h2>
                    <span className="text-xs py-0.5 px-2 rounded-md bg-muted/15 border border-border text-secondary-foreground shrink-0">{getCategory(selectedSkill.name)}</span>
                  </div>
                  <p className="text-secondary-foreground text-xs mt-1 m-0 line-clamp-2">{selectedSkill.description}</p>
                </div>
              </div>

              {/* Toolbar row — wraps gracefully on narrow widths */}
              <div className="flex gap-2 items-center flex-wrap">
                {/* Active switch */}
                <label className="flex items-center gap-1.5 bg-muted/10 py-1.5 px-2.5 rounded-md border border-border cursor-pointer text-xs">
                  <input
                    type="checkbox"
                    checked={selectedSkill.is_active}
                    onChange={() => handleToggleActive(selectedSkill.name, selectedSkill.is_active)}
                    className="cursor-pointer w-3.5 h-3.5"
                  />
                  <span className="text-secondary-foreground">激活</span>
                </label>

                {/* Profile select */}
                <div className="flex items-center gap-1.5 bg-muted/10 py-1 px-2 rounded-md border border-border">
                  <span className="text-xs text-secondary-foreground whitespace-nowrap">变体:</span>
                  <select
                    className="bg-transparent border-none text-xs text-foreground cursor-pointer focus:outline-none"
                    value={selectedProfile}
                    onChange={(e) => {
                      const newProf = e.target.value;
                      setSelectedProfile(newProf);
                      handleUpdateProfile(selectedSkill.name, newProf);
                    }}
                  >
                    <option value="Minimal">Minimal</option>
                    <option value="Core">Core</option>
                    <option value="Comprehensive">Comprehensive</option>
                  </select>
                </div>

                <button className="btn btn-secondary py-1.5 px-3 text-xs h-8 whitespace-nowrap" onClick={addToFurnace}>
                  🔥 融合炉
                </button>

                {/* P6: Export package */}
                <button
                  className="btn btn-secondary py-1.5 px-3 text-xs h-8 flex items-center gap-1 whitespace-nowrap"
                  onClick={async () => {
                    try {
                      const path = await skillSyncApi.exportPackage(selectedSkill.name);
                      toast.success(`已导出到 ${path}`);
                    } catch (e) {
                      toast.error("导出失败：" + e);
                    }
                  }}
                >
                  <Download size={12} />
                  导出包
                </button>
              </div>
            </div>

            {/* ── P3: Tool Sync Panel ── */}
            <div className="border-b border-border pb-3 mb-4">
              <div className="flex justify-between items-center mb-2">
                <span className="text-xs font-medium text-secondary-foreground flex items-center gap-1">
                  <ArrowRightLeft size={13} />
                  同步到工具
                </span>
                <button
                  className="btn py-0.5 px-2.5 text-xs"
                  onClick={() => handleSyncSkill(selectedSkill.name)}
                  disabled={isSyncing}
                >
                  {isSyncing ? "同步中..." : "↗ 同步选中"}
                </button>
              </div>
              <div className="flex flex-wrap gap-2">
                {toolStatuses.map(ts => {
                  const checked = (syncTargets[selectedSkill.name] || []).includes(ts.tool_id);
                  return (
                    <label
                      key={ts.tool_id}
                      className={cn(
                        "flex items-center gap-1.5 py-1 px-2.5 rounded-full border text-xs cursor-pointer transition-all",
                        ts.is_installed
                          ? checked
                            ? "bg-cyan-500/12 border-cyan-500/40 text-cyan-400"
                            : "bg-muted/10 border-border text-secondary-foreground"
                          : "bg-muted/5 border-border/50 text-muted-foreground opacity-50 cursor-not-allowed"
                      )}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        disabled={!ts.is_installed}
                        onChange={() => toggleSyncTarget(selectedSkill.name, ts.tool_id)}
                        className="w-3 h-3"
                      />
                      {ts.is_installed ? (
                        <CheckCircle2 size={11} className="text-[var(--color-success)]" />
                      ) : (
                        <XCircle size={11} className="text-muted-foreground/30" />
                      )}
                      {ts.display_name}
                    </label>
                  );
                })}
              </div>

              {/* Conflict resolution */}
              {conflicts.length > 0 && (
                <div className="mt-3 p-3 bg-amber-500/5 border border-amber-500/20 rounded-lg">
                  <p className="text-xs text-amber-400 font-medium mb-2 flex items-center gap-1">
                    <AlertCircle size={13} />
                    检测到 {conflicts.length} 个冲突
                  </p>
                  <div className="flex gap-2 mb-2">
                    {(["skip", "overwrite", "rename"] as const).map(s => (
                      <button
                        key={s}
                        className={cn(
                          "py-0.5 px-2 text-xs rounded-full border",
                          conflictStrategy === s
                            ? "bg-amber-500/15 border-amber-500/40 text-amber-400"
                            : "bg-muted/10 border-border text-secondary-foreground"
                        )}
                        onClick={() => setConflictStrategy(s)}
                      >
                        {s === "skip" ? "跳过" : s === "overwrite" ? "覆盖" : "重命名旧文件"}
                      </button>
                    ))}
                  </div>
                  <div className="flex gap-2">
                    <button
                      className="btn py-0.5 px-2.5 text-xs"
                      onClick={() => doSync(selectedSkill.name, syncTargets[selectedSkill.name] || [])}
                    >
                      确认同步
                    </button>
                    <button
                      className="btn btn-secondary py-0.5 px-2.5 text-xs"
                      onClick={() => setConflicts([])}
                    >
                      取消
                    </button>
                  </div>
                </div>
              )}
            </div>

            {/* Split workspace: Editor & Graph */}
            <div className="grid flex-1 min-h-0 grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1.2fr)_minmax(260px,1fr)] xl:grid-cols-[minmax(0,1.2fr)_minmax(280px,1fr)]">

              {/* Left side: Code content editor */}
              <div className="flex h-full min-h-0 flex-col">
                <div className="flex justify-between items-center mb-2">
                  <span className="text-xs font-medium text-secondary-foreground flex items-center gap-1">
                    <Layers size={13} />
                    Markdown 规则代码 ({selectedProfile})
                  </span>

                  <div className="flex gap-2">
                    {isEditing ? (
                      <>
                        <button
                          className="btn btn-secondary py-0.5 px-2 text-xs h-6"
                          onClick={() => loadSkillContent(selectedSkill.name, selectedProfile)}
                        >
                          取消
                        </button>
                        <button
                          className="btn py-0.5 px-2 text-xs h-6"
                          onClick={handleSaveContent}
                          disabled={isSaving}
                        >
                          {isSaving ? "保存中..." : "💾 保存"}
                        </button>
                      </>
                    ) : (
                      <button
                        className="btn btn-secondary py-0.5 px-2 text-xs h-6"
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
                      className="form-input w-full h-full font-mono text-xs bg-muted/15 resize-none text-foreground p-3 leading-normal"
                      value={skillContent}
                      onChange={(e) => setSkillContent(e.target.value)}
                    />
                  ) : (
                    <div
                      className="h-full w-full overflow-y-auto break-words rounded-lg border border-border bg-muted/10 p-3 font-mono text-xs leading-normal text-foreground whitespace-pre-wrap"
                    >
                      {skillContent}
                    </div>
                  )}
                </div>
              </div>

              {/* Right side: Force network */}
              <div className="flex h-full min-h-0 flex-col">
                <span className="text-xs font-medium text-secondary-foreground mb-2 flex items-center gap-1">
                  <Sparkles size={13} />
                  力导向关联拓扑图 (OBSIDIAN STYLE)
                </span>
                <div className="min-h-[280px] flex-1 overflow-hidden">
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
        ))}

        {/* Generate skill from workspace */}
        {section === "generate" && (
          <div className="card flex-none p-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <div className="text-sm font-semibold">从工作区生成技能</div>
                <p className="mt-1 text-xs text-muted-foreground">扫描一个项目，选择有代表性的文件，让模型据此生成一份技能草稿（SKILL.md），审阅后保存为本地技能。</p>
              </div>
              <button onClick={pickGenWorkspace} disabled={genBusy === "scan"} className="flex items-center gap-2 rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted/20">
                <FolderOpen className="h-4 w-4" />
                {genBusy === "scan" ? "扫描中…" : genWorkspace ? "重新选择工作区" : "选择工作区"}
              </button>
            </div>

            {genWorkspace && (
              <div className="mt-2 truncate text-xs text-muted-foreground" title={genWorkspace}>{genWorkspace}</div>
            )}

            {genFiles.length > 0 && (
              <>
                <div className="mt-3 flex items-center gap-2">
                  <input
                    value={genName}
                    onChange={(e) => setGenName(e.target.value)}
                    placeholder="技能名称，如 project-conventions"
                    className="h-9 flex-1 rounded-md border border-border bg-background px-3 text-sm"
                  />
                  <span className="text-xs text-muted-foreground">已选 {genSelected.size}/{genFiles.length}</span>
                  <button onClick={generateSkillDraft} disabled={genBusy === "generate" || !genName.trim() || genSelected.size === 0} className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50">
                    <Wand2 className="h-4 w-4" />
                    {genBusy === "generate" ? "生成中…" : "生成草稿"}
                  </button>
                </div>
                <div className="mt-3 max-h-56 overflow-auto rounded-md border border-border">
                  {genFiles.map((file) => (
                    <label key={file.path} className="flex cursor-pointer items-center gap-2 border-b border-border px-3 py-1.5 text-xs last:border-0 hover:bg-muted/10">
                      <input type="checkbox" checked={genSelected.has(file.path)} onChange={() => toggleGenFile(file.path)} />
                      <span className="flex-1 truncate" title={file.relativePath}>{file.relativePath}</span>
                      <span className="text-muted-foreground">{file.extension || "-"}</span>
                    </label>
                  ))}
                </div>
              </>
            )}

            {genWorkspace && genFiles.length === 0 && genBusy !== "scan" && (
              <div className="mt-3 text-center text-xs text-muted-foreground">该工作区没有扫描到可用文件</div>
            )}

            {genDraft && (
              <div className="mt-4 border-t border-border pt-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="text-sm font-semibold">草稿预览 · 分析了 {genDraft.files_analyzed} 个文件</div>
                  <div className="flex gap-2">
                    <button onClick={() => navigator.clipboard.writeText(genDraft.draft).then(() => toast.success("已复制"))} className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted/20">复制</button>
                    <button onClick={saveGeneratedSkill} disabled={genBusy === "save"} className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50">
                      {genBusy === "save" ? "保存中…" : "保存为本地技能"}
                    </button>
                  </div>
                </div>
                <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap rounded-md bg-muted p-3 text-xs">{genDraft.draft}</pre>
              </div>
            )}
          </div>
        )}

        {/* Fusion Furnace Drawer */}
        {section === "furnace" && <div className="card flex-none p-4">
          <div className="flex justify-between items-center">
            <div>
              <h3 className="card-title flex items-center gap-1.5 text-sm m-0">
                <Sparkles size={16} color="var(--color-warning)" />
                技能融合炉 (Fusion Furnace)
              </h3>
              <p className="text-secondary-foreground text-xs mt-0.5">
                至少将 2 个零散或冲突技能投入融合，AI 会进行知识蒸馏并合并冲突。
              </p>
            </div>

            <div className="flex items-center gap-2">
              <select
                value={selectedFusionModel}
                onChange={(event) => setSelectedFusionModel(event.target.value)}
                className="h-9 max-w-64 rounded-md border border-border bg-background px-2 text-xs"
              >
                {fusionModels.length === 0 && <option value="">没有启用模型</option>}
                {fusionModels.map((model) => (
                  <option key={model.id} value={`${model.platform_id}:${model.model_name}`}>
                    {model.model_name} · {model.platform_id}
                  </option>
                ))}
              </select>
              {furnacePot.length >= 2 && (
                <button className="btn" onClick={handleIgniteFusion} disabled={isFusing || !selectedFusionModel}>
                  {isFusing ? "生成融合草案中..." : "生成融合草案"}
                </button>
              )}
            </div>
          </div>

          {/* Selected slots */}
          <div className="flex flex-wrap gap-2 mt-3">
            {furnacePot.map((name) => (
              <div
                key={name}
                className="flex items-center gap-1.5 bg-amber-500/8 border border-amber-500/30 py-1 px-2.5 rounded-2xl text-xs text-amber-400"
              >
                <span>{name}</span>
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

          {/* F-D: SkillDAG conflict warning — these skills have a registered
              conflicts_with edge and may not compose cleanly. */}
          {dagConflicts.length > 0 && (
            <div className="mt-3 rounded-md border border-destructive/40 bg-destructive/5 p-3">
              <div className="mb-1 text-xs font-semibold text-destructive">⚠ 技能冲突（SkillDAG）</div>
              <ul className="space-y-0.5 text-xs text-muted-foreground">
                {dagConflicts.map((c, i) => (
                  <li key={i}>
                    <span className="text-foreground">{c.skill_a} ✕ {c.skill_b}</span>
                    {c.reason ? ` — ${c.reason}` : ""}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {/* Render visual diff on complete */}
          {showDiff && fusedResult && (
            <div className="border-t border-border mt-4 pt-4">
              <div className="flex justify-between items-center mb-3">
                <div>
                  <h4 className="text-emerald-500 text-sm font-bold">融合草案待审批</h4>
                  <p className="text-xs text-secondary-foreground mt-0.5">{fusedResult.explanation}</p>
                  {fusedResult.conflicts.length > 0 && (
                    <ul className="mt-2 space-y-1 text-xs text-warning">
                      {fusedResult.conflicts.map((conflict) => <li key={conflict}>{conflict}</li>)}
                    </ul>
                  )}
                </div>

                <div className="flex gap-2 items-center">
                  <input
                    type="text"
                    className="form-input text-xs py-1 px-2 w-40 bg-muted/15"
                    value={fusedSaveName}
                    onChange={(e) => setFusedSaveName(e.target.value)}
                    placeholder="超级技能 ID (例如: file_operations)"
                  />
                  <button className="btn py-1 px-3 text-xs" onClick={handleAcceptFusion}>
                    接受并写入本地库
                  </button>
                  <button className="btn btn-secondary py-1 px-3 text-xs" onClick={handleRejectFusion}>
                    拒绝草案
                  </button>
                </div>
              </div>

              {renderSplitDiff()}
            </div>
          )}
        </div>}

        {section === "sets" && (
          <div className="card flex-none p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="card-title flex items-center gap-1.5 text-sm m-0">
                  <Layers size={16} color="var(--color-secondary)" />
                  技能组合 (Skill Set)
                </h3>
                <p className="mt-1 text-xs leading-5 text-secondary-foreground">
                  将多个技能保存为一个组合，并一键同步到 Claude、Codex、Gemini、OpenCode 等工具。
                </p>
              </div>
              <button className="btn btn-secondary py-1 px-3 text-xs" onClick={loadSkillSets}>
                刷新组合
              </button>
            </div>

            <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,360px)_minmax(0,1fr)]">
              <div className="rounded-lg border border-border bg-muted/10 p-3">
                <div className="mb-2 text-xs font-semibold">新建组合</div>
                <input
                  className="form-input mb-2 text-sm"
                  value={skillSetName}
                  onChange={(e) => setSkillSetName(e.target.value)}
                  placeholder="组合名称，例如 前端重构套件"
                />
                <textarea
                  className="form-input mb-3 min-h-20 text-sm"
                  value={skillSetDescription}
                  onChange={(e) => setSkillSetDescription(e.target.value)}
                  placeholder="组合用途说明"
                />
                <div className="mb-2 text-xs text-muted-foreground">
                  当前将保存：{(furnacePot.length > 0 ? furnacePot : selectedSkillName ? [selectedSkillName] : []).join(", ") || "未选择技能"}
                </div>
                <div className="mb-3 flex flex-wrap gap-2">
                  {toolStatuses.map((tool) => (
                    <button
                      key={tool.tool_id}
                      type="button"
                      className={cn(
                        "rounded-md border px-2 py-1 text-xs",
                        skillSetTargets.includes(tool.tool_id)
                          ? "border-primary/40 bg-primary/10 text-primary"
                          : "border-border bg-background/60 text-muted-foreground"
                      )}
                      onClick={() => toggleSkillSetTarget(tool.tool_id)}
                    >
                      {tool.display_name}
                    </button>
                  ))}
                </div>
                <button className="btn w-full py-2 text-sm" onClick={handleCreateSkillSet} disabled={isSavingSkillSet}>
                  {isSavingSkillSet ? "保存中..." : "保存组合"}
                </button>
              </div>

              <div className="grid gap-3">
                {skillSets.length === 0 ? (
                  <div className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
                    还没有技能组合。先把常用技能放进熔炉，或选中左侧技能后创建组合。
                  </div>
                ) : skillSets.map((set) => (
                  <div key={set.id} className="rounded-lg border border-border bg-muted/10 p-3">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-sm font-semibold">{set.name}</div>
                        <p className="mt-1 line-clamp-2 text-xs text-secondary-foreground">{set.description || "无描述"}</p>
                      </div>
                      <div className="flex gap-2">
                        <button className="btn btn-secondary py-1 px-2 text-xs" onClick={() => handleSyncSkillSet(set)} disabled={isSyncing}>
                          同步
                        </button>
                        <button className="btn btn-secondary py-1 px-2 text-xs" onClick={() => handleDeleteSkillSet(set.id)}>
                          删除
                        </button>
                      </div>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      {set.items.map((item) => (
                        <span key={item.id} className="rounded-full border border-border bg-background/70 px-2 py-1 text-xs">
                          {item.skill_id}
                        </span>
                      ))}
                    </div>
                    <div className="mt-2 text-xs text-muted-foreground">
                      目标：{set.sync_targets.length > 0 ? set.sync_targets.join(", ") : "未设置"}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Skill Marketplace */}
        {section === "market" && <div className="card flex-none p-4">
          <h3 className="card-title flex items-center gap-1.5 text-sm">
            <Download size={16} color="var(--color-secondary)" />
            公共技能市场 (Skill Marketplace)
          </h3>
          <p className="text-secondary-foreground text-xs mt-0.5 mb-3">
            搜索真实 Git 来源。必须先读取并预览远程 SKILL.md，确认后才写入本地技能库。
          </p>

          <div className="mb-4 flex flex-col gap-2 sm:flex-row">
            <input
              type="text"
              className="form-input text-sm"
              placeholder="搜索技能市场，例如 code review、git、frontend"
              value={marketQuery}
              onChange={(e) => setMarketQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  handleSearchMarket();
                }
              }}
            />
            <button className="btn shrink-0 px-4 py-2 text-sm" onClick={handleSearchMarket} disabled={isSearchingMarket}>
              {isSearchingMarket ? "搜索中..." : "搜索"}
            </button>
          </div>

          {selectedMarketSkill && (
            <div className="mb-4 rounded-lg border border-primary/20 bg-primary/5 p-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-sm font-semibold">{selectedMarketSkill.name}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {selectedMarketSkill.source} · {selectedMarketSkill.author || "unknown"} · {selectedMarketSkill.stars ?? 0} stars
                  </div>
                </div>
                <div className="flex gap-2">
                  <button className="btn btn-secondary py-1 px-3 text-xs" onClick={() => handlePreviewMarketSkill(selectedMarketSkill)} disabled={isLoadingMarketPreview}>
                    {isLoadingMarketPreview ? "读取中..." : "预览 SKILL.md"}
                  </button>
                  <button className="btn py-1 px-3 text-xs" onClick={() => handleImportMarketSkill(false)} disabled={!marketPreview}>
                    确认导入
                  </button>
                  {marketImportConflict && (
                    <button className="btn py-1 px-3 text-xs" onClick={() => handleImportMarketSkill(true)}>
                      覆盖导入
                    </button>
                  )}
                </div>
              </div>
              <p className="mt-2 text-xs leading-5 text-secondary-foreground">{selectedMarketSkill.description}</p>
              <div className="mt-2 truncate text-xs text-muted-foreground">{selectedMarketSkill.url}</div>
              {marketPreview && marketPreview.skill.url === selectedMarketSkill.url && (
                <div className="mt-3 border-t border-border pt-3">
                  <div className="mb-2 text-xs text-muted-foreground">
                    commit/sha: {selectedMarketSkill.content_sha || selectedMarketSkill.revision} · hash: {marketPreview.content_hash}
                  </div>
                  <pre className="max-h-80 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background/60 p-3 text-xs leading-5">
                    {marketPreview.content}
                  </pre>
                </div>
              )}
            </div>
          )}

          {marketResults.length > 0 && (
            <div className="mb-4 grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-3">
              {marketResults.map((item) => (
                <button
                  key={`${item.source}-${item.name}-${item.url}`}
                  type="button"
                  className={cn(
                    "rounded-lg border p-3 text-left",
                    selectedMarketSkill?.url === item.url ? "border-primary/40 bg-primary/10" : "border-border bg-muted/10"
                  )}
                  onClick={() => handlePreviewMarketSkill(item)}
                >
                  <div className="text-xs font-semibold">{item.name}</div>
                  <p className="mt-1 line-clamp-3 text-xs leading-snug text-secondary-foreground">{item.description}</p>
                  <div className="mt-2 text-xs text-muted-foreground">{item.source} · {item.stars ?? 0} stars</div>
                </button>
              ))}
            </div>
          )}

          {marketResults.length === 0 && !isSearchingMarket && (
            <div className="rounded-md border border-dashed border-border p-6 text-center text-xs text-muted-foreground">
              搜索 GitHub 上的 SKILL.md 后再选择预览。
            </div>
          )}
        </div>}

        {/* ── P3: Disk Scanner Results ── */}
        {section === "sync" && (
          <div className="card flex-none p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="card-title flex items-center gap-1.5 text-sm m-0">
                  <RefreshCw size={16} color="var(--color-secondary)" />
                  技能同步 · 统一技能库
                </h3>
                <p className="mt-1 text-xs leading-5 text-secondary-foreground">
                  「一键统一」：扫描全盘技能 → 未纳管的收进中央库 <code>~/.omnix/skills</code> → 让所有已安装 Agent 软链共享同一份（改一处处处生效；Windows 无软链权限时自动回退为复制）。
                </p>
              </div>
              <div className="flex gap-2">
                <button className="btn btn-primary py-1 px-3 text-xs" onClick={handleUnifyAll} disabled={isUnifying || isScanning} title="扫描全盘 → 纳入中央库 → 全 Agent 软链共享">
                  {isUnifying ? "统一中..." : "🧩 一键统一技能库"}
                </button>
                <button className="btn btn-secondary py-1 px-3 text-xs" onClick={handleScanDisk} disabled={isScanning || isUnifying}>
                  {isScanning ? "扫描中..." : "仅扫描"}
                </button>
                <button className="btn btn-secondary py-1 px-3 text-xs" onClick={() => setShowGitPanel(!showGitPanel)}>
                  Git 源
                </button>
              </div>
            </div>
            <div className="mt-4 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
              {toolStatuses.map((tool) => (
                <div key={tool.tool_id} className="rounded-lg border border-border bg-muted/10 p-3">
                  <div className="text-sm font-semibold">{tool.display_name}</div>
                  <div className={cn("mt-2 text-xs", tool.is_installed ? "text-[var(--color-success)]" : "text-muted-foreground")}>
                    {tool.is_installed ? "已检测" : "未检测"} · {tool.verification === "verified" ? "已验证同步" : "实验性适配"}
                  </div>
                  <div className="mt-1 text-xs leading-5 text-muted-foreground">{tool.verification_note}</div>
                  <div className="mt-2 truncate text-xs text-muted-foreground">{tool.skill_base_path}</div>
                </div>
              ))}
            </div>
          </div>
        )}

        {section === "sync" && showScanPanel && scanReport && (
          <div className="card flex-none p-4">
            <div className="flex justify-between items-center mb-3">
              <h3 className="card-title flex items-center gap-1.5 text-sm m-0">
                <Search size={16} color="var(--color-secondary)" />
                磁盘扫描结果 ({scanReport.total_found})
              </h3>
              <button className="btn btn-secondary py-0.5 px-2 text-xs" onClick={() => setShowScanPanel(false)}>
                关闭
              </button>
            </div>

            {/* Tools scanned */}
            <div className="flex flex-wrap gap-2 mb-4">
              {scanReport.tools_scanned.map(t => (
                <div
                  key={t.tool_id}
                  className={cn(
                    "flex items-center gap-1.5 py-0.5 px-2 rounded-full text-xs border",
                    t.is_installed
                      ? "bg-[var(--color-success)]/8 border-[var(--color-success)]/20 text-[var(--color-success)]"
                      : "bg-muted/5 border-border/50 text-muted-foreground"
                  )}
                >
                  {t.is_installed ? <CheckCircle2 size={10} /> : <XCircle size={10} />}
                  {t.display_name} ({t.skill_count})
                </div>
              ))}
            </div>

            {/* Unmanaged skills — candidates for import */}
            {scanReport.unmanaged.length > 0 && (
              <div className="mb-4">
                <h4 className="text-xs font-medium text-amber-400 mb-2 flex items-center gap-1">
                  <AlertCircle size={12} />
                  未管理技能 ({scanReport.unmanaged.length}) — 建议导入
                </h4>
                <div className="flex flex-col gap-1.5">
                  {scanReport.unmanaged.map(item => (
                    <div key={`${item.tool_id}-${item.name}`} className="flex items-center justify-between bg-amber-500/5 border border-amber-500/15 rounded-lg py-1.5 px-3">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium">{item.name}</span>
                        <span className="text-xs text-muted-foreground">← {item.tool_display_name}</span>
                        {item.preview && <span className="text-xs text-secondary-foreground truncate max-w-[200px]">{item.preview}</span>}
                      </div>
                      <button
                        className="btn btn-secondary py-0.5 px-2 text-xs"
                        onClick={() => handleImportUnmanaged([item])}
                      >
                        <Upload size={10} /> 导入
                      </button>
                    </div>
                  ))}
                  {scanReport.unmanaged.length > 1 && (
                    <button
                      className="btn py-1 px-3 text-xs mt-1"
                      onClick={() => handleImportUnmanaged(scanReport.unmanaged)}
                    >
                      一键导入全部 ({scanReport.unmanaged.length})
                    </button>
                  )}
                </div>
              </div>
            )}

            {/* Drifted skills */}
            {scanReport.drifted.length > 0 && (
              <div className="mb-4">
                <h4 className="text-xs font-medium text-blue-400 mb-2 flex items-center gap-1">
                  <RefreshCw size={12} />
                  漂移技能 ({scanReport.drifted.length}) — 需要更新
                </h4>
                <div className="flex flex-col gap-1.5">
                  {scanReport.drifted.map(item => (
                    <div key={`${item.tool_id}-${item.name}`} className="flex items-center justify-between bg-blue-500/5 border border-blue-500/15 rounded-lg py-1.5 px-3">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium">{item.name}</span>
                        <span className="text-xs text-muted-foreground">→ {item.tool_display_name}</span>
                      </div>
                      <span className="text-xs text-blue-400">版本不一致</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Orphaned skills */}
            {scanReport.orphaned.length > 0 && (
              <div className="mb-4">
                <h4 className="text-xs font-medium text-red-400 mb-2">孤儿技能 ({scanReport.orphaned.length}) — 文件丢失</h4>
                <div className="flex flex-col gap-1.5">
                  {scanReport.orphaned.map(item => (
                    <div key={`${item.tool_id}-${item.name}`} className="flex items-center justify-between bg-red-500/5 border border-red-500/15 rounded-lg py-1.5 px-3">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-medium">{item.name}</span>
                        <span className="text-xs text-muted-foreground">→ {item.tool_display_name}</span>
                      </div>
                      <span className="text-xs text-red-400">目标文件丢失</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Managed — all good */}
            {scanReport.managed.length > 0 && (
              <div>
                <h4 className="text-xs font-medium text-[var(--color-success)] mb-2 flex items-center gap-1">
                  <CheckCircle2 size={12} />
                  已管理技能 ({scanReport.managed.length}) — 同步正常
                </h4>
                <div className="flex flex-wrap gap-1.5">
                  {scanReport.managed.map(item => (
                    <span key={`${item.tool_id}-${item.name}`} className="text-xs bg-[var(--color-success)]/5 text-[var(--color-success)] py-0.5 px-2 rounded-full border border-[var(--color-success)]/15">
                      {item.name} ({item.tool_display_name})
                    </span>
                  ))}
                </div>
              </div>
            )}

            {scanReport.total_found === 0 && (
              <div className="text-center py-6 text-muted-foreground text-xs">
                未发现任何技能文件。请确保已安装 AI 工具并添加技能。
              </div>
            )}
          </div>
        )}

      </div>

    </div>
    </div>
  );
};
