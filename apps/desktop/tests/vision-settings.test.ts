import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_VISION_SETTINGS, patchVision, selectVisionProfile, supportsVision, VISION_LIMITS } from "../src/settings/vision-settings.ts";
import { DEFAULT_OCR_SETTINGS, DEFAULT_OCR_WRIST_SETTINGS } from "../src/settings/vr-overlay-settings.ts";
import { DEFAULT_FEATURE_SETTINGS } from "../src/settings/feature-availability.ts";
import { applyVrDashboardAction } from "../src/vr-dashboard/settings-actions.ts";
import type { Settings } from "../src/settings/types.ts";
import type { ApiProfile, ApiProfileView } from "../src/providers/types.ts";

const settings = {
  schema_version: 31,
  features: { ...DEFAULT_FEATURE_SETTINGS },
  ocr: { ...DEFAULT_OCR_SETTINGS },
  vision: { ...DEFAULT_VISION_SETTINGS },
} as unknown as Settings;

function profile(provider: string, capability: string = "image_understanding"): ApiProfile {
  return { id: "images", name: "Images", provider, enabled_capabilities: [capability] } as ApiProfile;
}

test("vision defaults are explicit, flat and opt-in", () => {
  assert.equal(DEFAULT_VISION_SETTINGS.enabled, false);
  assert.equal(DEFAULT_VISION_SETTINGS.profile_id, null);
  assert.equal(DEFAULT_VISION_SETTINGS.model, "");
  assert.equal(DEFAULT_VISION_SETTINGS.answer_language, "zh-Hans");
  assert.equal(DEFAULT_VISION_SETTINGS.max_capture_seconds, 30);
  assert.equal(DEFAULT_VISION_SETTINGS.asr_finalize_timeout_ms, 10000);
  assert.equal(DEFAULT_VISION_SETTINGS.request_timeout_seconds, 60);
  assert.equal(DEFAULT_VISION_SETTINGS.max_output_tokens, 1024);
  assert.equal(DEFAULT_VISION_SETTINGS.image_max_edge, 1536);
  assert.equal(DEFAULT_VISION_SETTINGS.image_max_bytes, 4194304);
  assert.equal(DEFAULT_VISION_SETTINGS.display_seconds, 30);
  assert.deepEqual(DEFAULT_VISION_SETTINGS.wrist, DEFAULT_OCR_WRIST_SETTINGS);
  assert.notEqual(DEFAULT_VISION_SETTINGS.wrist, DEFAULT_OCR_SETTINGS.wrist);
  for (const [field, { min, max }] of Object.entries(VISION_LIMITS)) {
    const value = DEFAULT_VISION_SETTINGS[field as keyof typeof VISION_LIMITS];
    assert.ok(value >= min && value <= max, field);
  }
});

test("vision profiles need explicit image capability and an allowed protocol", () => {
  for (const provider of ["openai_compatible", "openrouter", "lm_studio", "ollama"]) {
    assert.equal(supportsVision(profile(provider)), true);
    assert.equal(supportsVision(profile(provider, "text_generation")), false);
    assert.equal(supportsVision(profile(provider, "text_translation")), false);
  }
  assert.equal(supportsVision(profile("openai")), false);
  assert.equal(supportsVision(profile("gemini")), false);
});

test("vision edits preserve OCR and profile changes do not reuse an unverified model", () => {
  const edited = patchVision(settings, { answer_language: "ja", model: "image-model" });
  assert.equal(edited.ocr, settings.ocr);
  assert.equal(settings.vision.model, "");
  const enabled = patchVision(edited, { enabled: true, profile_id: "old" });
  const selected = selectVisionProfile(enabled, "images");
  assert.equal(selected.vision.profile_id, "images");
  assert.equal(selected.vision.model, "");
  assert.equal(selected.vision.enabled, false);
  assert.equal(selected.vision.answer_language, "ja");
  assert.equal(selectVisionProfile(selected, "images"), selected);
});

test("dashboard vision toggle is independent of OCR and rejects unconfigured activation", () => {
  const profiles = [profile("openrouter")] as ApiProfileView[];
  assert.equal(applyVrDashboardAction(settings, "toggle_vision", profiles), settings);
  const available = { ...settings, features: { ...settings.features, visual_question_answering: true, ocr: false } };
  assert.equal(applyVrDashboardAction(available, "toggle_vision", profiles), available);
  const configured = patchVision(available, { profile_id: "images", model: "image-model" });
  const enabled = applyVrDashboardAction(configured, "toggle_vision", profiles);
  assert.equal(enabled.vision.enabled, true);
  assert.equal(enabled.ocr, settings.ocr);
  assert.equal(applyVrDashboardAction(enabled, "toggle_vision", profiles).vision.enabled, false);
  const noVr = { ...configured, features: { ...configured.features, vr_overlay: false } };
  assert.equal(applyVrDashboardAction(noVr, "toggle_vision", profiles), noVr);
});
