import { reportFrontendError } from "./diagnostics.ts";

// Tauri replaces window.confirm with an asynchronous native dialog.
export async function confirmAction(message: string): Promise<boolean> {
  try {
    return await window.confirm(message) === true;
  } catch {
    void reportFrontendError({
      kind: "window_error",
      operation: "confirm_action",
      message: "Confirmation dialog failed; action cancelled",
    });
    return false;
  }
}
