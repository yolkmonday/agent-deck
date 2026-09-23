import { useEffect, useState } from "react";
import { useT } from "@/i18n";
import { currentVersion } from "@/lib/updater";
import { useUpdater } from "@/store/updater";

export const UpdateSection = () => {
  const t = useT();
  const { status, version, error, manual, check, install } = useUpdater();
  const [current, setCurrent] = useState<string | null>(null);

  useEffect(() => {
    currentVersion()
      .then(setCurrent)
      .catch(() => undefined);
  }, []);

  const result =
    status === "checking"
      ? t("update.checking")
      : status === "none" && manual
        ? t("update.upToDate")
        : status === "error" && error
          ? t("update.failed", { error })
          : null;

  return (
    <div className="flex flex-col gap-1.5 border-t border-border pt-2 text-[12.5px] text-fg-2">
      <div className="flex items-center justify-between gap-2 px-1.5">
        <span className="font-mono text-fg-3">{current ? t("update.version", { version: current }) : ""}</span>
        <button
          type="button"
          disabled={status === "checking" || status === "downloading"}
          onClick={() => void check(true)}
          className="ad-interactive rounded-md border border-border px-2 py-1 text-[12px] hover:bg-surface-2 disabled:opacity-50"
        >
          {t("update.check")}
        </button>
      </div>
      {status === "available" && version && (
        <div className="flex items-center justify-between gap-2 px-1.5">
          <span>{t("update.available", { version })}</span>
          <button
            type="button"
            onClick={() => void install()}
            className="ad-interactive rounded-md bg-fg px-2 py-1 text-[12px] font-medium text-bg"
          >
            {t("update.install")}
          </button>
        </div>
      )}
      {result && <span className="px-1.5 text-[11.5px] text-fg-3">{result}</span>}
    </div>
  );
};
