import { Activity, BarChart3, Cpu, History, PiggyBank, SquareTerminal } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export type PageKey = "live" | "token" | "timeline" | "savings" | "terminal" | "provider";

export interface NavItem {
  key: PageKey;
  label: string;
  icon: LucideIcon;
  enabled: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { key: "live", label: "Live", icon: Activity, enabled: true },
  { key: "token", label: "Token & Biaya", icon: BarChart3, enabled: true },
  { key: "timeline", label: "Timeline", icon: History, enabled: true },
  { key: "savings", label: "Hemat Token", icon: PiggyBank, enabled: false },
  { key: "terminal", label: "Terminal", icon: SquareTerminal, enabled: false },
  { key: "provider", label: "Model & Provider", icon: Cpu, enabled: false },
];
