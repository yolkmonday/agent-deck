import type { FeedEvent } from "@/lib/types";

const dot = { waiting: "bg-waiting", busy: "bg-busy", ok: "bg-ok", idle: "bg-idle" } as const;
const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString("id-ID", { hour: "2-digit", minute: "2-digit", hour12: false }).replace(".", ":");

export const ActivityFeed = ({ events }: { events: FeedEvent[] }) => (
  <aside className="flex w-82.5 shrink-0 flex-col gap-3.5 border-l border-border px-5.5 pt-5.5 pb-5.5">
    <div className="flex items-center justify-between">
      <span className="text-sm font-semibold">Aktivitas terbaru</span>
      <span className="font-mono text-[11px] text-ok">real-time</span>
    </div>
    {events.length === 0 && <span className="text-xs text-fg-3">Belum ada perubahan sejak app dibuka.</span>}
    <div className="flex flex-col gap-3.5 overflow-y-auto">
      {events.map((e) => (
        <div key={e.id} className="flex gap-2.5">
          <span className="font-mono text-[11.5px] text-fg-3">{clock(e.timeMs)}</span>
          <span className={`mt-1.25 size-1.75 shrink-0 rounded-full ${dot[e.color]}`} />
          <div className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[12.5px] font-semibold">{e.project}</span>
            <span className="break-words text-xs leading-[1.4] text-fg-2">{e.text}</span>
          </div>
        </div>
      ))}
    </div>
  </aside>
);
