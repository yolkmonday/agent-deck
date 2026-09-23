import type { settings as en } from "@/i18n/en/settings";

export const settings: Record<keyof typeof en, string> = {
  "settings.attention.title": "Saat ada agent bertanya",
  "settings.attention.off": "Diam",
  "settings.attention.notify": "Beri tahu",
  "settings.attention.auto": "Langsung buka",
  "settings.notifySound": "Suara notifikasi",
  "settings.stallAfter": "Anggap macet setelah",
  "settings.slowToolAfter": "Anggap tool lambat setelah",
  "settings.hint":
    "Langsung buka akan memindahkan layar sendiri saat ada agent yang bertanya. Tool yang sedang jalan tidak dihitung macet.",
  "settings.language": "Bahasa",
};
