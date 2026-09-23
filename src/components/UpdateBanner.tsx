import { Download } from "lucide-react";
import { useT } from "@/i18n";
import { bannerVisible, useUpdater } from "@/store/updater";

export const UpdateBanner = () => {
  const t = useT();
  const state = useUpdater();
  if (!bannerVisible(state)) return null;
  const downloading = state.status === "downloading";
  return (
    <div className="flex items-center gap-3 border-b border-border bg-surface px-4 py-1.5 text-[12.5px] text-fg-2">
      <Download size={14} className="text-fg-3" />
      {downloading ? (
        <>
          <span>{t("update.downloading", { percent: Math.round(state.progress * 100) })}</span>
          <div className="h-1 w-40 overflow-hidden rounded-full bg-surface-2">
            <div className="h-full bg-fg-2 transition-[width]" style={{ width: `${state.progress * 100}%` }} />
          </div>
        </>
      ) : (
        <>
          <span className="flex-1">{t("update.available", { version: state.version ?? "" })}</span>
          <button
            type="button"
            onClick={() => void state.install()}
            className="ad-interactive rounded-md bg-fg px-2.5 py-1 text-[12px] font-medium text-bg"
          >
            {t("update.install")}
          </button>
          <button
            type="button"
            onClick={state.dismiss}
            className="ad-interactive rounded-md px-2 py-1 text-[12px] text-fg-3 hover:bg-surface-2"
          >
            {t("update.later")}
          </button>
        </>
      )}
    </div>
  );
};
