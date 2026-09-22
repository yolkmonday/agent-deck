import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { TermDataEvent, TermExitEvent, TermSession } from "@/lib/api";
import { termKill, termList, termStart } from "@/lib/api";

const writers = new Map<string, (chunk: string) => void>();

export const registerWriter = (id: string, write: (chunk: string) => void): (() => void) => {
  writers.set(id, write);
  return () => {
    if (writers.get(id) === write) writers.delete(id);
  };
};

interface TerminalState {
  sessions: TermSession[];
  activeId: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  start: (profileId: string, cwd: string) => Promise<void>;
  select: (id: string) => void;
  remove: (id: string) => void;
}

export const useTerminal = create<TerminalState>((set, get) => ({
  sessions: [],
  activeId: null,
  error: null,
  refresh: async () => {
    try {
      const sessions = await termList();
      const activeId = get().activeId;
      set({
        sessions,
        error: null,
        activeId: activeId && sessions.some((s) => s.id === activeId) ? activeId : (sessions[0]?.id ?? null),
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  start: async (profileId, cwd) => {
    try {
      const session = await termStart(profileId, cwd);
      set({ sessions: [...get().sessions, session], activeId: session.id, error: null });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },
  select: (id) => set({ activeId: id }),
  remove: (id) => {
    const sessions = get().sessions.filter((s) => s.id !== id);
    set({
      sessions,
      activeId: get().activeId === id ? (sessions[0]?.id ?? null) : get().activeId,
    });
  },
}));

export const stopSession = async (id: string): Promise<void> => {
  await termKill(id);
  useTerminal.getState().remove(id);
};

export const startTerminalEvents = async (): Promise<() => void> => {
  await useTerminal.getState().refresh();
  const unData = await listen<TermDataEvent>("term://data", (e) => {
    writers.get(e.payload.id)?.(e.payload.chunk);
  });
  const unExit = await listen<TermExitEvent>("term://exit", (e) => {
    useTerminal.setState({
      sessions: useTerminal.getState().sessions.map((s) =>
        s.id === e.payload.id ? { ...s, alive: false } : s,
      ),
    });
  });
  return () => {
    unData();
    unExit();
  };
};
