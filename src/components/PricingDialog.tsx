import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { pricingGet, pricingSet, type PriceEntry } from "@/lib/api";

const COLUMNS = [
  { key: "model", label: "Model", width: "w-56" },
  { key: "inputPerM", label: "Input", width: "w-24" },
  { key: "outputPerM", label: "Output", width: "w-24" },
  { key: "cacheReadPerM", label: "Cache read", width: "w-24" },
  { key: "cacheWritePerM", label: "Cache write", width: "w-24" },
] as const;

type Row = Record<(typeof COLUMNS)[number]["key"], string>;

const toRow = (e: PriceEntry): Row => ({
  model: e.model,
  inputPerM: String(e.inputPerM),
  outputPerM: String(e.outputPerM),
  cacheReadPerM: String(e.cacheReadPerM),
  cacheWritePerM: String(e.cacheWritePerM),
});

const isNum = (s: string) => s.trim() !== "" && Number.isFinite(Number(s));
const valid = (r: Row) =>
  r.model.trim() !== "" &&
  isNum(r.inputPerM) &&
  isNum(r.outputPerM) &&
  isNum(r.cacheReadPerM) &&
  isNum(r.cacheWritePerM);

const blank = (): Row => toRow({ model: "", inputPerM: 0, outputPerM: 0, cacheReadPerM: 0, cacheWritePerM: 0 });

export const PricingDialog = ({ onClose }: { onClose: () => void }) => {
  const qc = useQueryClient();
  const [rows, setRows] = useState<Row[] | null>(null);
  const query = useQuery({ queryKey: ["pricing"], queryFn: pricingGet });
  const save = useMutation({
    mutationFn: (entries: PriceEntry[]) => pricingSet(entries),
    onSuccess: (entries) => {
      qc.setQueryData(["pricing"], entries);
      void qc.invalidateQueries({ queryKey: ["history"] });
      onClose();
    },
  });

  useEffect(() => {
    if (rows === null && query.data) setRows(query.data.map(toRow));
  }, [query.data, rows]);

  const allValid = rows !== null && rows.length > 0 && rows.every(valid);
  const patch = (i: number, key: keyof Row, value: string) =>
    setRows((prev) => prev?.map((r, idx) => (idx === i ? { ...r, [key]: value } : r)) ?? prev);

  const submit = () => {
    if (!rows || !allValid) return;
    save.mutate(
      rows.map((r) => ({
        model: r.model.trim(),
        inputPerM: Number(r.inputPerM),
        outputPerM: Number(r.outputPerM),
        cacheReadPerM: Number(r.cacheReadPerM),
        cacheWritePerM: Number(r.cacheWritePerM),
      })),
    );
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6" onClick={onClose}>
      <div
        className="flex max-h-[85vh] w-full max-w-3xl flex-col gap-4 overflow-hidden rounded-xl border border-border bg-surface p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between">
          <div className="flex flex-col gap-1">
            <h2 className="text-base font-semibold">Harga model</h2>
            <span className="text-xs text-fg-2">USD per satu juta token.</span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="ad-interactive ad-press cursor-pointer rounded-md px-2 py-1 text-xs text-fg-3 hover:text-fg"
          >
            Tutup
          </button>
        </div>

        {query.isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(query.error)}
          </div>
        )}

        {rows === null ? (
          <div className="py-8 text-center text-sm text-fg-3">Memuat harga…</div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <table className="w-full border-collapse">
              <thead>
                <tr className="border-b border-border">
                  {COLUMNS.map((c) => (
                    <th key={c.key} className="pb-2.5 text-left text-[11px] font-medium text-fg-3">
                      {c.label}
                    </th>
                  ))}
                  <th />
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => (
                  <tr key={i} className="border-b border-border/60">
                    {COLUMNS.map((c) => {
                      const invalid = c.key === "model" ? r.model.trim() === "" : !isNum(r[c.key]);
                      return (
                        <td key={c.key} className="py-1.5 pr-2">
                          <input
                            value={r[c.key]}
                            onChange={(e) => patch(i, c.key, e.target.value)}
                            className={`${c.width} rounded-md border bg-bg px-2 py-1.5 font-mono text-[12.5px] text-fg outline-none ${
                              invalid ? "border-err" : "border-border focus:border-busy"
                            }`}
                          />
                        </td>
                      );
                    })}
                    <td className="py-1.5 text-right">
                      <button
                        type="button"
                        onClick={() => setRows((prev) => prev?.filter((_, idx) => idx !== i) ?? prev)}
                        className="ad-interactive ad-press cursor-pointer rounded-md px-2 py-1 text-xs text-fg-3 hover:bg-err/10 hover:text-err"
                      >
                        Hapus
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!allValid && <span className="mt-2 block text-xs text-err">Harus angka</span>}
          </div>
        )}

        <div className="flex items-center justify-between border-t border-border pt-4">
          <span className="text-xs text-fg-3">Harga ini perkiraan. Sesuaikan dengan tagihan asli kamu.</span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setRows((prev) => [...(prev ?? []), blank()])}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:border-fg-3 hover:text-fg"
            >
              Tambah model
            </button>
            <button
              type="button"
              disabled={!allValid || save.isPending}
              onClick={submit}
              className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-4 py-1.75 text-xs font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
            >
              {save.isPending ? "Menyimpan…" : "Simpan"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
