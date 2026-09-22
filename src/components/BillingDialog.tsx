import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { billingAccounts, billingSave, type BillingAccount } from "@/lib/api";
import type { BillingMode } from "@/lib/types";

const MODES: { value: BillingMode; label: string; hint: string }[] = [
  { value: "subscription", label: "Langganan", hint: "Bayar bulanan tetap, berapa pun token yang dipakai." },
  { value: "prepaid", label: "Prabayar", hint: "Saldo diisi di muka, lalu terpotong per token." },
  { value: "payg", label: "Per token", hint: "Ditagih sesuai pemakaian, tanpa komitmen." },
];

type Row = Omit<BillingAccount, "monthlyUsd" | "renewalDay" | "creditUsd"> & {
  monthlyUsd: string;
  renewalDay: string;
  creditUsd: string;
};

const toRow = (a: BillingAccount): Row => ({
  ...a,
  matches: [...a.matches],
  monthlyUsd: a.monthlyUsd === null ? "" : String(a.monthlyUsd),
  renewalDay: a.renewalDay === null ? "" : String(a.renewalDay),
  creditUsd: a.creditUsd === null ? "" : String(a.creditUsd),
});

const blank = (): Row => ({
  id: `b${Date.now().toString(16)}`,
  label: "",
  mode: "subscription",
  matches: [],
  monthlyUsd: "",
  renewalDay: "",
  creditUsd: "",
  startedOn: null,
  expiresOn: null,
});

const isNum = (s: string) => s.trim() !== "" && Number.isFinite(Number(s));
const optionalDate = (s: string) => s.trim() === "" || /^\d{4}-\d{2}-\d{2}$/.test(s.trim());

/** Only the fields the chosen mode uses are validated, and only they are sent. */
const valid = (r: Row): boolean => {
  if (r.label.trim() === "" || r.matches.every((m) => m.trim() === "")) return false;
  if (!optionalDate(r.startedOn ?? "") || !optionalDate(r.expiresOn ?? "")) return false;
  if (r.mode === "subscription") {
    const day = Number(r.renewalDay);
    return isNum(r.monthlyUsd) && Number(r.monthlyUsd) >= 0 && Number.isInteger(day) && day >= 1 && day <= 28;
  }
  if (r.mode === "prepaid") return isNum(r.creditUsd) && Number(r.creditUsd) >= 0;
  return true;
};

const toAccount = (r: Row): BillingAccount => ({
  id: r.id,
  label: r.label.trim(),
  mode: r.mode,
  matches: r.matches.map((m) => m.trim()).filter((m) => m !== ""),
  monthlyUsd: r.mode === "subscription" ? Number(r.monthlyUsd) : null,
  renewalDay: r.mode === "subscription" ? Number(r.renewalDay) : null,
  creditUsd: r.mode === "prepaid" ? Number(r.creditUsd) : null,
  startedOn: r.mode === "prepaid" ? r.startedOn?.trim() || null : null,
  expiresOn: r.expiresOn?.trim() || null,
});

const Field = ({ label, children }: { label: string; children: React.ReactNode }) => (
  <label className="flex min-w-0 flex-col gap-1">
    <span className="text-[11px] text-fg-3">{label}</span>
    {children}
  </label>
);

const input =
  "rounded-md border bg-bg px-2 py-1.5 font-mono text-[12.5px] text-fg outline-none border-border focus:border-busy";

export const BillingDialog = ({ onClose }: { onClose: () => void }) => {
  const qc = useQueryClient();
  const [rows, setRows] = useState<Row[] | null>(null);
  const query = useQuery({ queryKey: ["billing", "accounts"], queryFn: billingAccounts });
  const save = useMutation({
    mutationFn: (accounts: BillingAccount[]) => billingSave(accounts),
    onSuccess: (accounts) => {
      qc.setQueryData(["billing", "accounts"], accounts);
      void qc.invalidateQueries({ queryKey: ["billing", "summary"] });
      void qc.invalidateQueries({ queryKey: ["live"] });
      onClose();
    },
  });

  useEffect(() => {
    if (rows === null && query.data) setRows(query.data.map(toRow));
  }, [query.data, rows]);

  const patch = (i: number, next: Partial<Row>) =>
    setRows((prev) => prev?.map((r, idx) => (idx === i ? { ...r, ...next } : r)) ?? prev);

  const allValid = rows !== null && rows.every(valid);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-bg/70 px-6" onClick={onClose}>
      <div
        className="flex max-h-[85vh] w-full max-w-3xl flex-col gap-4 overflow-hidden rounded-xl border border-border bg-surface p-6"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between">
          <div className="flex flex-col gap-1">
            <h2 className="text-base font-semibold">Langganan &amp; saldo</h2>
            <span className="text-xs text-fg-2">
              Tandai model mana yang sudah dibayar bulanan atau pakai saldo, supaya estimasi biaya tidak
              dihitung sebagai tagihan.
            </span>
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
          <div className="py-8 text-center text-sm text-fg-3">Memuat akun…</div>
        ) : rows.length === 0 ? (
          <div className="rounded-[10px] border border-dashed border-border p-6 text-center text-sm text-fg-3">
            Belum ada akun. Kosongkan kalau semua model dibayar per token.
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
            {rows.map((r, i) => (
              <div key={r.id} className="flex flex-col gap-3 rounded-[10px] border border-border bg-bg p-4">
                <div className="flex items-end gap-3">
                  <Field label="Nama akun">
                    <input
                      value={r.label}
                      onChange={(e) => patch(i, { label: e.target.value })}
                      placeholder="Claude Max"
                      className={`${input} w-48 ${r.label.trim() === "" ? "border-err" : ""}`}
                    />
                  </Field>
                  <Field label="Cara bayar">
                    <select
                      value={r.mode}
                      onChange={(e) => patch(i, { mode: e.target.value as BillingMode })}
                      className={`${input} w-36 cursor-pointer`}
                    >
                      {MODES.map((m) => (
                        <option key={m.value} value={m.value}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                  </Field>
                  <span className="flex-1 pb-1.5 text-[11.5px] text-fg-3">
                    {MODES.find((m) => m.value === r.mode)?.hint}
                  </span>
                  <button
                    type="button"
                    onClick={() => setRows((prev) => prev?.filter((_, idx) => idx !== i) ?? prev)}
                    className="ad-interactive ad-press cursor-pointer rounded-md px-2 py-1.5 text-xs text-fg-3 hover:bg-err/10 hover:text-err"
                  >
                    Hapus
                  </button>
                </div>

                <Field label="Model yang dicakup (awalan, pisah dengan koma)">
                  <input
                    value={r.matches.join(", ")}
                    onChange={(e) => patch(i, { matches: e.target.value.split(",").map((s) => s) })}
                    placeholder="claude-"
                    className={`${input} ${
                      r.matches.every((m) => m.trim() === "") ? "border-err" : ""
                    }`}
                  />
                </Field>

                <div className="flex items-end gap-3">
                  {r.mode === "subscription" && (
                    <>
                      <Field label="Biaya per bulan (USD)">
                        <input
                          value={r.monthlyUsd}
                          onChange={(e) => patch(i, { monthlyUsd: e.target.value })}
                          placeholder="200"
                          className={`${input} w-32 ${
                            !isNum(r.monthlyUsd) || Number(r.monthlyUsd) < 0 ? "border-err" : ""
                          }`}
                        />
                      </Field>
                      <Field label="Tanggal perpanjangan (1-28)">
                        <input
                          value={r.renewalDay}
                          onChange={(e) => patch(i, { renewalDay: e.target.value })}
                          placeholder="14"
                          className={`${input} w-32 ${
                            !Number.isInteger(Number(r.renewalDay)) ||
                            Number(r.renewalDay) < 1 ||
                            Number(r.renewalDay) > 28
                              ? "border-err"
                              : ""
                          }`}
                        />
                      </Field>
                    </>
                  )}
                  {r.mode === "prepaid" && (
                    <>
                      <Field label="Saldo diisi (USD)">
                        <input
                          value={r.creditUsd}
                          onChange={(e) => patch(i, { creditUsd: e.target.value })}
                          placeholder="20"
                          className={`${input} w-32 ${
                            !isNum(r.creditUsd) || Number(r.creditUsd) < 0 ? "border-err" : ""
                          }`}
                        />
                      </Field>
                      <Field label="Mulai dipakai (YYYY-MM-DD)">
                        <input
                          value={r.startedOn ?? ""}
                          onChange={(e) => patch(i, { startedOn: e.target.value })}
                          placeholder="2026-03-01"
                          className={`${input} w-40 ${
                            optionalDate(r.startedOn ?? "") ? "" : "border-err"
                          }`}
                        />
                      </Field>
                    </>
                  )}
                  {r.mode !== "payg" && (
                    <Field label="Berakhir (YYYY-MM-DD, opsional)">
                      <input
                        value={r.expiresOn ?? ""}
                        onChange={(e) => patch(i, { expiresOn: e.target.value })}
                        placeholder="2026-04-01"
                        className={`${input} w-40 ${optionalDate(r.expiresOn ?? "") ? "" : "border-err"}`}
                      />
                    </Field>
                  )}
                  {r.mode === "payg" && (
                    <span className="pb-1.5 text-[11.5px] text-fg-3">
                      Ditagih per token. Tidak ada biaya tetap atau saldo yang perlu diisi.
                    </span>
                  )}
                </div>
              </div>
            ))}
            {!allValid && <span className="text-xs text-err">Lengkapi kolom yang ditandai.</span>}
          </div>
        )}

        <div className="flex items-center justify-between border-t border-border pt-4">
          <span className="text-xs text-fg-3">Kosongkan kalau semua model dibayar per token.</span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setRows((prev) => [...(prev ?? []), blank()])}
              className="ad-interactive ad-press cursor-pointer rounded-md border border-border px-3 py-1.75 text-xs font-medium text-fg-2 hover:border-fg-3 hover:text-fg"
            >
              Tambah akun
            </button>
            <button
              type="button"
              disabled={!allValid || save.isPending}
              onClick={() => rows && save.mutate(rows.map(toAccount))}
              className="ad-interactive ad-press cursor-pointer rounded-md bg-busy px-4 py-1.75 text-xs font-semibold text-bg hover:opacity-90 disabled:cursor-default disabled:opacity-45"
            >
              {save.isPending ? "Menyimpan…" : "Simpan"}
            </button>
          </div>
        </div>
        {save.isError && (
          <div className="rounded-md border border-err/40 bg-err/10 px-3 py-2 font-mono text-xs text-err">
            {String(save.error)}
          </div>
        )}
      </div>
    </div>
  );
};
