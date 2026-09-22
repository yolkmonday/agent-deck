import { Activity, BarChart3, Cpu, Folder, History, PiggyBank, Rss, SquareTerminal } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { createContext, useContext } from "react";

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
  label: string;
  icon: LucideIcon;
  enabled: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { key: "live", label: "Live", icon: Activity, enabled: true },
  { key: "activity", label: "Aktivitas", icon: Rss, enabled: true },
  { key: "token", label: "Token & Biaya", icon: BarChart3, enabled: true },
  { key: "timeline", label: "Timeline", icon: History, enabled: true },
  { key: "savings", label: "Hemat Token", icon: PiggyBank, enabled: true },
  { key: "terminal", label: "Terminal", icon: SquareTerminal, enabled: true },
  { key: "provider", label: "Model & Provider", icon: Cpu, enabled: true },
  { key: "project", label: "Project", icon: Folder, enabled: true },
];
