// Recent files store, backed by SQLite.

import { create } from "zustand";
import { addRecentFile, getRecentFiles } from "@/lib/db";
import type { RecentFile } from "@/types";

interface RecentFilesState {
  files: RecentFile[];
  loaded: boolean;
  load: () => Promise<void>;
  record: (path: string, name: string, kind: string) => Promise<void>;
}

export const useRecentFilesStore = create<RecentFilesState>((set, get) => ({
  files: [],
  loaded: false,

  load: async () => {
    const files = await getRecentFiles();
    set({ files, loaded: true });
  },

  record: async (path, name, kind) => {
    await addRecentFile(path, name, kind);
    await get().load();
  },
}));
