import { useEffect, useRef, useState } from "react";
import { ChevronDown, Eye, FlaskConical, Hand, Monitor, RefreshCw } from "lucide-react";
import { request } from "../../core-client/transport";
import { useTranslation } from "react-i18next";
import type { ApiProfileView } from "../../providers/types";
import type { VrOcrWristSettings } from "../../integrations/types";
import { TRANSLATION_LANGUAGE_CODES } from "../../translation-languages";
import { DropdownField } from "../../shared/ui/DropdownField";
import { LanguagePicker } from "../../shared/ui/LanguagePicker";
import type { VisionSettings } from "../types";
import { PreferenceToggle, RangeField, Select } from "../SettingsControls";
import { useTranslationProfileModels } from "../hooks/useTranslationProfileModels";
import { DEFAULT_OCR_WRIST_SETTINGS, VR_OVERLAY_POSITION_RANGES } from "../vr-overlay-settings";
import { supportsVision, VISION_LIMITS } from "../vision-settings";
import { formatBytes } from "../settings-derived";
import { openDesktopVision } from "../../vision/api";
import { useDesktopVisionSession } from "../../vision/useDesktopVisionSession";

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
  const { status: desktopStatus, statusError } = useDesktopVisionSession();
  const [shortcut, setShortcut] = useState(config.desktop_shortcut);
  const [openError, setOpenError] = useState(false);
  useEffect(() => setShortcut(config.desktop_shortcut), [config.desktop_shortcut]);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState("");
  const [testError, setTestError] = useState("");
  const testController = useRef<AbortController | null>(null);
  useEffect(() => {
    const cancelTest = () => {
      const controller = testController.current;
      testController.current = null;
      controller?.abort();
    };
    cancelTest();
    setTesting(false);
    setTestResult("");
    setTestError("");
    return cancelTest;
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
      if (testController.current === controller && !controller.signal.aborted) setTestResult(result.answer);
    } catch (error) {
      if (testController.current === controller && !controller.signal.aborted) setTestError(error instanceof Error ? error.message : t("settings.vision.testFailed"));
    } finally {
      if (testController.current === controller) {
        testController.current = null;
        setTesting(false);
      }
    }
  };
  const compatibleProfiles = profiles.filter(supportsVision);
  const selected = compatibleProfiles.find((profile) => profile.id === config.profile_id);
  const { models, loading, error, refresh } = useTranslationProfileModels(selected, Boolean(selected));
  const modelOptions = [
    { value: "", label: t("settings.vision.selectModel") },
    ...(config.model && !models.includes(config.model) ? [{
      value: config.model,
      label: config.model,
      description: t("settings.vision.savedModelUnavailable"),
    }] : []),
    ...models.map((model) => ({ value: model, label: model })),
  ];
  const wrist = config.wrist ?? DEFAULT_OCR_WRIST_SETTINGS;
  const updateWrist = (patch: Partial<VrOcrWristSettings>) => onChange({ wrist: { ...wrist, ...patch } });
  return <section className="vr-overlay-card vision-settings-card" id="vision-question-answering" aria-labelledby="vision-heading">
    <header className="vr-overlay-card-heading">
      <div><Eye size={18} /><span>
        <strong id="vision-heading">{t("settings.vision.title")}</strong>
        <small>{t("settings.vision.description")}</small>
      </span></div>
    </header>
    <div className="settings-toggle-list vr-overlay-card-toggles">
      <PreferenceToggle title={t("settings.vision.enable")} checked={config.enabled}
        description={t("settings.vision.commonDescription")}
        disabled={disabled || (!config.enabled && (!selected || !config.model.trim()))}
        onChange={(enabled) => onChange({ enabled })} />
    </div>
    <div className="vr-overlay-field-grid">
      <Select label={t("settings.vision.profile")} value={config.profile_id ?? ""} disabled={disabled}
        helper={!compatibleProfiles.length ? t("settings.vision.noProfiles") : undefined}
        options={[
          { value: "", label: t("settings.vision.noProfile") },
          ...compatibleProfiles.map((profile) => ({ value: profile.id, label: profile.name === profile.provider_display_name
            ? profile.name : `${profile.name} · ${profile.provider_display_name}` })),
          ...(config.profile_id && !selected ? [{ value: config.profile_id, label: t("settings.vision.unavailableProfile") }] : []),
        ]}
        onChange={(value) => onSelectProfile(value || null)} />
      <div className="field">
        <span>{t("settings.vision.model")}</span>
        <div className="translation-model-input-row">
          <DropdownField label={t("settings.vision.model")} value={config.model} options={modelOptions}
            searchable searchPlaceholder={t("settings.vision.selectModel")}
            emptyLabel={t("settings.vision.noModels")}
            disabled={disabled || !selected || loading || !models.length}
            onChange={(model) => onChange({ model, ...(!model ? { enabled: false } : {}) })} />
          <button className="secondary-button" type="button" aria-label={t("settings.apiManagement.refreshModels")}
            title={t("settings.apiManagement.refreshModels")} disabled={disabled || !selected || loading}
            onClick={() => void refresh()}>
            <RefreshCw size={15} className={loading ? "spin" : undefined} />
          </button>
        </div>
        {loading && <small>{t("settings.apiManagement.loadingModels")}</small>}
        {error && <small className="api-model-catalog-error" role="alert">{error}</small>}
        {selected && !loading && !error && !models.length && <small>{t("settings.vision.noModels")}</small>}
        {selected && !loading && !error && config.model && !models.includes(config.model)
          && <small>{t("settings.vision.savedModelUnavailable")}</small>}

      </div>
      <LanguagePicker label={t("settings.vision.answerLanguage")} value={config.answer_language}
        languageCodes={TRANSLATION_LANGUAGE_CODES} disabled={disabled}
        onChange={(answer_language) => onChange({ answer_language })} />

    </div>
    <details className="vision-settings-details">
      <summary><span>{t("settings.vision.systemPrompt")}</span><ChevronDown size={16} /></summary>
      <label className="field translation-prompt-field">
        <textarea key={config.system_prompt} aria-label={t("settings.vision.systemPrompt")}
          defaultValue={config.system_prompt} rows={4} maxLength={20000} disabled={disabled}
          onBlur={(event) => { const system_prompt = event.target.value; if (system_prompt.trim() && system_prompt !== config.system_prompt) onChange({ system_prompt }); else event.target.value = config.system_prompt; }} />
      </label>
    </details>
    <details className="vision-settings-details">
      <summary><span>{t("settings.vision.limits")}</span><ChevronDown size={16} /></summary>
      <div className="vr-overlay-range-grid">
        {Object.entries(VISION_LIMITS).filter(([field]) => field !== "display_seconds").map(([field, range]) => <RangeField key={field}
          label={t(`settings.vision.${field}`)} value={config[field as keyof typeof VISION_LIMITS]} {...range}
          disabled={disabled} formatValue={(value) => field === "image_max_bytes" ? formatBytes(value, locale)
            : field === "asr_finalize_timeout_ms" ? `${value} ms` : field.endsWith("seconds") ? `${value} s` : String(value)}
          onCommit={(value) => onChange({ [field]: value })} />)}
      </div>
    </details>
    <details className="vision-settings-details">
      <summary><span>{t("settings.vision.imageTest")}</span><ChevronDown size={16} /></summary>
      <p className="vision-settings-note">{t("settings.vision.testDescription")}</p>
      <div className="vr-overlay-card-actions">
        <button type="button" className="secondary-button" disabled={disabled || testing || !selected || !config.model.trim()}
          onClick={() => void testImage()}>
          {testing ? <RefreshCw size={15} className="spin" /> : <FlaskConical size={15} />}
          {t(testing ? "settings.vision.testing" : "settings.vision.testButton")}
        </button>
      </div>
      {testResult && <p className="vision-settings-note vision-settings-result" role="status">{testResult}</p>}
      {testError && <p className="vr-overlay-native-error" role="alert">{testError}</p>}
    </details>
    <details className="vision-settings-details">
      <summary><span>{t("settings.vision.usage")}</span><ChevronDown size={16} /></summary>
      <p className="vision-settings-note">{t("settings.vision.desktopDescription")}</p>
      <p className="vision-settings-note">{t("settings.vision.privacy")}</p>
    </details>
    <section className="vision-settings-group" aria-labelledby="vision-desktop-heading">
      <header className="vr-overlay-card-heading">
        <div><Monitor size={18} /><span><strong id="vision-desktop-heading">{t("settings.vision.desktopTitle")}</strong></span></div>
      </header>
      <PreferenceToggle title={t("settings.vision.desktopEnable")} checked={config.desktop_enabled}
        description={t("settings.vision.desktopDescription")} disabled={disabled || !config.enabled}
        onChange={(desktop_enabled) => onChange({ desktop_enabled })} />
      <label className="field">
        <span>{t("settings.vision.desktopShortcut")}</span>
        <input type="text" value={shortcut} disabled={disabled} placeholder="Ctrl+Alt+V"
          onChange={(event) => setShortcut(event.target.value)}
          onBlur={() => {
            const value = shortcut.trim();
            if (value && value !== config.desktop_shortcut) onChange({ desktop_shortcut: value });
            else setShortcut(config.desktop_shortcut);
          }} />
        <small>{t("settings.vision.desktopShortcutHint")}</small>
      </label>
      <p className="vision-settings-note">{t("visionWindow.captureHint", { shortcut: config.desktop_shortcut })}</p>
      {desktopStatus?.shortcut_error && <p className="vr-overlay-native-error" role="alert" title={desktopStatus.shortcut_error}>
        {t(`visionWindow.errors.${desktopStatus.shortcut_error.replace("desktop_vision.", "")}`, { defaultValue: t("visionWindow.errors.shortcutFailed") })}
      </p>}
      {config.enabled && config.desktop_enabled && desktopStatus?.available === false && <p className="vision-settings-note">{t("visionWindow.errors.unavailable")}</p>}
      {(statusError || openError) && <p className="vr-overlay-native-error" role="alert">{t(statusError ? "visionWindow.errors.statusFailed" : "visionWindow.errors.openFailed")}</p>}
      <div className="vr-overlay-card-actions">
        <button className="secondary-button" type="button" disabled={disabled || !config.enabled || !config.desktop_enabled}
          onClick={() => { setOpenError(false); void openDesktopVision().catch(() => setOpenError(true)); }}>
          <Eye size={15} />{t("settings.vision.openDesktop")}
        </button>
      </div>
    </section>
    <section className="vision-settings-group" aria-labelledby="vision-wrist-heading">
      <header className="vr-overlay-card-heading">
        <div><Hand size={18} /><span>
          <strong id="vision-wrist-heading">{t("settings.vision.wristTitle")}</strong>
          <small>{t("settings.vision.wristDescription")}</small>
        </span></div>
      </header>
      <PreferenceToggle title={t("settings.vision.vrEnable")} checked={config.vr_enabled}
        description={t(vrAvailable ? "settings.vision.controls" : "settings.vision.vrUnavailable")}
        disabled={disabled || !config.enabled || !vrAvailable} onChange={(vr_enabled) => onChange({ vr_enabled })} />
      <p className="vision-settings-note">{t("settings.vision.asrSupport")}</p>
      <RangeField label={t("settings.vision.display_seconds")} value={config.display_seconds}
        {...VISION_LIMITS.display_seconds} disabled={disabled || !vrAvailable} formatValue={(value) => `${value}s`}
        onCommit={(display_seconds) => onChange({ display_seconds })} />
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
      <details className="vision-settings-details">
        <summary><span>{t("settings.vrOcr.wristPosition")}</span><ChevronDown size={16} /></summary>
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
  </section>;
}
