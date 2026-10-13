import assert from "node:assert/strict";
import test from "node:test";

import {
  applyTheme,
  isThemePreference,
  readThemePreference,
  resolveTheme,
  subscribeThemePreference,
  updateThemePreference,
  writeThemePreference,
} from "../src/app/app-theme.ts";

function installFakeWindow(storage = new Map<string, string>()) {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const handlers = new Map<string, (event: { key: string | null }) => void>();
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      localStorage: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
      },
      addEventListener: (type: string, handler: (event: { key: string | null }) => void) => {
        handlers.set(type, handler);
      },
      removeEventListener: (type: string) => {
        handlers.delete(type);
      },
    },
  });
  return {
    handlers,
    storage,
    restore() {
      if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
      else delete (globalThis as { window?: unknown }).window;
    },
  };
}

test("validates stored theme preferences", () => {
  assert.equal(isThemePreference("system"), true);
  assert.equal(isThemePreference("light"), true);
  assert.equal(isThemePreference("dark"), true);
  assert.equal(isThemePreference("blue"), false);
  assert.equal(isThemePreference(null), false);
});

test("resolves the system preference against the reported system theme", () => {
  assert.equal(resolveTheme("system", true), "dark");
  assert.equal(resolveTheme("system", false), "light");
  assert.equal(resolveTheme("dark", false), "dark");
  assert.equal(resolveTheme("light", true), "light");
});

test("reads and writes the theme preference with a system fallback", () => {
  const fake = installFakeWindow();
  try {
    assert.equal(readThemePreference(), "system");
    fake.storage.set("vrcs.theme", "dark");
    assert.equal(readThemePreference(), "dark");
    fake.storage.set("vrcs.theme", "mauve");
    assert.equal(readThemePreference(), "system");
    writeThemePreference("light");
    assert.equal(fake.storage.get("vrcs.theme"), "light");
  } finally {
    fake.restore();
  }
});

test("falls back to the system preference when storage is unavailable", () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  delete (globalThis as { window?: unknown }).window;
  try {
    assert.equal(readThemePreference(), "system");
    writeThemePreference("dark");
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
  }
});

test("applies a theme to the document root and theme color", () => {
  const attributes = new Map<string, string>();
  const metaAttributes = new Map<string, string>();
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      documentElement: {
        getAttribute: (name: string) => attributes.get(name) ?? null,
        setAttribute: (name: string, value: string) => attributes.set(name, value),
      },
      querySelector: () => ({
        setAttribute: (name: string, value: string) => metaAttributes.set(name, value),
      }),
    },
  });

  try {
    applyTheme("dark");
    assert.equal(attributes.get("data-theme"), "dark");
    assert.equal(metaAttributes.get("content"), "#282c33");
    applyTheme("light");
    assert.equal(attributes.get("data-theme"), "light");
    assert.equal(metaAttributes.get("content"), "#f5f8fb");
    applyTheme("light");
    assert.equal(attributes.get("data-theme"), "light");
  } finally {
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as { document?: unknown }).document;
  }
});

test("commits preference updates inside the cross-fade callback", () => {
  const events: string[] = [];
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      localStorage: {
        getItem: () => null,
        setItem: () => undefined,
      },
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    },
  });
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      documentElement: {
        getAttribute: () => null,
        setAttribute: (_name: string, value: string) => events.push(`attr:${value}`),
      },
      querySelector: () => null,
      startViewTransition: (update: () => void) => {
        events.push("capture");
        update();
        events.push("finish-callback");
        return { finished: Promise.resolve() };
      },
    },
  });

  try {
    const unsubscribe = subscribeThemePreference((preference) => events.push(`notify:${preference}`));
    updateThemePreference("dark");
    assert.deepEqual(events, ["capture", "attr:dark", "notify:dark", "finish-callback"]);
    unsubscribe();
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
    else delete (globalThis as { window?: unknown }).window;
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as { document?: unknown }).document;
  }
});

test("cross-fades theme changes through the view transition api when available", () => {
  const attributes = new Map<string, string>();
  const updates: (() => void)[] = [];
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      documentElement: {
        getAttribute: (name: string) => attributes.get(name) ?? null,
        setAttribute: (name: string, value: string) => attributes.set(name, value),
      },
      querySelector: () => null,
      startViewTransition: (update: () => void) => {
        updates.push(update);
        return { finished: Promise.resolve() };
      },
    },
  });

  try {
    applyTheme("dark");
    assert.equal(attributes.get("data-theme"), undefined);
    assert.equal(updates.length, 1);
    updates[0]();
    assert.equal(attributes.get("data-theme"), "dark");
    applyTheme("dark");
    assert.equal(updates.length, 1);
  } finally {
    if (previousDocument) Object.defineProperty(globalThis, "document", previousDocument);
    else delete (globalThis as { document?: unknown }).document;
  }
});

test("notifies subscribers for local updates and cross-window storage changes", () => {
  const fake = installFakeWindow();
  const received: string[] = [];
  try {
    const unsubscribe = subscribeThemePreference((preference) => received.push(preference));
    assert.equal(fake.handlers.has("storage"), true);

    updateThemePreference("dark");
    assert.deepEqual(received, ["dark"]);

    fake.storage.set("vrcs.theme", "light");
    fake.handlers.get("storage")!({ key: "vrcs.theme" });
    assert.deepEqual(received, ["dark", "light"]);

    fake.handlers.get("storage")!({ key: "vrcs.other" });
    assert.deepEqual(received, ["dark", "light"]);

    unsubscribe();
    assert.equal(fake.handlers.has("storage"), false);
  } finally {
    fake.restore();
  }
});
