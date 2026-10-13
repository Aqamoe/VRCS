// Applies the persisted theme preference before first paint to prevent a
// light flash in dark mode. Keep the storage key and resolution in sync
// with src/app/app-theme.ts.
(function () {
  var stored = null;
  try {
    stored = window.localStorage.getItem("vrcs.theme");
  } catch (error) {
    stored = null;
  }
  var dark = stored === "dark"
    || (stored !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.setAttribute("data-theme", dark ? "dark" : "light");
})();
