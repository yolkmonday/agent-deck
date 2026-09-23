import { useEffect, useRef, useState, type ReactNode } from "react";
import { Icon } from "@iconify/react";
import { AgentIcon } from "@/components/BrandIcon";
import { ClaudeMessage } from "@/components/brainless/claude/claude-message";
import { ClaudeToolCall } from "@/components/brainless/claude/claude-tool-call";
import { useT, type MessageKey } from "@/i18n";
import { sessionTail, type TailEntry } from "@/lib/api";
import type { Session } from "@/lib/types";

const POLL_MS = 1500;
const NEAR_BOTTOM_PX = 24;

const statusLabel: Record<"busy" | "waiting" | "idle", MessageKey> = {
  busy: "transcriptModal.statusBusy",
  waiting: "transcriptModal.statusWaiting",
  idle: "transcriptModal.statusIdle",
};
const statusPill = {
  busy: "bg-busy/15 text-busy",
  waiting: "bg-waiting/15 text-waiting",
  idle: "bg-idle/15 text-idle",
} as const;
const statusIcon = {
  busy: "lucide:loader-circle",
  waiting: "lucide:message-circle-question",
  idle: "lucide:circle-pause",
} as const;
const healthPill = { stalled: "bg-err/15 text-err", slow: "bg-waiting/15 text-waiting" } as const;
const healthIcon = { stalled: "lucide:octagon-alert", slow: "lucide:hourglass" } as const;
const healthText: Record<"stalled" | "slow", MessageKey> = {
  stalled: "transcriptModal.healthStalled",
  slow: "transcriptModal.healthSlow",
};
const agentText = { claude: "text-claude", opencode: "text-opencode", codex: "text-codex" } as const;

const toolStatus = { running: "pending", ok: "success", error: "error" } as const;
const toolStatusKey: Record<"running" | "ok" | "error", MessageKey> = {
  running: "transcriptModal.toolRunning",
  ok: "transcriptModal.toolOk",
  error: "transcriptModal.toolError",
};

// Mirrors the collector's `tool_detail`: the one argument that names the call.
const ARG_KEYS = ["command", "file_path", "pattern", "description", "prompt", "url"];

const oneLine = (s: string, max: number): string => {
  const line = s.split("\n")[0].trim();
  return line.length > max ? `${line.slice(0, max)}…` : line;
};

const argSummary = (input: string): string => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(input);
  } catch {
    return oneLine(input, 100);
  }
  if (parsed && typeof parsed === "object") {
    const obj = parsed as Record<string, unknown>;
    for (const key of ARG_KEYS) {
      const value = obj[key];
      if (typeof value === "string" && value) return oneLine(value, 100);
    }
    const first = Object.values(obj).find((v) => typeof v === "string" && v);
    if (typeof first === "string") return oneLine(first, 100);
  }
  return oneLine(input, 100);
};

const Entry = ({ entry }: { entry: TailEntry }) => {
  const t = useT();
  switch (entry.kind) {
    case "user":
      return <ClaudeMessage role="user">{entry.text}</ClaudeMessage>;
    case "assistant":
      return <ClaudeMessage role="assistant">{entry.text}</ClaudeMessage>;
    // `claude-thinking` is a live status line: it returns null once it stops, so a
    // historical block gets its own plain disclosure instead of a fake stopwatch.
    case "thinking":
      return (
        <details className="font-mono text-[13px] leading-[1.55] [&_summary::-webkit-details-marker]:hidden">
          <summary className="cursor-pointer list-none text-fg-3 hover:text-fg-2">
            <span aria-hidden>✻ </span>
            {t("transcriptModal.thinking")}
          </summary>
          <div className="mt-1 pl-4 whitespace-pre-wrap text-fg-3">{entry.text}</div>
        </details>
      );
    case "tool":
      return (
        <ClaudeToolCall
          tool={entry.name || "tool"}
          arg={argSummary(entry.input)}
          result={t(toolStatusKey[entry.status])}
          status={toolStatus[entry.status]}
          expandHint={t("transcriptModal.expandHint")}
        >
          {entry.input}
        </ClaudeToolCall>
      );
    case "result":
      return (
        <ClaudeToolCall
          tool={entry.toolName || "tool"}
          result={entry.preview}
          status={entry.isError ? "error" : "success"}
        />
      );
  }
};

const Chip = ({
  active,
  title,
  onClick,
  children,
}: {
  active: boolean;
  title?: string;
  onClick: () => void;
  children: ReactNode;
}) => (
  <button
    type="button"
    onClick={onClick}
    title={title}
    aria-pressed={active}
    className={`flex cursor-pointer items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11.5px] font-semibold ${
      active ? "border-border bg-surface-2 text-fg" : "border-transparent bg-bg text-fg-3 hover:text-fg-2"
    }`}
  >
    {children}
  </button>
);

export const TranscriptModal = ({
  session,
  agentId = null,
  onClose,
}: {
  session: Session;
  agentId?: string | null;
  onClose: () => void;
}) => {
  const t = useT();
  const [agent, setAgent] = useState<string | null>(agentId ?? null);
  const [entries, setEntries] = useState<TailEntry[]>([]);
  const [found, setFound] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [atBottom, setAtBottom] = useState(true);
  const listRef = useRef<HTMLDivElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);

  const { id, cwd } = session;

  useEffect(() => {
    closeRef.current = onClose;
  }, [onClose]);

  // Polling lives and dies with the modal: no closed viewer keeps reading disk.
  useEffect(() => {
    let cancelled = false;
    const tick = () => {
      void sessionTail(id, cwd, agent)
        .then((tail) => {
          if (cancelled) return;
          setEntries(tail.entries);
          setFound(tail.found);
          setError(null);
        })
        .catch((e: unknown) => {
          if (cancelled) return;
          setError(typeof e === "string" ? e : t("transcriptModal.readError"));
        });
    };
    tick();
    const timer = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [id, cwd, agent]);

  useEffect(() => {
    setEntries([]);
    setAtBottom(true);
  }, [agent]);

  useEffect(() => {
    if (!atBottom) return;
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries, atBottom]);

  // Escape closes, Tab cycles inside, and focus goes back to whatever opened this.
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const focusables = () =>
      Array.from(
        panel?.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, summary, [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      ).filter((el) => !el.hasAttribute("disabled"));
    panel?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        closeRef.current();
        return;
      }
      if (e.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (active === panel || (!e.shiftKey && active === last)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      } else if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      if (previous && previous !== document.body) previous.focus();
    };
  }, []);

  const onScroll = () => {
    const el = listRef.current;
    if (!el) return;
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight <= NEAR_BOTTOM_PX);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
      role="dialog"
      aria-modal="true"
      aria-label={t("transcriptModal.ariaLabel", { project: session.project })}
      onClick={onClose}
    >
      <div
        ref={panelRef}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        className="flex h-[80vh] w-full max-w-[900px] flex-col rounded-xl border border-border bg-surface outline-none"
      >
        <div className="flex items-center gap-3 border-b border-border px-5 py-3.5">
          <AgentIcon agent={session.agent} size={16} className={`shrink-0 ${agentText[session.agent]}`} />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="truncate text-sm font-semibold" title={session.project}>
              {session.project}
            </span>
            <span className="truncate font-mono text-[11.5px] text-fg-3" title={session.cwd}>
              {session.model ?? "-"}
            </span>
          </div>
          {session.health === "ok" ? (
            <span
              className={`flex shrink-0 items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${statusPill[session.status]}`}
            >
              <Icon
                icon={statusIcon[session.status]}
                width={12}
                height={12}
                className={session.status === "busy" ? "animate-spin [animation-duration:2s] motion-reduce:animate-none" : ""}
              />
              {t(statusLabel[session.status])}
            </span>
          ) : (
            <span
              className={`flex shrink-0 items-center gap-1.5 rounded-full px-2.25 py-0.75 text-[11.5px] font-semibold ${healthPill[session.health]}`}
            >
              <Icon icon={healthIcon[session.health]} width={12} height={12} />
              {t(healthText[session.health])}
            </span>
          )}
          <button
            type="button"
            onClick={onClose}
            aria-label={t("common.close")}
            className="shrink-0 cursor-pointer text-fg-3 hover:text-fg"
          >
            <Icon icon="lucide:x" width={16} height={16} />
          </button>
        </div>
        {session.subagents.length > 0 && (
          <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-5 py-2.5">
            <Chip active={agent === null} onClick={() => setAgent(null)}>
              {t("transcriptModal.main")}
            </Chip>
            {session.subagents.map((sub) => (
              <Chip
                key={sub.id}
                active={agent === sub.id}
                title={sub.description}
                onClick={() => setAgent(sub.id)}
              >
                <Icon icon="lucide:git-branch" width={11} height={11} />
                {sub.agentType}
              </Chip>
            ))}
          </div>
        )}
        <div className="relative min-h-0 flex-1">
          <div
            ref={listRef}
            onScroll={onScroll}
            className="flex h-full flex-col gap-2.5 overflow-y-auto px-5 py-4"
          >
            {error && (
              <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 text-xs text-err">
                {error}
              </div>
            )}
            {entries.length === 0 &&
              (found ? (
                <span className="py-6 text-center text-xs text-fg-3">{t("transcriptModal.emptyText")}</span>
              ) : (
                <span className="py-6 text-center text-xs text-fg-3">{t("transcriptModal.noFile")}</span>
              ))}
            {entries.map((entry, i) => (
              <Entry key={i} entry={entry} />
            ))}
          </div>
          {!atBottom && entries.length > 0 && (
            <button
              type="button"
              onClick={() => setAtBottom(true)}
              className="absolute right-5 bottom-3 flex cursor-pointer items-center gap-1.5 rounded-full border border-border bg-surface-2 px-3 py-1.5 text-[11.5px] font-semibold text-fg-2"
            >
              <Icon icon="lucide:arrow-down" width={12} height={12} />
              {t("transcriptModal.scrollToBottom")}
            </button>
          )}
        </div>
        <div className="flex items-center gap-2 border-t border-border px-5 py-2.5 text-[11.5px] text-fg-3">
          <Icon icon="lucide:eye-off" width={12} height={12} />
          {t("transcriptModal.footerHint")}
        </div>
      </div>
    </div>
  );
};
