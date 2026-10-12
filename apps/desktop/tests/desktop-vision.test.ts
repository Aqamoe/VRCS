import assert from "node:assert/strict";
import test from "node:test";
import {
  applyDesktopVisionSession, applyDesktopVisionStatus, canAskDesktopVision,
  desktopVisionBusy, desktopVisionSendKey, type DesktopVisionStatus,
} from "../src/vision/status.ts";

const status: DesktopVisionStatus = {
  available: true, state: "editing", session_id: 3, revision: 4, image_id: 7,
  question: "Last recognition", answer: "", error: null, error_detail: null,
  shortcut_error: null, shortcut: "Ctrl+Alt+V", profile_name: "Images", model: "image-model",
};

test("vision snapshots cannot replace newer session or revision events", () => {
  assert.equal(applyDesktopVisionStatus(status, { ...status, session_id: 2, revision: 100 }), status);
  assert.equal(applyDesktopVisionStatus(status, { ...status, revision: 3 }), status);
  assert.equal(applyDesktopVisionStatus(status, { ...status, state: "error" }), status);
  const next = { ...status, revision: 5 };
  assert.equal(applyDesktopVisionStatus(status, next), next);
  const session = { ...status, session_id: 4, revision: 0 };
  assert.equal(applyDesktopVisionStatus(status, session), session);
  assert.equal(applyDesktopVisionStatus(null, status), status);
});

test("typed drafts survive unrelated updates, ask echoes, failures and cancelled reselection", () => {
  const current = { status, draft: "My edited question\nwith two lines" };
  for (const patch of [
    { state: "requesting" as const, question: "An echoed question" },
    { state: "error" as const, question: "", error: "vision.request_failed" },
    { state: "complete" as const, answer: "Answer" },
    { shortcut_error: "desktop_vision.shortcut_conflict" },
    { session_id: 4, revision: 0, state: "selecting" as const },
  ]) {
    const next = applyDesktopVisionSession(current, { ...status, revision: 5, ...patch });
    assert.equal(next.draft, current.draft);
    assert.equal(next.status?.image_id, status.image_id);
  }
  const selecting = applyDesktopVisionSession(current, { ...status, revision: 5, state: "selecting" });
  assert.equal(applyDesktopVisionSession(selecting, { ...status, session_id: 4, revision: 0 }).draft, current.draft);
  assert.equal(applyDesktopVisionSession(current, { ...status, revision: 3 }), current);
});

test("only new recognized speech updates a draft, and empty or failed ASR keeps it", () => {
  const recording = { status: { ...status, state: "recording" as const }, draft: "Typed fallback" };
  const partial = applyDesktopVisionSession(recording, { ...status, revision: 5, state: "recording", question: "New speech" });
  assert.equal(partial.draft, "New speech");
  const finalizing = applyDesktopVisionSession(partial, { ...status, revision: 6, state: "finalizing_asr", question: "New speech" });
  const final = applyDesktopVisionSession(finalizing, { ...status, revision: 7, question: "New speech finalized" });
  assert.equal(final.draft, "New speech finalized");
  assert.equal(applyDesktopVisionSession(recording, { ...status, revision: 5 }).draft, status.question,
    "a new recording can recognize the same text as the previous recording");
  assert.equal(applyDesktopVisionSession(recording, { ...status, session_id: 4, revision: 0 }).draft, "Typed fallback",
    "cancellation invalidates the recording session without restoring old recognized text");
  assert.equal(applyDesktopVisionSession(recording, { ...status, revision: 5, question: "" }).draft, "Typed fallback");
  assert.equal(applyDesktopVisionSession(recording, { ...status, revision: 5, state: "error", question: "" }).draft, "Typed fallback");
  assert.equal(applyDesktopVisionSession(final, { ...status, revision: 8, model: "other-model", question: final.status!.question }).draft, final.draft);
});

test("disabling or clearing the session clears sensitive frontend drafts", () => {
  const current = { status, draft: "Private question" };
  assert.equal(applyDesktopVisionSession(current, { ...status, revision: 5, state: "disabled", image_id: null }).draft, "");
  assert.equal(applyDesktopVisionSession(current, { ...status, revision: 5, state: "idle", image_id: null }).draft, "");
  assert.equal(applyDesktopVisionSession({ status: null, draft: "" }, status).draft, status.question);
});

test("sending requires a current image, a nonempty draft, availability and an idle operation", () => {
  assert.equal(canAskDesktopVision(status, "  Question  "), true);
  assert.equal(canAskDesktopVision({ ...status, image_id: 0 }, "Question"), true);
  assert.equal(canAskDesktopVision(null, "Question"), false);
  assert.equal(canAskDesktopVision(status, " \n "), false);
  assert.equal(canAskDesktopVision({ ...status, image_id: null }, "Question"), false);
  assert.equal(canAskDesktopVision({ ...status, available: false }, "Question"), false);
  assert.equal(canAskDesktopVision({ ...status, state: "disabled" }, "Question"), false);
  for (const state of ["capturing", "selecting", "recording", "finalizing_asr", "requesting"] as const) {
    assert.equal(desktopVisionBusy({ ...status, state }), true, state);
    assert.equal(canAskDesktopVision({ ...status, state }, "Question"), false, state);
  }
  assert.equal(canAskDesktopVision({ ...status, state: "error" }, "Retry"), true);
});

test("question limits count trimmed Unicode scalars instead of UTF-16 units", () => {
  for (const character of ["a", "你", "😀"]) {
    const question = character.repeat(4096);
    assert.equal(canAskDesktopVision(status, question), true, character);
    assert.equal(canAskDesktopVision(status, ` \n${question}\t `), true, "surrounding whitespace is trimmed");
    assert.equal(canAskDesktopVision(status, question + character), false, character);
  }
  assert.equal(canAskDesktopVision(status, "e\u0301".repeat(2048)), true);
  assert.equal(canAskDesktopVision(status, "e\u0301".repeat(2048) + "a"), false, "combining marks count as scalars");
});

test("Enter sends, Shift+Enter is a newline, and IME composition cannot send", () => {
  const event = { key: "Enter", shiftKey: false, isComposing: false, keyCode: 13 };
  assert.equal(desktopVisionSendKey(event), true);
  assert.equal(desktopVisionSendKey({ ...event, shiftKey: true }), false);
  assert.equal(desktopVisionSendKey({ ...event, isComposing: true }), false);
  assert.equal(desktopVisionSendKey({ ...event, keyCode: 229 }), false);
  assert.equal(desktopVisionSendKey({ ...event, key: "Escape" }), false);
});
