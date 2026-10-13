import { useEffect, useState } from "react";
import { flushSync } from "react-dom";

import {
  applyTheme,
  readThemePreference,
  resolveTheme,
  subscribeThemePreference,
  updateThemePreference,
} from "./app-theme";
import type { ThemePreference } from "./app-theme";

const PREFERS_DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Applies the theme preference for this window and exposes the setting for
 * the system settings panel. All mounts share the module-level preference
 * state in app-theme.ts.
 */
export function useAppTheme() {
  const [themePreference, setPreference] = useState<ThemePreference>(readThemePreference);

  useEffect(() => subscribeThemePreference((preference) => {
    // Commit synchronously so the toggle buttons repaint inside the same
    // cross-fade frame as the theme attribute flip.
    flushSync(() => setPreference(preference));
  }), []);

  useEffect(() => {
    const media = window.matchMedia(PREFERS_DARK_QUERY);
    applyTheme(resolveTheme(themePreference, media.matches));
    const handleSystemThemeChange = () => {
      if (themePreference === "system") applyTheme(resolveTheme("system", media.matches));
    };
    media.addEventListener("change", handleSystemThemeChange);
    return () => media.removeEventListener("change", handleSystemThemeChange);
  }, [themePreference]);

  return { themePreference, setThemePreference: updateThemePreference };
}
