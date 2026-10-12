import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DesktopVisionStatus } from "./status";

export async function getDesktopVisionStatus(): Promise<DesktopVisionStatus> {
  if (!isTauri()) return {
    available: false, state: "disabled", session_id: 0, revision: 0, image_id: null,
    question: "", answer: "", error: null, error_detail: null, shortcut_error: null,
    shortcut: "Ctrl+Alt+V", profile_name: "", model: "",
  };
  return invoke("desktop_vision_status");
}

export async function listenDesktopVisionStatus(onStatus: (status: DesktopVisionStatus) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => undefined;
  return listen<DesktopVisionStatus>("desktop-vision-status-changed", (event) => onStatus(event.payload));
}

export async function openDesktopVision(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_open");
}

export async function captureDesktopVision(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_capture");
}

export async function startDesktopVisionRecording(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_start_recording");
}

export async function finishDesktopVisionRecording(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_finish_recording");
}

export async function askDesktopVision(question: string, imageId: number): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_ask", { question, imageId });
}

export async function cancelDesktopVision(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_cancel");
}

export async function closeDesktopVision(): Promise<void> {
  if (isTauri()) await invoke("desktop_vision_close");
}

export async function getDesktopVisionImage(imageId: number): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("desktop_vision_image", { imageId });
}
