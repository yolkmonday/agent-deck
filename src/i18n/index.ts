import { useCallback } from "react";
import { create } from "zustand";
import { en, type MessageKey } from "@/i18n/en";
import { id } from "@/i18n/id";

export type Lang = "en" | "id";
export type Params = Record<string, string | number>;
export type { MessageKey };

const STORAGE_KEY = "agent-deck.lang";
const dictionaries: Record<Lang, Record<MessageKey, string>> = { en, id };

export const isLang = (v: unknown): v is Lang => v === "en" || v === "id";

export const readStoredLang = (): Lang => {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return isLang(v) ? v : "en";
  } catch {
    return "en";
  }
};

const applyDocumentLang = (lang: Lang) => {
  if (typeof document !== "undefined") document.documentElement.lang = lang;
};

export const useLang = create<{ lang: Lang; setLang: (lang: Lang) => void }>((set) => ({
  lang: readStoredLang(),
  setLang: (lang) => {
    try {
      localStorage.setItem(STORAGE_KEY, lang);
    } catch {
      // Preference is a convenience; switching must still work without storage.
    }
    applyDocumentLang(lang);
    set({ lang });
  },
}));

export const interpolate = (template: string, params?: Params): string =>
  params ? template.replace(/\{(\w+)\}/g, (m, k: string) => (k in params ? String(params[k]) : m)) : template;

export const translate = (lang: Lang, key: MessageKey, params?: Params): string =>
  interpolate(dictionaries[lang][key] ?? en[key], params);

/** For non-React code: reads the language at call time. */
export const t = (key: MessageKey, params?: Params): string => translate(useLang.getState().lang, key, params);

export const useT = () => {
  const lang = useLang((s) => s.lang);
  return useCallback((key: MessageKey, params?: Params) => translate(lang, key, params), [lang]);
};

export const locale = (lang: Lang = useLang.getState().lang): "en-US" | "id-ID" =>
  lang === "id" ? "id-ID" : "en-US";

export const syncDocumentLang = () => applyDocumentLang(useLang.getState().lang);
