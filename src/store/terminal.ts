import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import type { TermExitEvent, TermSession } from "@/lib/api";
import { termKill, termList, termStart } from "@/lib/api";
import { dispose as disposeInstance, measureInitialSize } from "@/lib/terminal-instances";

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
      const { cols, rows } = measureInitialSize();
      const session = await termStart(profileId, cwd, cols, rows);
      set({ sessions: [...get().sessions, session], activeId: session.id, error: null });
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },
  select: (id) => set({ activeId: id }),
  remove: (id) => {
    disposeInstance(id);
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
  const unExit = await listen<TermExitEvent>("term://exit", (e) => {
    useTerminal.setState({
      sessions: useTerminal.getState().sessions.map((s) =>
        s.id === e.payload.id ? { ...s, alive: false } : s,
      ),
    });
  });
  return () => {
    unExit();
  };
};
