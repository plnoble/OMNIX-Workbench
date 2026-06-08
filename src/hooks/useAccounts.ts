/**
 * useAccounts — Agent account credential management
 *
 * Manages: accounts list, account form state, modal visibility
 */

import { useState, useCallback } from "react";
import { accountApi, settingsApi } from "@/lib/tauri-api";
import type { AgentAccount, PlatformModel } from "@/types";

export interface UseAccountsReturn {
  accounts: AgentAccount[];
  isAccountModalOpen: boolean;
  accFormId: string;
  accFormName: string;
  accFormKey: string;
  accFormHost: string;
  accFormModel: string;

  loadAccounts: () => Promise<void>;
  saveAccount: () => Promise<void>;
  switchAccount: (id: string) => Promise<void>;
  deleteAccount: (id: string) => Promise<void>;
  openAccountModal: (acc?: AgentAccount | null) => void;
  closeAccountModal: () => void;
  updateAccForm: (field: string, value: string) => void;
}

export function useAccounts(activeModels: PlatformModel[]): UseAccountsReturn {
  const [accounts, setAccounts] = useState<AgentAccount[]>([]);
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);

  // Form state
  const [accFormId, setAccFormId] = useState("");
  const [accFormName, setAccFormName] = useState("");
  const [accFormKey, setAccFormKey] = useState("");
  const [accFormHost, setAccFormHost] = useState("");
  const [accFormModel, setAccFormModel] = useState("deepseek-chat");

  const loadAccounts = useCallback(async () => {
    try {
      const list = await accountApi.list();
      setAccounts(list);
    } catch (e) {
      console.error("[useAccounts] Failed to load accounts:", e);
    }
  }, []);

  const saveAccount = useCallback(async () => {
    if (!accFormName.trim()) {
      throw new Error("请输入账户显示名称");
    }

    const id = accFormId || `acc_${Date.now()}`;
    const existingAcc = accFormId ? accounts.find((a) => a.id === accFormId) : null;

    const newAcc: AgentAccount = {
      id,
      account_name: accFormName,
      api_key: accFormKey,
      api_host: accFormHost,
      target_model: accFormModel,
      is_active: existingAcc?.is_active ?? false,
      updated_at: new Date().toISOString(),
    };

    await accountApi.save(newAcc);
    setIsAccountModalOpen(false);
    resetForm();
    await loadAccounts();
  }, [accFormId, accFormName, accFormKey, accFormHost, accFormModel, accounts, loadAccounts]);

  const switchAccount = useCallback(async (id: string) => {
    await accountApi.switch(id);
    await loadAccounts();
    await settingsApi.syncExternalConfigs();
  }, [loadAccounts]);

  const deleteAccount = useCallback(async (id: string) => {
    await accountApi.delete(id);
    await loadAccounts();
  }, [loadAccounts]);

  const openAccountModal = useCallback((acc?: AgentAccount | null) => {
    if (acc) {
      setAccFormId(acc.id);
      setAccFormName(acc.account_name);
      setAccFormKey(acc.api_key);
      setAccFormHost(acc.api_host);
      setAccFormModel(acc.target_model);
    } else {
      resetForm();
      // Auto-select first active model if available
      if (activeModels.length > 0) {
        setAccFormModel(activeModels[0].model_name);
      }
    }
    setIsAccountModalOpen(true);
  }, [activeModels]);

  const closeAccountModal = useCallback(() => {
    setIsAccountModalOpen(false);
    resetForm();
  }, []);

  const updateAccForm = useCallback((field: string, value: string) => {
    switch (field) {
      case "name": setAccFormName(value); break;
      case "key": setAccFormKey(value); break;
      case "host": setAccFormHost(value); break;
      case "model": setAccFormModel(value); break;
    }
  }, []);

  function resetForm() {
    setAccFormId("");
    setAccFormName("");
    setAccFormKey("");
    setAccFormHost("");
    setAccFormModel("deepseek-chat");
  }

  return {
    accounts, isAccountModalOpen,
    accFormId, accFormName, accFormKey, accFormHost, accFormModel,
    loadAccounts, saveAccount, switchAccount, deleteAccount,
    openAccountModal, closeAccountModal, updateAccForm,
  };
}
