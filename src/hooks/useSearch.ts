/**
 * useSearch — Web search providers and search execution
 *
 * Manages: search provider CRUD, search execution, search history,
 *          add/edit modal state
 */

import { useState, useCallback } from "react";
import { searchApi } from "@/lib/tauri-api";
import type { SearchProvider, WebSearchResult, SearchHistoryEntry } from "@/types";

export interface UseSearchReturn {
  providers: SearchProvider[];
  results: WebSearchResult[];
  history: SearchHistoryEntry[];
  searchQuery: string;
  selectedProviderId: string;
  isSearching: boolean;

  // Modal state
  showSearchProviderModal: boolean;
  editingSearchProvider: SearchProvider | null;
  searchProviderForm: { id: string; name: string; api_type: string; api_key: string; api_address: string; is_enabled: boolean };
  openSearchProviderModal: (provider?: SearchProvider) => void;
  closeSearchProviderModal: () => void;
  updateSearchProviderForm: (field: string, value: string | boolean) => void;

  loadProviders: () => Promise<void>;
  saveProvider: (provider: SearchProvider) => Promise<void>;
  deleteProvider: (id: string) => Promise<void>;
  search: (query: string) => Promise<WebSearchResult[]>;
  loadHistory: () => Promise<void>;
  deleteHistoryItem: (id: string) => Promise<void>;
  clearHistory: () => Promise<void>;
  setSearchQuery: (q: string) => void;
  setSelectedProviderId: (id: string) => void;
}

const EMPTY_PROVIDER_FORM = {
  id: "",
  name: "",
  api_type: "searxng",
  api_key: "",
  api_address: "",
  is_enabled: true,
};

export function useSearch(): UseSearchReturn {
  const [providers, setProviders] = useState<SearchProvider[]>([]);
  const [results, setResults] = useState<WebSearchResult[]>([]);
  const [history, setHistory] = useState<SearchHistoryEntry[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedProviderId, setSelectedProviderId] = useState("");
  const [isSearching, setIsSearching] = useState(false);

  // Modal state
  const [showSearchProviderModal, setShowSearchProviderModal] = useState(false);
  const [editingSearchProvider, setEditingSearchProvider] = useState<SearchProvider | null>(null);
  const [searchProviderForm, setSearchProviderForm] = useState(EMPTY_PROVIDER_FORM);

  const openSearchProviderModal = useCallback((provider?: SearchProvider) => {
    if (provider) {
      setEditingSearchProvider(provider);
      setSearchProviderForm({
        id: provider.id,
        name: provider.name,
        api_type: provider.api_type,
        api_key: provider.api_key,
        api_address: provider.api_address,
        is_enabled: provider.is_enabled,
      });
    } else {
      setEditingSearchProvider(null);
      setSearchProviderForm({ ...EMPTY_PROVIDER_FORM, id: `sp_${Date.now()}` });
    }
    setShowSearchProviderModal(true);
  }, []);

  const closeSearchProviderModal = useCallback(() => {
    setShowSearchProviderModal(false);
    setEditingSearchProvider(null);
    setSearchProviderForm(EMPTY_PROVIDER_FORM);
  }, []);

  const updateSearchProviderForm = useCallback((field: string, value: string | boolean) => {
    setSearchProviderForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  const loadProviders = useCallback(async () => {
    try {
      const list = await searchApi.listProviders();
      setProviders(list);
    } catch (e) {
      console.error("[useSearch] Failed to load providers:", e);
    }
  }, []);

  const saveProvider = useCallback(async (provider: SearchProvider) => {
    try {
      await searchApi.saveProvider(provider);
      await loadProviders();
    } catch (e) {
      console.error("[useSearch] Failed to save provider:", e);
      throw e;
    }
  }, [loadProviders]);

  const deleteProvider = useCallback(async (id: string) => {
    try {
      await searchApi.deleteProvider(id);
      await loadProviders();
    } catch (e) {
      console.error("[useSearch] Failed to delete provider:", e);
    }
  }, [loadProviders]);

  const search = useCallback(async (query: string): Promise<WebSearchResult[]> => {
    setIsSearching(true);
    try {
      const providerId = selectedProviderId || undefined;
      const res = await searchApi.search(query, providerId, 10);
      setResults(res);
      return res;
    } catch (e) {
      console.error("[useSearch] Search failed:", e);
      setResults([]);
      return [];
    } finally {
      setIsSearching(false);
    }
  }, [selectedProviderId]);

  const loadHistory = useCallback(async () => {
    try {
      const list = await searchApi.getHistory(50);
      setHistory(list);
    } catch (e) {
      console.error("[useSearch] Failed to load history:", e);
    }
  }, []);

  const deleteHistoryItem = useCallback(async (id: string) => {
    try {
      await searchApi.deleteHistoryItem(id);
      await loadHistory();
    } catch (e) {
      console.error("[useSearch] Failed to delete history item:", e);
    }
  }, [loadHistory]);

  const clearHistory = useCallback(async () => {
    try {
      await searchApi.clearHistory();
      setHistory([]);
    } catch (e) {
      console.error("[useSearch] Failed to clear history:", e);
    }
  }, []);

  return {
    providers, results, history, searchQuery, selectedProviderId, isSearching,
    showSearchProviderModal, editingSearchProvider, searchProviderForm,
    openSearchProviderModal, closeSearchProviderModal, updateSearchProviderForm,
    loadProviders, saveProvider, deleteProvider, search,
    loadHistory, deleteHistoryItem, clearHistory,
    setSearchQuery, setSelectedProviderId,
  };
}
