export const lib = {
  "nav.live": "Live",
  "nav.activity": "Activity",
  "nav.token": "Tokens & Cost",
  "nav.timeline": "Timeline",
  "nav.savings": "Token Savings",
  "nav.terminal": "Terminal",
  "nav.provider": "Model & Provider",
  "nav.project": "Project",

  "attention.waitingLabel": "{project} needs an answer · {duration}",
  "attention.health.stalled": "stalled",
  "attention.health.slow": "slow",
  "attention.healthLabel": "{project} {state} · {reason}",
  "attention.orphanLabel": "{agent} running in {cwd} but no session · {duration}",

  "events.started": "session started",
  "events.waiting": "needs an answer",
  "events.idle": "idle",
  "events.busyAgain": "busy again",

  "models.defaultLimitsNote": "These numbers are estimates. Check the provider's docs.",

  "notify.waiting.title": "{project} needs an answer",
  "notify.waiting.body": "{who} · waiting for input",
  "notify.stalled.title": "{project} stalled",
  "notify.stalled.bodyFallback": "{who} · no progress",

  "activity.idle": "Idle",
  "activity.waiting": "Waiting for your answer",
  "activity.thinking": "Thinking",
  "activity.done": "Done",

  "cost.notionalHint": "Estimate if billed per token. Doesn't add to the bill.",
} as const;
