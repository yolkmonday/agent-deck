import type { OcModel } from "@/lib/api";

export interface LimitPair {
  context: number;
  output: number;
}

const LIMIT_RULES: { match: string[]; limits: LimitPair }[] = [
  { match: ["claude"], limits: { context: 200000, output: 64000 } },
  { match: ["gpt", "codex", "openai"], limits: { context: 128000, output: 16384 } },
  { match: ["deepseek"], limits: { context: 65536, output: 8192 } },
  { match: ["kimi", "moonshot"], limits: { context: 200000, output: 8192 } },
  { match: ["qwen"], limits: { context: 131072, output: 8192 } },
  { match: ["glm", "zhipu"], limits: { context: 128000, output: 8192 } },
  { match: ["minimax"], limits: { context: 192000, output: 8192 } },
  { match: ["gemini"], limits: { context: 1000000, output: 8192 } },
];

const FALLBACK_LIMITS: LimitPair = { context: 32768, output: 4096 };

export const defaultLimits = (modelId: string): LimitPair => {
  const needle = modelId.toLowerCase();
  for (const rule of LIMIT_RULES) {
    if (rule.match.some((token) => needle.includes(token))) return { ...rule.limits };
  }
  return { ...FALLBACK_LIMITS };
};

export const toggleOne = (selected: string[], id: string): string[] =>
  selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];

export const toggleAll = (selected: string[], visibleIds: string[]): string[] => {
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selected.includes(id));
  if (allVisibleSelected) {
    const visible = new Set(visibleIds);
    return selected.filter((id) => !visible.has(id));
  }
  const chosen = new Set(selected);
  const added = visibleIds.filter((id) => !chosen.has(id));
  return [...selected, ...added];
};

export const applyLimits = (models: OcModel[], ids: string[], limits: LimitPair): OcModel[] => {
  const targets = new Set(ids);
  return models.map((m) =>
    targets.has(m.id) ? { ...m, contextLimit: limits.context, outputLimit: limits.output } : m,
  );
};

export const applyDefaultLimits = (models: OcModel[], ids: string[]): OcModel[] => {
  const targets = new Set(ids);
  return models.map((m) => (targets.has(m.id) ? { ...m, ...limitFields(defaultLimits(m.id)) } : m));
};

const limitFields = (limits: LimitPair) => ({ contextLimit: limits.context, outputLimit: limits.output });

export const removeSelected = (models: OcModel[], ids: string[]): OcModel[] => {
  const targets = new Set(ids);
  return models.filter((m) => !targets.has(m.id));
};

export const mergeFetched = (models: OcModel[], chosenIds: string[]): OcModel[] => {
  const known = new Set(models.map((m) => m.id));
  const added = chosenIds
    .filter((id) => !known.has(id))
    .map((id) => ({ id, name: null, ...limitFields(defaultLimits(id)) }));
  return [...models, ...added];
};

export const incompleteModels = (models: OcModel[]): OcModel[] =>
  models.filter(
    (m) => m.contextLimit === null || m.outputLimit === null || m.contextLimit <= 0 || m.outputLimit <= 0,
  );

export const canSave = (models: OcModel[]): boolean => incompleteModels(models).length === 0;

export const filterModels = (models: OcModel[], query: string): OcModel[] => {
  const needle = query.trim().toLowerCase();
  if (needle === "") return models;
  return models.filter(
    (m) => m.id.toLowerCase().includes(needle) || (m.name ?? "").toLowerCase().includes(needle),
  );
};
