import assert from "node:assert/strict";
import test from "node:test";
import { captureShortcut, shortcutModifiers, type ShortcutKeyEvent } from "../src/shared/ui/shortcut.ts";

function key(overrides: Partial<ShortcutKeyEvent> = {}): ShortcutKeyEvent {
  return { key: "v", code: "KeyV", ctrlKey: true, altKey: false, shiftKey: false, metaKey: false, ...overrides };
}

test("shortcut capture normalizes supported modifiers and keys for native registration", () => {
  assert.equal(captureShortcut(key()), "Ctrl+V");
  assert.equal(captureShortcut(key({ altKey: true, shiftKey: true })), "Ctrl+Alt+Shift+V");
  assert.equal(captureShortcut(key({ ctrlKey: false, altKey: true, key: "5", code: "Digit5" })), "Alt+5");
  assert.equal(captureShortcut(key({ key: "%", code: "Digit5", shiftKey: true })), "Ctrl+Shift+5");
  assert.equal(captureShortcut(key({ key: "ж", code: "KeyV" })), "Ctrl+V");
  assert.equal(captureShortcut(key({ key: "a", code: "KeyQ" })), "Ctrl+A");
  assert.equal(captureShortcut(key({ key: "F1", code: "F1" })), "Ctrl+F1");
  assert.equal(captureShortcut(key({ key: "F11", code: "F11", shiftKey: true })), "Ctrl+Shift+F11");
  assert.deepEqual(shortcutModifiers(key({ altKey: true, shiftKey: true })), ["Ctrl", "Alt", "Shift"]);
});

test("shortcut capture rejects unsupported, incomplete, repeated and composing keys", () => {
  for (const event of [
    key({ ctrlKey: false }), key({ metaKey: true }), key({ repeat: true }), key({ isComposing: true }),
    key({ getModifierState: (modifier) => modifier === "AltGraph" }),
    ...["Control", "Alt", "Shift", "Meta", "Escape", "Tab", "Enter", " ", "F12", "F24", "Dead", "Process", "Unidentified"].map(
      (value) => key({ key: value, code: value }),
    ),
  ]) assert.equal(captureShortcut(event), null, JSON.stringify(event));
});
