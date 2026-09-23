import { create } from "zustand";
import { findUpdate, restartApp, type FoundUpdate } from "@/lib/updater";

export type UpdateStatus = "idle" | "checking" | "none" | "available" | "downloading" | "error";
export interface UpdaterState {
  status: UpdateStatus;
  version: string | null;
  progress: number;
  error: string | null;
  dismissed: string | null;
  manual: boolean;
  /** True when the current "error" status came from a failed install, not a failed check. */
  installError: boolean;
}
export type UpdaterEvent =
  | { type: "check"; manual: boolean }
  | { type: "found"; version: string }
  | { type: "none" }
  | { type: "failed"; error: string }
  | { type: "progress"; progress: number }
  | { type: "dismiss" }
  | { type: "install-failed"; error: string };

export const initialUpdaterState: UpdaterState = {
  status: "idle",
  version: null,
  progress: 0,
  error: null,
  dismissed: null,
  manual: false,
  installError: false,
};

export const reduce = (s: UpdaterState, e: UpdaterEvent): UpdaterState => {
  switch (e.type) {
    case "check":
      if (s.status === "downloading") return s;
      return { ...s, status: "checking", error: null, manual: e.manual, installError: false };
    case "found":
      return { ...s, status: "available", version: e.version, progress: 0, installError: false };
    case "none":
      return { ...s, status: "none" };
    case "failed":
      if (s.manual) return { ...s, status: "error", error: e.error };
      // A periodic re-check failing must not hide a banner for an update we already found.
      if (s.version) return { ...s, status: "available", error: null };
      return { ...s, status: "idle", error: null };
    case "progress":
      return { ...s, status: "downloading", progress: Math.min(1, Math.max(0, e.progress)) };
    case "dismiss":
      return { ...s, dismissed: s.version, installError: false };
    case "install-failed":
      return { ...s, status: "error", manual: true, error: e.error, installError: true };
  }
};

export const bannerVisible = (s: UpdaterState): boolean =>
  s.status === "downloading" ||
  (s.status === "available" && s.version !== s.dismissed) ||
  (s.status === "error" && s.installError);

const AUTO_DELAY_MS = 10_000;
const AUTO_INTERVAL_MS = 6 * 60 * 60 * 1000;

let pending: FoundUpdate | null = null;

export const useUpdater = create<
  UpdaterState & { check: (manual: boolean) => Promise<void>; install: () => Promise<void>; dismiss: () => void }
>((set, get) => {
  const dispatch = (e: UpdaterEvent) => set(reduce(get(), e));
  return {
    ...initialUpdaterState,
    check: async (manual) => {
      if (get().status === "checking" || get().status === "downloading") return;
      dispatch({ type: "check", manual });
      try {
        const found = await findUpdate();
        if (pending) void pending.dispose().catch(() => undefined);
        pending = found;
        dispatch(pending ? { type: "found", version: pending.version } : { type: "none" });
      } catch (err) {
        const error = err instanceof Error ? err.message : String(err);
        if (!manual) console.warn("[updater] check failed", error);
        dispatch({ type: "failed", error });
      }
    },
    install: async () => {
      if (!pending) return;
      dispatch({ type: "progress", progress: 0 });
      try {
        await pending.install((p) => dispatch({ type: "progress", progress: p }));
        await restartApp();
      } catch (err) {
        dispatch({ type: "install-failed", error: err instanceof Error ? err.message : String(err) });
      }
    },
    dismiss: () => dispatch({ type: "dismiss" }),
  };
});

export const startAutoCheck = (): (() => void) => {
  if (import.meta.env.DEV) return () => undefined;
  const run = () => void useUpdater.getState().check(false);
  const first = setTimeout(run, AUTO_DELAY_MS);
  const every = setInterval(run, AUTO_INTERVAL_MS);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
};
