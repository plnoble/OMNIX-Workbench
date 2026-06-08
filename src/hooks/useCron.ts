/**
 * useCron — Scheduled task management
 */

import { useState, useCallback } from "react";
import { cronApi } from "@/lib/tauri-api";
import type { CronTask, CronRun, DetectedAgent } from "@/types";
import { DEFAULT_CRON_SCHEDULE, AGENT_NAMES } from "@/lib/constants";

interface CronFormState {
  title: string;
  schedule: string;
  agent_name: string;
  args: string;
  workspace_dir: string;
  is_active: boolean;
}

const EMPTY_CRON_FORM: CronFormState = {
  title: "",
  schedule: DEFAULT_CRON_SCHEDULE,
  agent_name: AGENT_NAMES[0],
  args: "",
  workspace_dir: "direct",
  is_active: true,
};

export interface UseCronReturn {
  cronTasks: CronTask[];
  cronRuns: CronRun[];
  showCronModal: boolean;
  editingCron: CronTask | null;
  cronForm: CronFormState;

  loadCronTasks: () => Promise<void>;
  loadCronRuns: () => Promise<void>;
  saveCronTask: () => Promise<void>;
  deleteCronTask: (id: string) => Promise<void>;
  toggleCronTask: (task: CronTask) => Promise<void>;
  triggerCronTask: (id: string) => Promise<void>;
  clearCronRuns: () => Promise<void>;
  openCronModal: (task?: CronTask | null) => void;
  closeCronModal: () => void;
  updateCronForm: (field: keyof CronFormState, value: string | boolean) => void;
}

export function useCron(detectedAgents: DetectedAgent[]): UseCronReturn {
  const [cronTasks, setCronTasks] = useState<CronTask[]>([]);
  const [cronRuns, setCronRuns] = useState<CronRun[]>([]);
  const [showCronModal, setShowCronModal] = useState(false);
  const [editingCron, setEditingCron] = useState<CronTask | null>(null);
  const [cronForm, setCronForm] = useState<CronFormState>(EMPTY_CRON_FORM);

  const loadCronTasks = useCallback(async () => {
    try {
      const list = await cronApi.listTasks();
      setCronTasks(list);
    } catch (e) {
      console.error("[useCron] Failed to load tasks:", e);
    }
  }, []);

  const loadCronRuns = useCallback(async () => {
    try {
      const list = await cronApi.listRuns();
      setCronRuns(list);
    } catch (e) {
      console.error("[useCron] Failed to load runs:", e);
    }
  }, []);

  const saveCronTask = useCallback(async () => {
    if (!cronForm.title.trim() || !cronForm.schedule.trim()) {
      throw new Error("请输入名称和 Cron 表达式");
    }

    const id = editingCron ? editingCron.id : `cron_${Date.now()}`;
    const newTask: CronTask = {
      id,
      title: cronForm.title,
      schedule: cronForm.schedule,
      agent_name: cronForm.agent_name,
      args: cronForm.args,
      workspace_dir: cronForm.workspace_dir,
      is_active: cronForm.is_active,
      last_run: editingCron ? editingCron.last_run : null,
      created_at: editingCron ? editingCron.created_at : new Date().toISOString(),
    };

    await cronApi.saveTask(newTask);
    setShowCronModal(false);
    setEditingCron(null);
    setCronForm(EMPTY_CRON_FORM);
    await loadCronTasks();
  }, [cronForm, editingCron, loadCronTasks]);

  const deleteCronTask = useCallback(async (id: string) => {
    await cronApi.deleteTask(id);
    await loadCronTasks();
  }, [loadCronTasks]);

  const toggleCronTask = useCallback(async (task: CronTask) => {
    await cronApi.toggleActive({ id: task.id, isActive: !task.is_active });
    await loadCronTasks();
  }, [loadCronTasks]);

  const triggerCronTask = useCallback(async (id: string) => {
    await cronApi.trigger(id);
    await loadCronRuns();
  }, [loadCronRuns]);

  const clearCronRuns = useCallback(async () => {
    await cronApi.clearRuns();
    await loadCronRuns();
  }, [loadCronRuns]);

  const openCronModal = useCallback((task?: CronTask | null) => {
    if (task) {
      setEditingCron(task);
      setCronForm({
        title: task.title,
        schedule: task.schedule,
        agent_name: task.agent_name,
        args: task.args,
        workspace_dir: task.workspace_dir,
        is_active: task.is_active,
      });
    } else {
      setEditingCron(null);
      // Default to first installed agent, or fallback to Claude Code
      const installedAgent = detectedAgents.find((a) => a.status === "installed");
      setCronForm({
        ...EMPTY_CRON_FORM,
        agent_name: installedAgent?.name ?? AGENT_NAMES[0],
      });
    }
    setShowCronModal(true);
  }, [detectedAgents]);

  const closeCronModal = useCallback(() => {
    setShowCronModal(false);
    setEditingCron(null);
    setCronForm(EMPTY_CRON_FORM);
  }, []);

  const updateCronForm = useCallback((field: keyof CronFormState, value: string | boolean) => {
    setCronForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  return {
    cronTasks, cronRuns, showCronModal, editingCron, cronForm,
    loadCronTasks, loadCronRuns, saveCronTask, deleteCronTask,
    toggleCronTask, triggerCronTask, clearCronRuns,
    openCronModal, closeCronModal, updateCronForm,
  };
}
