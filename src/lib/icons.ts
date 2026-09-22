import lucideIcons from "@iconify-json/lucide/icons.json";
import simpleIcons from "@iconify-json/simple-icons/icons.json";
import { addCollection } from "@iconify/react";
import type { Agent } from "@/lib/types";

const PALETTE = ["#4c9aff", "#f5a524", "#3dd68c", "#e8825c", "#34d0e0", "#a78bfa"] as const;

const AGENT_ICONS: Record<Agent, string> = {
  claude: "simple-icons:claude",
  opencode: "simple-icons:opencode",
  codex: "simple-icons:openai",
};

const MODEL_MATCHERS: [string, string][] = [
  ["claude", "simple-icons:claude"],
  ["gpt", "simple-icons:openai"],
  ["codex", "simple-icons:openai"],
  ["openai", "simple-icons:openai"],
  ["deepseek", "simple-icons:deepseek"],
  ["kimi", "simple-icons:kimi"],
  ["moonshot", "simple-icons:moonshotai"],
  ["qwen", "simple-icons:qwen"],
  ["minimax", "simple-icons:minimax"],
  ["mistral", "simple-icons:mistralai"],
  ["gemini", "simple-icons:googlegemini"],
  ["ollama", "simple-icons:ollama"],
];

const PROVIDER_ICONS: Record<string, string> = {
  anthropic: "simple-icons:anthropic",
  openai: "simple-icons:openai",
  deepseek: "simple-icons:deepseek",
};

let registered = false;

export const registerIcons = (): void => {
  if (registered) return;
  registered = true;
  addCollection(simpleIcons);
  addCollection(lucideIcons);
};

export const agentIcon = (agent: Agent): string => AGENT_ICONS[agent];

export const modelIcon = (model: string | null): string | null => {
  if (model === null) return null;
  const lower = model.toLowerCase();
  for (const [needle, icon] of MODEL_MATCHERS) {
    if (lower.includes(needle)) return icon;
  }
  return null;
};

export const providerIcon = (providerId: string): string | null => PROVIDER_ICONS[providerId] ?? null;

const hash = (key: string): number => {
  let h = 0;
  for (let i = 0; i < key.length; i += 1) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return h;
};

export const fallbackTile = (key: string): { color: string; initials: string } => {
  const letters = key.replace(/[^A-Za-z0-9]/g, "").slice(0, 2).toUpperCase();
  return {
    color: PALETTE[hash(key) % PALETTE.length],
    initials: letters === "" ? "?" : letters,
  };
};
