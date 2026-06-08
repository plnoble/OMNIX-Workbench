/**
 * useMcpServers — MCP server configuration CRUD
 *
 * Manages: MCP server list, add/edit/delete, form state, modal visibility
 */

import { useState, useCallback } from "react";
import { mcpApi } from "@/lib/tauri-api";
import type { McpServer } from "@/types";

interface McpServerForm {
  id: string;
  name: string;
  command: string;
  args: string;
  env: string;
  url: string;
  server_type: "stdio" | "sse";
  is_enabled: boolean;
}

const EMPTY_MCP_FORM: McpServerForm = {
  id: "",
  name: "",
  command: "",
  args: "[]",
  env: "{}",
  url: "",
  server_type: "stdio",
  is_enabled: true,
};

export interface UseMcpServersReturn {
  mcpServers: McpServer[];
  showMcpModal: boolean;
  editingMcpServer: McpServer | null;
  mcpForm: McpServerForm;

  loadMcpServers: () => Promise<void>;
  openMcpModal: (server?: McpServer) => void;
  closeMcpModal: () => void;
  updateMcpForm: (field: string, value: string | boolean) => void;
  saveMcpServer: () => Promise<void>;
  deleteMcpServer: (id: string) => Promise<void>;
}

export function useMcpServers(): UseMcpServersReturn {
  const [mcpServers, setMcpServers] = useState<McpServer[]>([]);
  const [showMcpModal, setShowMcpModal] = useState(false);
  const [editingMcpServer, setEditingMcpServer] = useState<McpServer | null>(null);
  const [mcpForm, setMcpForm] = useState<McpServerForm>({ ...EMPTY_MCP_FORM });

  const loadMcpServers = useCallback(async () => {
    try {
      const list = await mcpApi.list();
      setMcpServers(list);
    } catch (e) {
      console.error("[useMcpServers] Failed to load servers:", e);
    }
  }, []);

  const openMcpModal = useCallback((server?: McpServer) => {
    if (server) {
      setEditingMcpServer(server);
      setMcpForm({
        id: server.id,
        name: server.name,
        command: server.command,
        args: server.args,
        env: server.env,
        url: server.url,
        server_type: server.server_type as "stdio" | "sse",
        is_enabled: server.is_enabled,
      });
    } else {
      setEditingMcpServer(null);
      setMcpForm({
        ...EMPTY_MCP_FORM,
        id: `mcp_${Date.now()}`,
      });
    }
    setShowMcpModal(true);
  }, []);

  const closeMcpModal = useCallback(() => {
    setShowMcpModal(false);
    setEditingMcpServer(null);
    setMcpForm({ ...EMPTY_MCP_FORM });
  }, []);

  const updateMcpForm = useCallback((field: string, value: string | boolean) => {
    setMcpForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  const saveMcpServer = useCallback(async () => {
    try {
      await mcpApi.save({
        id: mcpForm.id,
        name: mcpForm.name,
        command: mcpForm.command,
        args: mcpForm.args,
        env: mcpForm.env,
        url: mcpForm.url,
        server_type: mcpForm.server_type,
        is_enabled: mcpForm.is_enabled,
      });
      await loadMcpServers();
      closeMcpModal();
    } catch (e) {
      console.error("[useMcpServers] Failed to save server:", e);
      throw e;
    }
  }, [mcpForm, loadMcpServers, closeMcpModal]);

  const deleteMcpServer = useCallback(async (id: string) => {
    try {
      await mcpApi.delete(id);
      await loadMcpServers();
    } catch (e) {
      console.error("[useMcpServers] Failed to delete server:", e);
    }
  }, [loadMcpServers]);

  return {
    mcpServers, showMcpModal, editingMcpServer, mcpForm,
    loadMcpServers, openMcpModal, closeMcpModal,
    updateMcpForm, saveMcpServer, deleteMcpServer,
  };
}
