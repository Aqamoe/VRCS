export interface DesktopVisionStatus {
  available: boolean;
  state: "disabled" | "idle" | "capturing" | "selecting" | "editing" | "recording" | "finalizing_asr" | "requesting" | "complete" | "error";
  session_id: number;
  revision: number;
  image_id: number | null;
  question: string;
  answer: string;
  error: string | null;
  error_detail: string | null;
  shortcut_error: string | null;
  shortcut: string;
  profile_name: string;
  model: string;
}

export function applyDesktopVisionStatus(current: DesktopVisionStatus | null, next: DesktopVisionStatus): DesktopVisionStatus {
  if (current && (next.session_id < current.session_id ||
    (next.session_id === current.session_id && next.revision <= current.revision))) return current;
  return next;
}

export interface DesktopVisionSession {
  status: DesktopVisionStatus | null;
  draft: string;
}

export function applyDesktopVisionSession(current: DesktopVisionSession, next: DesktopVisionStatus): DesktopVisionSession {
  if (applyDesktopVisionStatus(current.status, next) === current.status) return current;
  let draft = current.draft;
  if (next.state === "disabled" || (next.image_id === null && next.state === "idle")) draft = "";
  else if (!current.status) draft = next.question;
  else if (next.question && (
    (next.question !== current.status.question && (next.state === "recording" || next.state === "finalizing_asr")) ||
    (next.state === "editing" && next.session_id === current.status.session_id &&
      (current.status.state === "recording" || current.status.state === "finalizing_asr")))) {
    // Native question is the last recognition result, not the editable text draft.
    draft = next.question;
  }
  return { status: next, draft };
}

export function desktopVisionBusy(status: DesktopVisionStatus | null): boolean {
  return Boolean(status && ["capturing", "selecting", "recording", "finalizing_asr", "requesting"].includes(status.state));
}

export const DESKTOP_VISION_QUESTION_LIMIT = 4096;

export function canAskDesktopVision(status: DesktopVisionStatus | null, draft: string): boolean {
  const question = draft.trim();
  return Boolean(status?.available && status.state !== "disabled" && status.image_id !== null &&
    !desktopVisionBusy(status) && question && Array.from(question).length <= DESKTOP_VISION_QUESTION_LIMIT);
}

export function desktopVisionSendKey(event: { key: string; shiftKey: boolean; isComposing: boolean; keyCode: number }): boolean {
  return event.key === "Enter" && !event.shiftKey && !event.isComposing && event.keyCode !== 229;
}
