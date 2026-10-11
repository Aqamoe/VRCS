import { useEffect, useRef, useState } from "react";
import { Eye, Glasses } from "lucide-react";
import { request } from "../../core-client/transport";
import { useTranslation } from "react-i18next";
import type { ApiProfileView } from "../../providers/types";
import type { VrOcrWristSettings } from "../../integrations/types";
import { localizedLanguageName, TRANSLATION_LANGUAGE_CODES } from "../../translation-languages";
import type { VisionSettings } from "../types";
import { PreferenceToggle, RangeField, Select } from "../SettingsControls";
import { DEFAULT_OCR_WRIST_SETTINGS, VR_OVERLAY_POSITION_RANGES } from "../vr-overlay-settings";
import { supportsVision, VISION_LIMITS } from "../vision-settings";
import { formatBytes } from "../settings-derived";

export function VisionSettingsCard({ config, profiles, vrAvailable, disabled, onChange, onSelectProfile }: {
  config: VisionSettings;
  profiles: ApiProfileView[];
  vrAvailable: boolean;
  disabled: boolean;
  onChange: (patch: Partial<VisionSettings>) => void;
  onSelectProfile: (profileId: string | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? "en-US";
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState("");
  const [testError, setTestError] = useState("");
  const testController = useRef<AbortController | null>(null);
  useEffect(() => {
    testController.current?.abort();
    setTesting(false);
    setTestResult("");
    setTestError("");
    return () => testController.current?.abort();
  }, [config.profile_id, config.model]);
  const testImage = async () => {
    if (testController.current) return;
    const controller = new AbortController();
    testController.current = controller;
    setTesting(true);
    setTestResult("");
    setTestError("");
    try {
      const result = await request<{ answer: string }>("/api/vision/test", {
        method: "POST", signal: controller.signal,
        timeoutMs: config.request_timeout_seconds * 1000 + 2000,
      });
      if (!controller.signal.aborted) setTestResult(result.answer);
    } catch (error) {
      if (!controller.signal.aborted) setTestError(error instanceof Error ? error.message : t("settings.vision.testFailed"));
    } finally {
      if (testController.current === controller) {
        testController.current = null;
        setTesting(false);
      }
    }
  };
  const compatibleProfiles = profiles.filter(supportsVision);
  const selected = compatibleProfiles.find((profile) => profile.id === config.profile_id);
  const wrist = config.wrist ?? DEFAULT_OCR_WRIST_SETTINGS;
  const updateWrist = (patch: Partial<VrOcrWristSettings>) => onChange({ wrist: { ...wrist, ...patch } });
  const languages = [...new Set([config.answer_language, ...TRANSLATION_LANGUAGE_CODES])];
  return <div className="vr-overlay-card-list" id="vision-question-answering">
    <section className="vr-overlay-card" aria-labelledby="vision-heading">
      <header className="vr-overlay-card-heading"><div><Eye size={18} /><span>
        <strong id="vision-heading">{t("settings.vision.title")}</strong>
        <small>{t("settings.vision.description")}</small>
      </span></div></header>
      <PreferenceToggle title={t("settings.vision.enable")} checked={config.enabled}
        description={!vrAvailable ? t("settings.vision.vrUnavailable") : t("settings.vision.controls")}
        disabled={disabled || !vrAvailable || (!config.enabled && (!selected || !config.model.trim()))}
        onChange={(enabled) => onChange({ enabled })} />
      <div className="vr-overlay-field-grid">
        <Select label={t("settings.vision.profile")} value={config.profile_id ?? ""} disabled={disabled}
          helper={!compatibleProfiles.length ? t("settings.vision.noProfiles") : t("settings.vision.imageRequired")}
          options={[
            { value: "", label: t("settings.vision.noProfile") },
            ...compatibleProfiles.map((profile) => ({ value: profile.id, label: `${profile.name} · ${profile.provider_display_name}` })),
            ...(config.profile_id && !selected ? [{ value: config.profile_id, label: t("settings.vision.unavailableProfile") }] : []),
          ]}
          onChange={(value) => onSelectProfile(value || null)} />
        <label className="field"><span>{t("settings.vision.model")}</span>
          <input key={config.model} defaultValue={config.model} maxLength={256} disabled={disabled || !selected}
            onBlur={(event) => { const model = event.target.value.trim(); if (model !== config.model) onChange({ model, ...(!model ? { enabled: false } : {}) }); }} />
          <small>{t("settings.vision.imageRequired")}</small>
        </label>
        <Select label={t("settings.vision.answerLanguage")} value={config.answer_language} disabled={disabled}
          options={languages.map((value) => ({ value, label: localizedLanguageName(value, locale) }))}
          onChange={(answer_language) => onChange({ answer_language })} />
      </div>
      <label className="field"><span>{t("settings.vision.systemPrompt")}</span>
        <textarea key={config.system_prompt} defaultValue={config.system_prompt} rows={4} maxLength={20000} disabled={disabled}
          onBlur={(event) => { const system_prompt = event.target.value; if (system_prompt.trim() && system_prompt !== config.system_prompt) onChange({ system_prompt }); else event.target.value = config.system_prompt; }} />
      </label>
      <p className="settings-help">{t("settings.vision.privacy")}</p>
      <p className="settings-help">{t("settings.vision.asrSupport")}</p>
      <details className="vr-overlay-range-group"><summary>{t("settings.vision.limits")}</summary>
        <div className="vr-overlay-range-grid">
          {Object.entries(VISION_LIMITS).map(([field, range]) => <RangeField key={field}
            label={t(`settings.vision.${field}`)} value={config[field as keyof typeof VISION_LIMITS]} {...range}
            disabled={disabled} formatValue={(value) => field === "image_max_bytes" ? formatBytes(value, locale)
              : field === "asr_finalize_timeout_ms" ? `${value} ms` : field.endsWith("seconds") ? `${value} s` : String(value)}
            onCommit={(value) => onChange({ [field]: value })} />)}
        </div>
      </details>
      <details className="vr-overlay-range-group"><summary>{t("settings.vision.imageTest")}</summary>
        <p className="settings-help">{t("settings.vision.testDescription")}</p>
        <button type="button" className="secondary" disabled={disabled || testing || !selected || !config.model.trim()}
          onClick={() => void testImage()}>{t(testing ? "settings.vision.testing" : "settings.vision.testButton")}</button>
        {testResult && <p className="settings-help" role="status">{testResult}</p>}
        {testError && <p className="field-error" role="alert">{testError}</p>}
      </details>
    </section>
    <section className="vr-overlay-card" aria-labelledby="vision-wrist-heading">
      <header className="vr-overlay-card-heading"><div><Glasses size={18} /><span>
        <strong id="vision-wrist-heading">{t("settings.vision.wristTitle")}</strong>
        <small>{t("settings.vision.wristDescription")}</small>
      </span></div></header>
      <div className="vr-overlay-field-grid">
        <Select label={t("settings.vrOverlay.hand")} value={wrist.hand} disabled={disabled}
          options={["left", "right", "dominant"].map((value) => ({ value, label: t(`settings.vrOverlay.hands.${value}`) }))}
          onChange={(hand) => updateWrist({ hand: hand as VrOcrWristSettings["hand"] })} />
        <Select label={t("settings.vrOverlay.dominantHand")} value={wrist.dominant_hand} disabled={disabled || wrist.hand !== "dominant"}
          options={["left", "right"].map((value) => ({ value, label: t(`settings.vrOverlay.hands.${value}`) }))}
          onChange={(dominant_hand) => updateWrist({ dominant_hand: dominant_hand as VrOcrWristSettings["dominant_hand"] })} />
        <RangeField label={t("settings.vrOverlay.fontSize")} value={wrist.font_size_px} min={18} max={72} step={1}
          disabled={disabled} formatValue={(value) => `${value}px`} onCommit={(font_size_px) => updateWrist({ font_size_px })} />
        <RangeField label={t("settings.vrOverlay.width")} value={wrist.width_m} min={0.1} max={1} step={0.01}
          disabled={disabled} formatValue={(value) => `${value.toFixed(2)}m`} onCommit={(width_m) => updateWrist({ width_m })} />
        <RangeField label={t("settings.vrOverlay.opacity")} value={wrist.opacity} min={0.1} max={1} step={0.05}
          disabled={disabled} formatValue={(value) => `${Math.round(value * 100)}%`} onCommit={(opacity) => updateWrist({ opacity })} />
      </div>
      <details className="vr-overlay-range-group"><summary>{t("settings.vrOcr.wristPosition")}</summary>
        <div className="vr-overlay-range-grid">
          {([
            ["offset_x_m", "horizontal"], ["offset_y_m", "vertical"], ["offset_z_m", "depth"],
            ["pitch_deg", "pitch"], ["yaw_deg", "yaw"], ["roll_deg", "roll"],
          ] as const).map(([field, label]) => <RangeField key={field} label={t(`settings.vrOverlay.${label}`)}
            value={wrist[field]} {...VR_OVERLAY_POSITION_RANGES.wrist[field]} disabled={disabled}
            formatValue={(value) => field.startsWith("offset_") ? `${value.toFixed(2)}m` : `${value}°`}
            onCommit={(value) => updateWrist({ [field]: value })} />)}
        </div>
      </details>
    </section>
  </div>;
}
