import { afterEach, describe, expect, test } from "bun:test";
import { en } from "@/i18n/en";
import { id } from "@/i18n/id";
import { interpolate, locale, readStoredLang, t, translate, useLang } from "@/i18n";

afterEach(() => {
  useLang.setState({ lang: "en" });
  delete (globalThis as { localStorage?: unknown }).localStorage;
});

const fakeStorage = (initial: Record<string, string> = {}) => {
  const data = { ...initial };
  (globalThis as { localStorage?: unknown }).localStorage = {
    getItem: (k: string) => data[k] ?? null,
    setItem: (k: string, v: string) => {
      data[k] = v;
    },
  };
  return data;
};

describe("interpolate", () => {
  test("replaces known params", () => {
    expect(interpolate("Version {version} is ready", { version: "0.3.0" })).toBe("Version 0.3.0 is ready");
  });
  test("leaves unknown placeholders visible", () => {
    expect(interpolate("Hi {name}", {})).toBe("Hi {name}");
  });
  test("numbers are stringified", () => {
    expect(interpolate("{n} min", { n: 5 })).toBe("5 min");
  });
});

describe("readStoredLang", () => {
  test("defaults to en when storage is missing", () => {
    expect(readStoredLang()).toBe("en");
  });
  test("reads a valid stored value", () => {
    fakeStorage({ "agent-deck.lang": "id" });
    expect(readStoredLang()).toBe("id");
  });
  test.each(["fr", "", '{"lang":"id"}', "ID"])("invalid value %p falls back to en", (v) => {
    fakeStorage({ "agent-deck.lang": v });
    expect(readStoredLang()).toBe("en");
  });
  test("throwing storage falls back to en", () => {
    (globalThis as { localStorage?: unknown }).localStorage = {
      getItem: () => {
        throw new Error("denied");
      },
    };
    expect(readStoredLang()).toBe("en");
  });
});

describe("useLang", () => {
  test("setLang persists and switches t()", () => {
    const data = fakeStorage();
    useLang.getState().setLang("id");
    expect(data["agent-deck.lang"]).toBe("id");
    expect(t("settings.language")).toBe(id["settings.language"]);
  });
  test("setLang still switches when storage throws", () => {
    (globalThis as { localStorage?: unknown }).localStorage = {
      setItem: () => {
        throw new Error("denied");
      },
    };
    useLang.getState().setLang("id");
    expect(useLang.getState().lang).toBe("id");
  });
});

describe("dictionaries", () => {
  test("id has exactly the en keys", () => {
    expect(Object.keys(id).sort()).toEqual(Object.keys(en).sort());
  });
  test("no empty strings", () => {
    for (const [k, v] of [...Object.entries(en), ...Object.entries(id)]) expect(`${k}=${v}`).not.toBe(`${k}=`);
  });
  test("placeholders match between languages", () => {
    const names = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join(",");
    for (const k of Object.keys(en) as (keyof typeof en)[]) expect(`${k}:${names(id[k])}`).toBe(`${k}:${names(en[k])}`);
  });
  test("translate picks the language", () => {
    expect(translate("en", "settings.language")).toBe("Language");
  });
  test("locale maps lang to BCP 47", () => {
    expect(locale("en")).toBe("en-US");
    expect(locale("id")).toBe("id-ID");
  });
});
