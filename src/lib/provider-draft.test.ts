import { describe, expect, test } from "bun:test";
import type { OcModel, OcProviderInput } from "@/lib/api";
import { isDraftDirty } from "@/lib/provider-draft";

const model = (over: Partial<OcModel> = {}): OcModel => ({
  id: "m",
  name: null,
  contextLimit: 1000,
  outputLimit: 100,
  ...over,
});

const provider = (over: Partial<OcProviderInput> = {}): OcProviderInput => ({
  id: "aki",
  name: "aki gateway",
  npm: "@ai-sdk/openai-compatible",
  baseUrl: "https://gateway.example.com/v1",
  headerStyle: "bearer",
  customHeaderName: null,
  enabled: true,
  models: [model()],
  ...over,
});

describe("isDraftDirty", () => {
  test("identical_drafts_are_not_dirty", () => {
    expect(isDraftDirty(provider(), provider())).toBe(false);
  });

  test("different_scalar_field_is_dirty", () => {
    expect(isDraftDirty(provider({ name: "changed" }), provider())).toBe(true);
    expect(isDraftDirty(provider({ baseUrl: "https://other.example.com" }), provider())).toBe(true);
    expect(isDraftDirty(provider({ enabled: false }), provider())).toBe(true);
    expect(isDraftDirty(provider({ headerStyle: "custom" }), provider())).toBe(true);
  });

  test("added_model_is_dirty", () => {
    const baseline = provider();
    const draft = provider({ models: [...baseline.models, model({ id: "n" })] });
    expect(isDraftDirty(draft, baseline)).toBe(true);
  });

  test("removed_model_is_dirty", () => {
    const baseline = provider({ models: [model({ id: "a" }), model({ id: "b" })] });
    const draft = provider({ models: [model({ id: "a" })] });
    expect(isDraftDirty(draft, baseline)).toBe(true);
  });

  test("changed_model_field_is_dirty", () => {
    const baseline = provider({ models: [model({ id: "a", contextLimit: 1000 })] });
    const draft = provider({ models: [model({ id: "a", contextLimit: 2000 })] });
    expect(isDraftDirty(draft, baseline)).toBe(true);
  });

  test("model_order_change_is_dirty", () => {
    const baseline = provider({ models: [model({ id: "a" }), model({ id: "b" })] });
    const draft = provider({ models: [model({ id: "b" }), model({ id: "a" })] });
    expect(isDraftDirty(draft, baseline)).toBe(true);
  });

  test("customHeaderName_null_vs_undefined_is_not_dirty", () => {
    const baseline = provider({ customHeaderName: null });
    const draft = { ...provider(), customHeaderName: undefined } as unknown as OcProviderInput;
    expect(isDraftDirty(draft, baseline)).toBe(false);
  });

  test("model_name_null_vs_undefined_is_not_dirty", () => {
    const baseline = provider({ models: [model({ name: null })] });
    const draft = provider({ models: [{ ...model(), name: undefined } as unknown as OcModel] });
    expect(isDraftDirty(draft, baseline)).toBe(false);
  });
});
