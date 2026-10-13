import { useRef, type ReactNode, type RefObject } from "react";
import { useTranslation } from "react-i18next";

import { SettingsDialog } from "../../settings/components/SettingsDialog";

// In-app replacement for the native window.confirm/window.alert prompts.
export function ConfirmDialog({
  title,
  message,
  confirmLabel,
  busy = false,
  busyLabel,
  icon,
  returnFocusRef,
  onClose,
  onConfirm,
}: {
  title: string;
  message: ReactNode;
  confirmLabel?: string;
  busy?: boolean;
  busyLabel?: string;
  icon?: ReactNode;
  returnFocusRef?: RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onConfirm?: () => void | Promise<void>;
}) {
  const { t } = useTranslation();
  // Dialogs open from a click, so the focused element is the trigger button by default.
  const capturedFocusRef = useRef<HTMLButtonElement | null>(
    document.activeElement instanceof HTMLButtonElement ? document.activeElement : null,
  );
  const confirmText = confirmLabel ?? t("common.understood");
  return (
    <SettingsDialog
      label={title}
      saving={busy}
      returnFocusRef={returnFocusRef ?? capturedFocusRef}
      className="confirm-dialog"
      onClose={onClose}
    >
      <div className="api-profile-editor">
        <div className="api-profile-editor-heading"><strong>{title}</strong></div>
        <div className="api-profile-editor-content">
          <p className="confirm-dialog-description">{message}</p>
        </div>
        <div className="api-profile-editor-actions">
          <div />
          <div className="settings-inline-actions">
            {onConfirm ? (
              <>
                <button className="secondary-button" type="button" autoFocus disabled={busy} onClick={onClose}>
                  {t("common.cancel")}
                </button>
                <button
                  className="secondary-button api-danger-button"
                  type="button"
                  disabled={busy}
                  onClick={() => void onConfirm()}
                >
                  {icon}
                  {busy ? busyLabel ?? t("common.loading") : confirmText}
                </button>
              </>
            ) : (
              <button className="primary-button" type="button" autoFocus onClick={onClose}>
                {confirmText}
              </button>
            )}
          </div>
        </div>
      </div>
    </SettingsDialog>
  );
}
