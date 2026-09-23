import { common } from "@/i18n/en/common";
import { componentsA } from "@/i18n/en/components-a";
import { componentsB } from "@/i18n/en/components-b";
import { format } from "@/i18n/en/format";
import { lib } from "@/i18n/en/lib";
import { pages } from "@/i18n/en/pages";
import { settings } from "@/i18n/en/settings";
import { update } from "@/i18n/en/update";

export const en = {
  ...common,
  ...settings,
  ...format,
  ...lib,
  ...componentsA,
  ...componentsB,
  ...pages,
  ...update,
} as const;

export type MessageKey = keyof typeof en;
