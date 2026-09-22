import { totalTokens } from "@/lib/format";
import type { Session } from "@/lib/types";

export const summarize = (sessions: Session[]) => {
  const count = (status: Session["status"]) => sessions.filter((s) => s.status === status).length;
  return {
    active: sessions.length,
    busy: count("busy"),
    waiting: count("waiting"),
    idle: count("idle"),
    tokens: sessions.reduce((sum, s) => sum + totalTokens(s.tokens), 0),
    cost: sessions.reduce((sum, s) => sum + s.costUsd, 0),
    unpriced: sessions.filter((s) => !s.priced).length,
  };
};
