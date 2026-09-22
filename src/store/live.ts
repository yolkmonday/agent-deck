import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { diffEvents } from "@/lib/events";
import type { FeedEvent, LiveSnapshot } from "@/lib/types";

interface LiveState {
  snapshot: LiveSnapshot | null;
  events: FeedEvent[];
  apply: (s: LiveSnapshot) => void;
}

export const useLive = create<LiveState>((set, get) => ({
  snapshot: null,
  events: [],
  apply: (s) => {
    const prev = get().snapshot?.sessions ?? null;
    const fresh = diffEvents(prev, s.sessions, s.generatedAtMs);
    set({ snapshot: s, events: [...fresh, ...get().events].slice(0, 50) });
  },
}));

export const startLive = async (): Promise<() => void> => {
  const { apply } = useLive.getState();
  apply(await invoke<LiveSnapshot>("live_snapshot"));
  return listen<LiveSnapshot>("live://snapshot", (e) => apply(e.payload));
};
