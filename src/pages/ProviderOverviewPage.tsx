import { useQuery } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { AgentIcon, ModelIcon } from "@/components/BrandIcon";
import { ModelChips } from "@/components/ModelChips";
import { ProviderTable } from "@/components/ProviderTable";
import { useT } from "@/i18n";
import { modelsOverview } from "@/lib/api";
import { useNavigate } from "@/lib/nav";

export const ProviderOverviewPage = ({ onNew }: { onNew: () => void }) => {
  const t = useT();
  const open = useNavigate();
  const query = useQuery({ queryKey: ["models-overview"], queryFn: modelsOverview });

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center justify-between px-7 py-4.5">
        <div className="flex flex-col gap-0.75">
          <h1 className="text-[22px] font-semibold">{t("providerOverviewPage.title")}</h1>
          <span className="text-[12.5px] text-fg-2">{t("providerOverviewPage.subtitle")}</span>
        </div>
        <button
          type="button"
          onClick={onNew}
          className="ad-interactive ad-press flex cursor-pointer items-center gap-1.5 rounded-md bg-busy px-3.5 py-2 text-[13px] font-semibold text-bg hover:opacity-90"
        >
          <Plus size={15} />
          {t("providerOverviewPage.newProvider")}
        </button>
      </header>

      <main className="flex min-w-0 flex-1 flex-col gap-6.5 overflow-y-auto px-7 pb-7">
        {query.isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(query.error)}
          </div>
        )}

        {query.isPending && (
          <div className="flex h-64 items-center justify-center text-sm text-fg-3">
            {t("providerOverviewPage.loading")}
          </div>
        )}

        {query.data && (
          <>
            <section className="flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <AgentIcon agent="claude" size={16} className="text-claude" />
                <span className="text-sm font-semibold">Claude</span>
              </div>
              <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                <ModelChips
                  available={query.data.claude.models}
                  recentlyUsed={query.data.claude.recentlyUsed}
                />
              </div>
            </section>

            <section className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <AgentIcon agent="opencode" size={16} className="text-opencode" />
                  <span className="text-sm font-semibold">opencode</span>
                </div>
                <span className="text-xs text-fg-3">{t("providerOverviewPage.opencodeNote")}</span>
              </div>
              <div className="rounded-[10px] border border-border bg-surface px-5 py-4">
                <ProviderTable providers={query.data.opencode} onOpen={open.provider} />
              </div>
            </section>

            <section className="flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <AgentIcon agent="codex" size={16} className="text-codex" />
                <span className="text-sm font-semibold">Codex</span>
              </div>
              <div className="flex flex-col gap-2 rounded-[10px] border border-border bg-surface px-5 py-4">
                {query.data.codex.models.length === 0 ? (
                  <span className="text-xs text-fg-3">{t("providerOverviewPage.noCodexModels")}</span>
                ) : (
                  <div className="flex flex-wrap gap-1.5">
                    {query.data.codex.models.map((m) => (
                      <span
                        key={m}
                        className="flex items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 py-1 font-mono text-[12px] text-fg"
                      >
                        <ModelIcon model={m} size={13} />
                        {m}
                      </span>
                    ))}
                  </div>
                )}
                <span className="text-[11.5px] text-fg-3">{query.data.codex.note}</span>
              </div>
            </section>
          </>
        )}
      </main>
    </div>
  );
};
