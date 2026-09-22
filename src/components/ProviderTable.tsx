import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ChevronRight } from "lucide-react";
import type { OcAuth, OcProvider } from "@/lib/api";
import { secretMigrateInline } from "@/lib/api";

const AUTH_LABEL: Record<OcAuth, string> = {
  config: "config",
  cli: "CLI",
  none: "belum ada",
};

const AUTH_TONE: Record<OcAuth, string> = {
  config: "text-ok",
  cli: "text-busy",
  none: "text-fg-3",
};

const Head = ({ children, right = false }: { children: string; right?: boolean }) => (
  <th className={`pb-2.5 text-[11px] font-medium text-fg-3 ${right ? "text-right" : "text-left"}`}>
    {children}
  </th>
);

export const ProviderTable = ({
  providers,
  onOpen,
}: {
  providers: OcProvider[];
  onOpen: (id: string) => void;
}) => {
  const qc = useQueryClient();
  const migrate = useMutation({
    mutationFn: (id: string) => secretMigrateInline(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["models-overview"] }),
  });
  const busyId = migrate.isPending ? migrate.variables : null;

  if (providers.length === 0) {
    return (
      <div className="rounded-[10px] border border-dashed border-border p-8 text-center text-sm text-fg-3">
        Belum ada provider opencode di config global.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <table className="w-full border-collapse">
        <thead>
          <tr className="border-b border-border">
            <Head>Provider</Head>
            <Head>Auth</Head>
            <Head right>Model</Head>
            <Head>Contoh model</Head>
            <Head right>Status</Head>
            <th />
          </tr>
        </thead>
        <tbody>
          {providers.map((p) => {
            const examples = p.models.slice(0, 3).map((m) => m.id);
            return (
              <tr
                key={p.id}
                onClick={() => onOpen(p.id)}
                className="cursor-pointer border-b border-border/60 last:border-0 hover:bg-surface-2"
              >
                <td className="py-3 pr-4">
                  <div className="flex flex-col gap-1">
                    <span className="text-[13px] font-medium text-fg">{p.name}</span>
                    <span className="font-mono text-[11.5px] text-fg-3">{p.id}</span>
                  </div>
                </td>
                <td className="py-3 pr-4">
                  <div className="flex flex-col gap-1">
                    <span className={`text-[12.5px] ${AUTH_TONE[p.auth]}`}>{AUTH_LABEL[p.auth]}</span>
                    {p.keyMasked && (
                      <span className="font-mono text-[11.5px] text-fg-3">{p.keyMasked}</span>
                    )}
                  </div>
                </td>
                <td className="py-3 pr-4 text-right font-mono text-[12.5px] text-fg-2">
                  {p.models.length}
                </td>
                <td className="py-3 pr-4">
                  <span className="flex flex-wrap gap-1">
                    {examples.length === 0 ? (
                      <span className="text-[11.5px] text-fg-3">—</span>
                    ) : (
                      examples.map((m) => (
                        <span
                          key={m}
                          className="rounded border border-border px-1.5 py-0.5 font-mono text-[11px] text-fg-2"
                        >
                          {m}
                        </span>
                      ))
                    )}
                  </span>
                </td>
                <td className="py-3 pr-4 text-right">
                  {!p.enabled ? (
                    <span className="text-[11.5px] text-fg-3">nonaktif</span>
                  ) : p.keyInline ? (
                    <span className="text-[11.5px] font-medium text-waiting">plaintext</span>
                  ) : (
                    <span className="text-[11.5px] text-ok">aktif</span>
                  )}
                </td>
                <td className="py-3 text-right">
                  <ChevronRight size={15} className="ml-auto text-fg-3" />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      {migrate.isError && (
        <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
          {String(migrate.error)}
        </div>
      )}

      {providers
        .filter((p) => p.keyInline)
        .map((p) => (
          <div
            key={`inline:${p.id}`}
            className="flex items-center gap-2.5 rounded-md border border-waiting/40 bg-waiting/10 px-3 py-2"
          >
            <AlertTriangle size={14} className="shrink-0 text-waiting" />
            <span className="flex-1 text-[12px] text-fg-2">
              <span className="font-mono text-fg">{p.id}</span> — Key masih plaintext
            </span>
            <button
              type="button"
              disabled={busyId === p.id}
              onClick={() => migrate.mutate(p.id)}
              className="cursor-pointer rounded-md border border-waiting/50 px-2.5 py-1 text-[11.5px] font-medium text-waiting disabled:cursor-default disabled:opacity-45"
            >
              {busyId === p.id ? "Memindahkan…" : "Pindahkan ke file 0600"}
            </button>
          </div>
        ))}
    </div>
  );
};
