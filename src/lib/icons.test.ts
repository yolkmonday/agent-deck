import { describe, expect, test } from "bun:test";
import { agentIcon, fallbackTile, modelIcon, providerIcon } from "@/lib/icons";

const VERIFIED = [
  "simple-icons:claude",
  "simple-icons:anthropic",
  "simple-icons:openai",
  "simple-icons:deepseek",
  "simple-icons:moonshotai",
  "simple-icons:kimi",
  "simple-icons:qwen",
  "simple-icons:minimax",
  "simple-icons:mistralai",
  "simple-icons:googlegemini",
  "simple-icons:ollama",
  "simple-icons:opencode",
];

const PALETTE = ["#4c9aff", "#f5a524", "#3dd68c", "#e8825c", "#34d0e0", "#a78bfa"];

describe("agentIcon", () => {
  test("agent_icons_map_to_verified_names", () => {
    expect(agentIcon("claude")).toBe("simple-icons:claude");
    expect(agentIcon("opencode")).toBe("simple-icons:opencode");
    expect(agentIcon("codex")).toBe("simple-icons:openai");
  });
});

describe("modelIcon", () => {
  test("model_icon_matches_by_substring", () => {
    expect(modelIcon("claude-sonnet-5")).toBe("simple-icons:claude");
    expect(modelIcon("gpt-5.4")).toBe("simple-icons:openai");
    expect(modelIcon("kn/deepseek-v4-1-flash")).toBe("simple-icons:deepseek");
    expect(modelIcon("cbai/kimi-k2.6(high)")).toBe("simple-icons:kimi");
    expect(modelIcon("qwen3-8-max")).toBe("simple-icons:qwen");
  });

  test("model_icon_returns_null_for_glm_family", () => {
    expect(modelIcon("glm-5.2")).toBeNull();
    expect(modelIcon("zhipu-x")).toBeNull();
    expect(modelIcon("chatglm-4")).toBeNull();
  });

  test("model_icon_returns_null_for_unknown_and_null_input", () => {
    expect(modelIcon("totally-unknown")).toBeNull();
    expect(modelIcon(null)).toBeNull();
  });
});

describe("providerIcon", () => {
  test("provider_icon_known_and_unknown", () => {
    expect(providerIcon("anthropic")).toBe("simple-icons:anthropic");
    expect(providerIcon("openai")).toBe("simple-icons:openai");
    expect(providerIcon("deepseek")).toBe("simple-icons:deepseek");
    expect(providerIcon("aki")).toBeNull();
    expect(providerIcon("kn")).toBeNull();
    expect(providerIcon("oa")).toBeNull();
  });
});

describe("fallbackTile", () => {
  test("fallback_tile_is_deterministic_and_uses_palette", () => {
    expect(fallbackTile("aki").color).toBe(fallbackTile("aki").color);
    expect(PALETTE).toContain(fallbackTile("aki").color);
    expect(fallbackTile("aki").initials).toBe("AK");
    expect(fallbackTile("k").initials).toBe("K");
    expect(fallbackTile("-").initials).toBe("?");
  });
});

describe("verified list guard", () => {
  test("every_mapped_name_is_in_the_verified_list", () => {
    const produced: (string | null)[] = [
      agentIcon("claude"),
      agentIcon("opencode"),
      agentIcon("codex"),
      ...["claude", "gpt", "codex", "openai", "deepseek", "kimi", "moonshot", "qwen", "minimax", "mistral", "gemini", "ollama"].map(
        (m) => modelIcon(`${m}-model`),
      ),
      ...["anthropic", "openai", "deepseek"].map((p) => providerIcon(p)),
    ];
    for (const name of produced) {
      expect(name).not.toBeNull();
      expect(VERIFIED).toContain(name as string);
    }
  });
});
