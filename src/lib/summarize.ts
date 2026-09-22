import { totalTokens } from "@/lib/format";
import type { Session } from "@/lib/types";

export const summarize = (sessions: Session[]) => {
  const count = (status: Session["status"]) => sessions.filter((s) => s.status === status).length;
  // A subscription's tokens are already paid for, so they never reach spend. The
  // notional figure is what the same tokens would have cost at API rates.
  const notional = sessions.reduce((sum, s) => sum + s.costUsd, 0);
  const spend = sessions.reduce(
    (sum, s) => sum + (s.billingMode === "subscription" ? 0 : s.costUsd),
    0,
  );
  return {
    active: sessions.length,
    busy: count("busy"),
    waiting: count("waiting"),
    idle: count("idle"),
    tokens: sessions.reduce((sum, s) => sum + totalTokens(s.tokens), 0),
    cost: spend,
    spend,
    notional,
    unpriced: sessions.filter((s) => !s.priced).length,
    stalled: sessions.filter((s) => s.health === "stalled").length,
  };
};
