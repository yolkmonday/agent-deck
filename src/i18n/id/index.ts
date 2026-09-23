import type { MessageKey } from "@/i18n/en";
import { common } from "@/i18n/id/common";
import { componentsA } from "@/i18n/id/components-a";
import { componentsB } from "@/i18n/id/components-b";
import { format } from "@/i18n/id/format";
import { lib } from "@/i18n/id/lib";
import { pages } from "@/i18n/id/pages";
import { settings } from "@/i18n/id/settings";
import { update } from "@/i18n/id/update";

export const id: Record<MessageKey, string> = {
  ...common,
  ...settings,
  ...format,
  ...lib,
  ...componentsA,
  ...componentsB,
  ...pages,
  ...update,
};
