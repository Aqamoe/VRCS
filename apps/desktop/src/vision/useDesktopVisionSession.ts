import { useEffect, useState } from "react";
import { getDesktopVisionStatus, listenDesktopVisionStatus } from "./api";
import { applyDesktopVisionSession, type DesktopVisionSession, type DesktopVisionStatus } from "./status";

export function useDesktopVisionSession() {
  const [session, setSession] = useState<DesktopVisionSession>({ status: null, draft: "" });
  const [statusError, setStatusError] = useState(false);
  useEffect(() => {
    let disposed = false;
    let unlisten: () => void = () => undefined;
    const receive = (next: DesktopVisionStatus) => {
      if (!disposed) {
        setSession((current) => applyDesktopVisionSession(current, next));
        setStatusError(false);
      }
    };
    const load = async () => {
      const stop = await listenDesktopVisionStatus(receive);
      if (disposed) { stop(); return; }
      unlisten = stop;
      receive(await getDesktopVisionStatus());
    };
    void load().catch(() => { if (!disposed) setStatusError(true); });
    return () => { disposed = true; unlisten(); };
  }, []);
  return {
    ...session, statusError,
    setDraft: (draft: string) => setSession((current) => ({ ...current, draft })),
  };
}
