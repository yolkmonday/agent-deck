import { AgentIcon, ModelIcon } from "@/components/BrandIcon";
import type { ModelRow } from "@/lib/api";
import { formatUsd } from "@/lib/cost";
import { formatTokens } from "@/lib/format";

const dot: Record<string, string> = {
  claude: "bg-claude",
  opencode: "bg-opencode",
  codex: "bg-codex",
};

const text: Record<string, string> = {
  claude: "text-claude",
  opencode: "text-opencode",
  codex: "text-codex",
};

const label: Record<string, string> = { claude: "Claude", opencode: "opencode", codex: "Codex" };

const Head = ({ children, right = false }: { children: string; right?: boolean }) => (
  <th className={`pb-2.5 text-[11px] font-medium text-fg-3 ${right ? "text-right" : "text-left"}`}>{children}</th>
);

export const ModelTable = ({ rows }: { rows: ModelRow[] }) => {
  const sorted = [...rows].sort((a, b) => b.costUsd - a.costUsd);
  const max = sorted.reduce((acc, r) => Math.max(acc, r.costUsd), 0);
  return (
    <table className="w-full border-collapse">
      <thead>
        <tr className="border-b border-border">
          <Head>Model</Head>
          <Head>Agent</Head>
          <Head right>Input</Head>
          <Head right>Output</Head>
          <Head right>Cache</Head>
          <Head right>Biaya</Head>
        </tr>
      </thead>
      <tbody>
        {sorted.map((r) => {
          const cache = r.tokens.cacheRead + r.tokens.cacheWrite;
          return (
            <tr key={`${r.agent}:${r.model}`} className="border-b border-border/60 last:border-0">
              <td className="py-3">
                <div className="flex flex-col gap-1.5">
                  <span className="flex items-center gap-1.5 font-mono text-[12.5px] text-fg">
                    <ModelIcon model={r.model} size={14} />
                    {r.model}
                  </span>
                  {max > 0 && (
                    <span className="h-0.75 w-24 overflow-hidden rounded-full bg-surface-2">
                      <span
                        className={`block h-full rounded-full ${dot[r.agent] ?? "bg-idle"}`}
                        style={{ width: `${Math.max(2, (r.costUsd / max) * 100)}%` }}
                      />
                    </span>
                  )}
                </div>
              </td>
              <td className="py-3">
                <span className="flex items-center gap-2 text-[12.5px] text-fg-2">
                  <AgentIcon agent={r.agent as "claude" | "opencode" | "codex"} size={14} className={text[r.agent] ?? "text-fg-3"} />
                  {label[r.agent] ?? r.agent}
                </span>
              </td>
              <td className="py-3 text-right font-mono text-[12.5px] text-fg-2">{formatTokens(r.tokens.input)}</td>
              <td className="py-3 text-right font-mono text-[12.5px] text-fg-2">{formatTokens(r.tokens.output)}</td>
              <td className="py-3 text-right font-mono text-[12.5px] text-fg-2">{formatTokens(cache)}</td>
              <td className="py-3 text-right font-mono text-[12.5px] font-semibold text-fg">{formatUsd(r.costUsd)}</td>
            </tr>
          );
        })}
      </tbody>
      <tfoot>
        <tr className="border-t border-border">
          <td className="pt-3 text-[12.5px] font-semibold text-fg-2">Total</td>
          <td />
          <td colSpan={3} className="pt-3 text-right text-[11.5px] text-fg-3">
            {sorted.reduce((acc, r) => acc + r.messages, 0)} pesan
          </td>
          <td className="pt-3 text-right font-mono text-[12.5px] font-semibold text-fg">
            {formatUsd(sorted.reduce((acc, r) => acc + r.costUsd, 0))}
          </td>
        </tr>
      </tfoot>
    </table>
  );
};
