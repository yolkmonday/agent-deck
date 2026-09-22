import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * ClaudeToolCall — Claude Code's collapsed tool/result line.
 *
 * In the terminal this is faked with box-drawing glyphs and a "ctrl+o to
 * expand" hint. Here it's a real <details> disclosure: keyboard-operable,
 * announced to screen readers, and it keeps the exact ⏺ / ⎿ visual grammar.
 *
 * Local addition to the upstream brainless component: `expandHint`, so a surface
 * that is not the terminal does not tell the user to press ctrl+o. A browser
 * expands a <details> by clicking its summary.
 */
type Status = "success" | "error" | "pending";

const STATUS_COLOR: Record<Status, string> = {
  success: "#4ea96f",
  error: "#f7768e",
  pending: "#e0af68",
};

export function ClaudeToolCall({
  tool,
  arg,
  result,
  status = "success",
  defaultOpen = false,
  expandHint = "(ctrl+o to expand)",
  className,
  children,
}: {
  tool: string;
  arg?: string;
  result: string;
  status?: Status;
  defaultOpen?: boolean;
  /** Pass `null` to show no hint, or a surface-appropriate one. */
  expandHint?: string | null;
  className?: string;
  children?: React.ReactNode;
}) {
  const expandable = Boolean(children);

  return (
    <details
      open={defaultOpen}
      className={cn(
        "group font-mono text-[13px] leading-[1.55] [&_summary::-webkit-details-marker]:hidden",
        className,
      )}
    >
      <summary
        className={cn(
          "list-none",
          expandable ? "cursor-pointer" : "cursor-default",
          "rounded-none outline-none focus-visible:ring-1 focus-visible:ring-[#7dcfff]/60",
        )}
      >
        <span className="flex min-w-0 items-baseline gap-2">
          <span aria-hidden className="shrink-0" style={{ color: STATUS_COLOR[status] }}>
            ⏺
          </span>
          <span className="min-w-0 break-words">
            <span className="text-[#c0caf5]">{tool}</span>
            {arg !== undefined ? (
              <>
                <span className="text-[#565f89]">(</span>
                <span className="text-[#7dcfff]">{arg}</span>
                <span className="text-[#565f89]">)</span>
              </>
            ) : null}
          </span>
        </span>
        <span className="flex min-w-0 items-baseline gap-2 text-[#8b8fa3]">
          {/* invisible status glyph spacer: aligns ⎿ under the tool name */}
          <span aria-hidden className="invisible shrink-0">
            ⏺
          </span>
          <span className="flex min-w-0 items-baseline gap-2">
            <span aria-hidden className="shrink-0 text-[#565f89]">
              ⎿
            </span>
            <span className="min-w-0 break-words">
              {result}
              {expandable && expandHint ? (
                <span className="ml-2 text-[#565f89] group-open:hidden">
                  {expandHint}
                </span>
              ) : null}
            </span>
          </span>
        </span>
      </summary>

      {expandable ? (
        <div className="mt-1 whitespace-pre-wrap pl-[32px] text-[#8b8fa3]">
          {children}
        </div>
      ) : null}
    </details>
  );
}
