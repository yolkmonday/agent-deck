import { describe, expect, test } from "bun:test";
import type { OcModel } from "@/lib/api";
import {
  applyDefaultLimits,
  applyLimits,
  canSave,
  defaultLimits,
  filterModels,
  incompleteModels,
  mergeFetched,
  removeSelected,
  toggleAll,
  toggleOne,
} from "@/lib/models";

const model = (over: Partial<OcModel> = {}): OcModel => ({
  id: "m",
  name: null,
  contextLimit: 1000,
  outputLimit: 100,
  ...over,
});

describe("defaultLimits", () => {
  test("default_limits_match_known_families", () => {
    expect(defaultLimits("claude-sonnet-5")).toEqual({ context: 200000, output: 64000 });
    expect(defaultLimits("kn/deepseek-v4-1-flash")).toEqual({ context: 65536, output: 8192 });
    expect(defaultLimits("cx/gpt-5.4")).toEqual({ context: 128000, output: 16384 });
    expect(defaultLimits("cbai/kimi-k2.6(high)")).toEqual({ context: 200000, output: 8192 });
  });

  test("default_limits_fall_back_for_unknown", () => {
    expect(defaultLimits("totally-unknown")).toEqual({ context: 32768, output: 4096 });
  });
});

describe("toggleOne", () => {
  test("toggle_one_adds_then_removes", () => {
    const added = toggleOne([], "a");
    expect(added).toEqual(["a"]);
    expect(toggleOne(added, "a")).toEqual([]);
  });
});

describe("toggleAll", () => {
  test("toggle_all_selects_all_visible_then_clears_them", () => {
    const all = toggleAll([], ["a", "b"]);
    expect(all).toEqual(["a", "b"]);
    expect(toggleAll(all, ["a", "b"])).toEqual([]);
  });

  test("toggle_all_preserves_selection_outside_the_filter", () => {
    const on = toggleAll(["a", "z"], ["a", "b"]);
    expect(on).toContain("z");
    expect(on).toContain("a");
    expect(on).toContain("b");

    const off = toggleAll(["a", "z"], ["a"]);
    expect(off).toEqual(["z"]);
  });
});

describe("applyLimits", () => {
  test("apply_limits_only_touches_selected", () => {
    const models = [model({ id: "a" }), model({ id: "b" })];
    const next = applyLimits(models, ["a"], { context: 5, output: 6 });
    expect(next).not.toBe(models);
    expect(next[0]).toEqual(model({ id: "a", contextLimit: 5, outputLimit: 6 }));
    expect(next[1]).toBe(models[1]);
  });
});

describe("applyDefaultLimits", () => {
  test("apply_default_limits_uses_per_model_family", () => {
    const models = [model({ id: "claude-opus-5" }), model({ id: "deepseek-chat" })];
    const next = applyDefaultLimits(models, ["claude-opus-5", "deepseek-chat"]);
    expect(next[0]?.contextLimit).toBe(200000);
    expect(next[0]?.outputLimit).toBe(64000);
    expect(next[1]?.contextLimit).toBe(65536);
    expect(next[1]?.outputLimit).toBe(8192);
  });
});

describe("removeSelected", () => {
  test("remove_selected_keeps_the_rest_in_order", () => {
    const models = [model({ id: "a" }), model({ id: "b" }), model({ id: "c" })];
    expect(removeSelected(models, ["b"]).map((m) => m.id)).toEqual(["a", "c"]);
  });
});

describe("mergeFetched", () => {
  test("merge_fetched_skips_duplicates_and_sets_default_limits", () => {
    const models = [model({ id: "a", contextLimit: 7, outputLimit: 8 })];
    const next = mergeFetched(models, ["a", "b"]);
    expect(next.map((m) => m.id)).toEqual(["a", "b"]);
    expect(next[1]?.contextLimit).not.toBeNull();
    expect(next[1]?.outputLimit).not.toBeNull();
    expect(next[1]?.contextLimit).toBe(defaultLimits("b").context);
  });
});

describe("incompleteModels", () => {
  test("incomplete_models_flags_null_and_zero", () => {
    const flagged = incompleteModels([
      model({ id: "zero", contextLimit: 0 }),
      model({ id: "null", outputLimit: null }),
      model({ id: "fine" }),
    ]);
    expect(flagged.map((m) => m.id)).toEqual(["zero", "null"]);
  });
});

describe("canSave", () => {
  test("can_save_is_false_with_any_incomplete_model", () => {
    expect(canSave([model({ id: "a" }), model({ id: "b", outputLimit: null })])).toBe(false);
    expect(canSave([model({ id: "a" }), model({ id: "b" })])).toBe(true);
  });
});

describe("filterModels", () => {
  test("filter_models_matches_id_and_name_case_insensitively", () => {
    const models = [model({ id: "Alpha", name: "Satu" }), model({ id: "beta", name: "Dua" })];
    expect(filterModels(models, "alph").map((m) => m.id)).toEqual(["Alpha"]);
    expect(filterModels(models, "DUA").map((m) => m.id)).toEqual(["beta"]);
    expect(filterModels(models, "  ")).toEqual(models);
  });
});
