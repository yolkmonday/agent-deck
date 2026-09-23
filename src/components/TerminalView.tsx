import { useEffect, useRef } from "react";
import type { TermSession } from "@/lib/api";
import { fitInstance, getOrCreate, refreshInstance } from "@/lib/terminal-instances";

export const TerminalView = ({ session, visible }: { session: TermSession; visible: boolean }) => {
  const hostRef = useRef<HTMLDivElement | null>(null);

  // Mount: attach the session's persistent terminal element into this view's
  // host and fit it. Unmount only detaches the element (it is owned by
  // terminal-instances, not this component) so switching tabs or leaving and
  // returning to the Terminal page reuses the same live terminal instead of
  // recreating one and replaying scrollback text into it.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const instance = getOrCreate(session.id);
    host.appendChild(instance.element);
    fitInstance(session.id);
    instance.term.focus();

    const observer = new ResizeObserver(() => fitInstance(session.id));
    observer.observe(host);

    return () => {
      observer.disconnect();
      if (instance.element.parentNode === host) {
        host.removeChild(instance.element);
      }
    };
  }, [session.id]);

  // A write that landed while this tab was hidden (`display: none`) is not
  // guaranteed to have painted; re-fit and force a repaint now that it can.
  useEffect(() => {
    if (!visible) return;
    fitInstance(session.id);
    refreshInstance(session.id);
  }, [visible, session.id]);

  return <div ref={hostRef} className="h-full w-full overflow-hidden bg-bg p-2.5" />;
};
