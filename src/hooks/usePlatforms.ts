/**
 * usePlatforms — Model platform and model CRUD management
 *
 * Manages: platforms, selectedPlatformId, platformModels, modelTestingState,
 * fetchingModels, activeModels, platform/model form state and modals
 */

import { useState, useCallback } from "react";
import { platformApi, modelApi } from "@/lib/tauri-api";
import type { ModelPlatform, PlatformModel, ModelTestState, ProviderType, HealthCheckDetail } from "@/types";

type CapabilityField = "vision" | "audio" | "reasoning" | "coding" | "long_context" | "tool_use" | "embedding" | "speedy";

interface PlatformFormState {
  name: string;
  api_type: ProviderType;
  api_key: string;
  api_address: string;
}

interface ModelFormState {
  model_name: string;
  has_vision: boolean;
  has_audio: boolean;
  has_reasoning: boolean;
  has_coding: boolean;
  has_long_context: boolean;
  has_tool_use: boolean;
  has_embedding: boolean;
  has_speedy: boolean;
}

const EMPTY_PLATFORM_FORM: PlatformFormState = {
  name: "",
  api_type: "openai",
  api_key: "",
  api_address: "",
};

const EMPTY_MODEL_FORM: ModelFormState = {
  model_name: "",
  has_vision: false,
  has_audio: false,
  has_reasoning: false,
  has_coding: true,
  has_long_context: false,
  has_tool_use: true,
  has_embedding: false,
  has_speedy: false,
};

export interface UsePlatformsReturn {
  // State
  platforms: ModelPlatform[];
  selectedPlatformId: string;
  platformModels: PlatformModel[];
  modelTestingState: Record<string, ModelTestState>;
  fetchingModels: boolean;
  activeModels: PlatformModel[];
  batchTesting: Record<string, boolean>;
  showPlatformModal: boolean;
  editingPlatform: ModelPlatform | null;
  platformForm: PlatformFormState;
  showModelModal: boolean;
  modelForm: ModelFormState;

  // Platform actions
  loadPlatforms: () => Promise<void>;
  selectPlatform: (id: string) => Promise<void>;
  togglePlatform: (plat: ModelPlatform) => Promise<void>;
  savePlatform: () => Promise<void>;
  deletePlatform: (id: string) => Promise<void>;
  fetchRemoteModels: () => Promise<void>;
  batchTestModels: (platformId: string) => Promise<void>;
  openPlatformModal: (plat?: ModelPlatform | null) => void;
  closePlatformModal: () => void;
  updatePlatformForm: (field: keyof PlatformFormState, value: string) => void;

  // Model actions
  toggleModelEnabled: (model: PlatformModel) => Promise<void>;
  toggleCapability: (model: PlatformModel, field: CapabilityField) => Promise<void>;
  testModel: (modelId: string) => Promise<HealthCheckDetail>;
  saveCustomModel: () => Promise<void>;
  deleteModel: (id: string) => Promise<void>;
  loadActiveModels: () => Promise<void>;
  openModelModal: () => void;
  closeModelModal: () => void;
  updateModelForm: (field: keyof ModelFormState, value: string | boolean) => void;
}

export function usePlatforms(): UsePlatformsReturn {
  const [platforms, setPlatforms] = useState<ModelPlatform[]>([]);
  const [selectedPlatformId, setSelectedPlatformId] = useState("");
  const [platformModels, setPlatformModels] = useState<PlatformModel[]>([]);
  const [modelTestingState, setModelTestingState] = useState<Record<string, ModelTestState>>({});
  const [fetchingModels, setFetchingModels] = useState(false);
  const [activeModels, setActiveModels] = useState<PlatformModel[]>([]);
  const [batchTesting, setBatchTesting] = useState<Record<string, boolean>>({});

  // Platform modal state
  const [showPlatformModal, setShowPlatformModal] = useState(false);
  const [editingPlatform, setEditingPlatform] = useState<ModelPlatform | null>(null);
  const [platformForm, setPlatformForm] = useState<PlatformFormState>(EMPTY_PLATFORM_FORM);

  // Model modal state
  const [showModelModal, setShowModelModal] = useState(false);
  const [modelForm, setModelForm] = useState<ModelFormState>(EMPTY_MODEL_FORM);

  // ── Active Models (must be declared first — used by other callbacks) ──

  const loadActiveModels = useCallback(async () => {
    try {
      const list = await modelApi.getActive();
      setActiveModels(list);
    } catch (e) {
      console.error("[usePlatforms] Failed to load active models:", e);
    }
  }, []);

  // ── Platform CRUD ──────────────────────────────────

  const loadPlatforms = useCallback(async () => {
    try {
      const list = await platformApi.list();
      setPlatforms(list);
      if (list.length > 0 && !selectedPlatformId) {
        selectPlatform(list[0].id);
      }
    } catch (e) {
      console.error("[usePlatforms] Failed to load platforms:", e);
    }
  }, [selectedPlatformId]);

  const selectPlatform = useCallback(async (id: string) => {
    setSelectedPlatformId(id);
    try {
      const models = await modelApi.listByPlatform(id);
      setPlatformModels(models);
    } catch (e) {
      console.error("[usePlatforms] Failed to load models for platform:", id, e);
    }
  }, []);

  const togglePlatform = useCallback(async (plat: ModelPlatform) => {
    const updated: ModelPlatform = { ...plat, is_enabled: !plat.is_enabled };
    try {
      await platformApi.save(updated);
      await loadPlatforms();
      await loadActiveModels();
    } catch (e) {
      console.error("[usePlatforms] Failed to toggle platform:", e);
    }
  }, [loadPlatforms, loadActiveModels]);

  const savePlatform = useCallback(async () => {
    if (!platformForm.name.trim() || !platformForm.api_address.trim()) {
      throw new Error("请填写平台显示名称和 API 基底地址");
    }

    const id = editingPlatform ? editingPlatform.id : `plat_${Date.now()}`;
    const newPlatform: ModelPlatform = {
      id,
      name: platformForm.name,
      api_type: platformForm.api_type,
      api_key: platformForm.api_key,
      api_address: platformForm.api_address,
      is_enabled: true,
    };

    await platformApi.save(newPlatform);

    // Auto-add API key to platform_api_keys table if provided
    // (handles the case where user types a key in the simple input for new platforms)
    if (platformForm.api_key.trim() && platformForm.api_type !== "ollama") {
      try {
        const { apiKeyApi } = await import("@/lib/tauri-api");
        // Check if this platform already has keys
        const existing = await apiKeyApi.list(id);
        if (existing.length === 0) {
          await apiKeyApi.add(id, platformForm.api_key.trim(), "主 Key");
        }
      } catch (e) {
        console.error("[usePlatforms] Failed to auto-add API key:", e);
      }
    }

    setShowPlatformModal(false);
    setEditingPlatform(null);
    setPlatformForm(EMPTY_PLATFORM_FORM);
    await loadPlatforms();
    selectPlatform(id);
  }, [platformForm, editingPlatform, loadPlatforms, selectPlatform]);

  const deletePlatform = useCallback(async (id: string) => {
    await platformApi.delete(id);
    setSelectedPlatformId("");
    await loadPlatforms();
  }, [loadPlatforms]);

  const fetchRemoteModels = useCallback(async () => {
    if (!selectedPlatformId) return;
    setFetchingModels(true);
    try {
      const imported = await platformApi.fetchRemoteModels(selectedPlatformId);
      setPlatformModels(imported);
      await loadActiveModels();
    } finally {
      setFetchingModels(false);
    }
  }, [selectedPlatformId, loadActiveModels]);

  const batchTestModels = useCallback(async (platformId: string) => {
    setBatchTesting((prev) => ({ ...prev, [platformId]: true }));
    try {
      const updated = await modelApi.batchCheck(platformId);
      setPlatformModels(updated);
      // Sync testing state from model status values
      const newTestingState: Record<string, ModelTestState> = {};
      for (const m of updated) {
        // Map backend status string to ModelTestState
        const s = m.status;
        newTestingState[m.id] = (
          s === "success" || s === "auth_error" || s === "rate_limited" ||
          s === "error" || s === "unreachable" || s === "no_api_key"
        ) ? s as ModelTestState : "idle";
      }
      setModelTestingState((prev) => ({ ...prev, ...newTestingState }));
    } catch (e) {
      console.error("[usePlatforms] Batch test failed:", e);
      const { toast } = await import("@/components/ui/sonner");
      toast.error("健康检测失败：" + String(e));
    } finally {
      setBatchTesting((prev) => ({ ...prev, [platformId]: false }));
    }
  }, []);

  const openPlatformModal = useCallback((plat?: ModelPlatform | null) => {
    if (plat) {
      setEditingPlatform(plat);
      setPlatformForm({
        name: plat.name,
        api_type: plat.api_type,
        api_key: plat.api_key,
        api_address: plat.api_address,
      });
    } else {
      setEditingPlatform(null);
      setPlatformForm(EMPTY_PLATFORM_FORM);
    }
    setShowPlatformModal(true);
  }, []);

  const closePlatformModal = useCallback(() => {
    setShowPlatformModal(false);
    setEditingPlatform(null);
    setPlatformForm(EMPTY_PLATFORM_FORM);
  }, []);

  const updatePlatformForm = useCallback((field: keyof PlatformFormState, value: string) => {
    setPlatformForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  // ── Model CRUD ────────────────────────────────────

  const toggleModelEnabled = useCallback(async (model: PlatformModel) => {
    const updated: PlatformModel = { ...model, is_enabled: !model.is_enabled };
    await modelApi.save(updated);
    if (selectedPlatformId) await selectPlatform(selectedPlatformId);
    await loadActiveModels();
  }, [selectedPlatformId, selectPlatform, loadActiveModels]);

  const toggleCapability = useCallback(
    async (model: PlatformModel, field: CapabilityField) => {
      const key = `has_${field}` as keyof PlatformModel; // field is CapabilityField which maps to has_* boolean fields
      const updated = { ...model, [key]: !model[key] };
      await modelApi.save(updated);
      if (selectedPlatformId) await selectPlatform(selectedPlatformId);
    },
    [selectedPlatformId, selectPlatform]
  );

  const testModel = useCallback(async (modelId: string) => {
    setModelTestingState((prev) => ({ ...prev, [modelId]: "testing" }));
    try {
      const res = await modelApi.checkStatus(modelId);
      // res is HealthCheckDetail { status, http_code, latency_ms, message }
      setModelTestingState((prev) => ({ ...prev, [modelId]: res.status as ModelTestState }));
      return res;
    } catch (e) {
      console.error("[usePlatforms] Model test failed:", e);
      setModelTestingState((prev) => ({ ...prev, [modelId]: "error" }));
      throw e;
    }
  }, []);

  const saveCustomModel = useCallback(async () => {
    if (!modelForm.model_name.trim()) {
      throw new Error("请输入模型名称");
    }

    const modelName = modelForm.model_name.trim();
    if (!selectedPlatformId) {
      throw new Error("请先选择一个供应商，再添加自定义模型。");
    }
    const id = `${selectedPlatformId}::${encodeURIComponent(modelName)}`;
    const newModel: PlatformModel = {
      id,
      platform_id: selectedPlatformId,
      model_name: modelName,
      has_vision: modelForm.has_vision,
      has_audio: modelForm.has_audio,
      has_reasoning: modelForm.has_reasoning,
      has_coding: modelForm.has_coding,
      has_long_context: modelForm.has_long_context,
      has_tool_use: modelForm.has_tool_use,
      has_embedding: modelForm.has_embedding,
      has_speedy: modelForm.has_speedy,
      is_enabled: true,
      status: "unknown",
    };

    await modelApi.save(newModel);
    setShowModelModal(false);
    setModelForm(EMPTY_MODEL_FORM);
    if (selectedPlatformId) await selectPlatform(selectedPlatformId);
    await loadActiveModels();
  }, [modelForm, selectedPlatformId, selectPlatform, loadActiveModels]);

  const deleteModel = useCallback(async (id: string) => {
    await modelApi.delete(id);
    if (selectedPlatformId) await selectPlatform(selectedPlatformId);
    await loadActiveModels();
  }, [selectedPlatformId, selectPlatform, loadActiveModels]);

  const openModelModal = useCallback(() => {
    setModelForm(EMPTY_MODEL_FORM);
    setShowModelModal(true);
  }, []);

  const closeModelModal = useCallback(() => {
    setShowModelModal(false);
    setModelForm(EMPTY_MODEL_FORM);
  }, []);

  const updateModelForm = useCallback((field: keyof ModelFormState, value: string | boolean) => {
    setModelForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  return {
    platforms, selectedPlatformId, platformModels, modelTestingState,
    fetchingModels, activeModels, batchTesting,
    showPlatformModal, editingPlatform, platformForm,
    showModelModal, modelForm,
    loadPlatforms, selectPlatform, togglePlatform, savePlatform,
    deletePlatform, fetchRemoteModels, batchTestModels,
    openPlatformModal, closePlatformModal, updatePlatformForm,
    toggleModelEnabled, toggleCapability, testModel,
    saveCustomModel, deleteModel, loadActiveModels,
    openModelModal, closeModelModal, updateModelForm,
  };
}
