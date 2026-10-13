import assert from "node:assert/strict";
import test from "node:test";
import { confirmAction } from "../src/app/confirm-action.ts";

test("confirmation waits for native dialogs and fails closed", async () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "window");
  try {
    for (const result of [true, false]) {
      Object.defineProperty(globalThis, "window", { configurable: true, value: { confirm: () => result } });
      assert.equal(await confirmAction("Delete?"), result);
      Object.defineProperty(globalThis, "window", { configurable: true, value: { confirm: async () => result } });
      assert.equal(await confirmAction("Delete?"), result);
    }
    Object.defineProperty(globalThis, "window", { configurable: true, value: { confirm: async () => { throw new Error("Permission denied"); } } });
    assert.equal(await confirmAction("Delete?"), false);
  } finally {
    if (original) Object.defineProperty(globalThis, "window", original);
    else Reflect.deleteProperty(globalThis, "window");
  }
});
