import type { OcModel, OcProviderInput } from "@/lib/api";

// Nullable string fields come back as `null` from the API but a fresh draft may
// leave them `undefined`; treat the two as the same "empty" value.
const normalizeNullable = <T>(value: T | null | undefined): T | null => value ?? null;

const modelsEqual = (a: OcModel[], b: OcModel[]): boolean => {
  if (a.length !== b.length) return false;
  return a.every((m, i) => {
    const other = b[i];
    if (other === undefined) return false;
    return (
      m.id === other.id &&
      normalizeNullable(m.name) === normalizeNullable(other.name) &&
      normalizeNullable(m.contextLimit) === normalizeNullable(other.contextLimit) &&
      normalizeNullable(m.outputLimit) === normalizeNullable(other.outputLimit)
    );
  });
};

// Deep compare of every field opencode's provider form can change, order-sensitive
// for models. Used to tell whether unsaved edits sit on top of the last save.
export const isDraftDirty = (draft: OcProviderInput, baseline: OcProviderInput): boolean => {
  if (draft.id !== baseline.id) return true;
  if (draft.name !== baseline.name) return true;
  if (draft.npm !== baseline.npm) return true;
  if (draft.baseUrl !== baseline.baseUrl) return true;
  if (draft.headerStyle !== baseline.headerStyle) return true;
  if (normalizeNullable(draft.customHeaderName) !== normalizeNullable(baseline.customHeaderName)) return true;
  if (draft.enabled !== baseline.enabled) return true;
  if (!modelsEqual(draft.models, baseline.models)) return true;
  return false;
};
