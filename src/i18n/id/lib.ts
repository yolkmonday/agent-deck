import type { lib as en } from "@/i18n/en/lib";

export const lib: Record<keyof typeof en, string> = {
  "attention.waitingLabel": "{project} butuh jawaban · {duration}",
  "attention.health.stalled": "macet",
  "attention.health.slow": "lambat",
  "attention.healthLabel": "{project} {state} · {reason}",
  "attention.orphanLabel": "{agent} jalan di {cwd} tapi tidak ada sesi · {duration}",

  "events.started": "sesi dimulai",
  "events.waiting": "butuh jawaban",
  "events.idle": "diam",
  "events.busyAgain": "sibuk lagi",

  "models.defaultLimitsNote": "Angka ini perkiraan. Sesuaikan dengan dokumentasi provider.",

  "notify.waiting.title": "{project} butuh jawaban",
  "notify.waiting.body": "{who} · menunggu input",
  "notify.stalled.title": "{project} macet",
  "notify.stalled.bodyFallback": "{who} · tidak ada kemajuan",

  "activity.idle": "Diam",
  "activity.waiting": "Menunggu jawaban kamu",
  "activity.thinking": "Berpikir",
  "activity.done": "Selesai",

  "cost.notionalHint": "Perkiraan kalau dibayar per token. Tidak menambah tagihan.",
};
