import { Activity, BarChart3, Cpu, Folder, History, PiggyBank, Rss, SquareTerminal } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { createContext, useContext } from "react";
import type { MessageKey } from "@/i18n";

export type PageKey = "live" | "activity" | "token" | "timeline" | "savings" | "terminal" | "provider" | "project";

export interface NavActions {
  provider: (id: string) => void;
  project: () => void;
}

const NavContext = createContext<NavActions | null>(null);

export const NavProvider = NavContext.Provider;

export const useNavigate = (): NavActions => {
  const ctx = useContext(NavContext);
  if (ctx === null) throw new Error("useNavigate must be used inside NavProvider");
  return ctx;
};

export interface NavItem {
  key: PageKey;
  labelKey: MessageKey;
  icon: LucideIcon;
  enabled: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { key: "live", labelKey: "nav.live", icon: Activity, enabled: true },
  { key: "activity", labelKey: "nav.activity", icon: Rss, enabled: true },
  { key: "token", labelKey: "nav.token", icon: BarChart3, enabled: true },
  { key: "timeline", labelKey: "nav.timeline", icon: History, enabled: true },
  { key: "savings", labelKey: "nav.savings", icon: PiggyBank, enabled: true },
  { key: "terminal", labelKey: "nav.terminal", icon: SquareTerminal, enabled: true },
  { key: "provider", labelKey: "nav.provider", icon: Cpu, enabled: true },
  { key: "project", labelKey: "nav.project", icon: Folder, enabled: true },
];
