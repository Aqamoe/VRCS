import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

const testDirectory = dirname(fileURLToPath(import.meta.url));
const browser = [process.env.PROGRAMFILES, process.env["PROGRAMFILES(X86)"], process.env.LOCALAPPDATA]
  .filter(Boolean).flatMap(root => [
    join(root, "Google/Chrome/Application/chrome.exe"),
    join(root, "Microsoft/Edge/Application/msedge.exe"),
  ]).find(existsSync);

test("dark theme applies tokens through the document root and settings surfaces", {
  timeout: 45000, skip: !browser && "No installed Chrome or Edge executable was found.",
}, async () => {
  const bundle = await build({
    stdin: { resolveDir: join(testDirectory, ".."), loader: "tsx", contents: `
      import React from 'react';
      import { createRoot } from 'react-dom/client';
      import { useAppTheme } from './src/app/useAppTheme';
      import './src/styles.css';

      function ThemeProbe() {
        const { themePreference, setThemePreference } = useAppTheme();
        return (
          <div className="settings-surface">
            <nav className="settings-tabbar">
              <button id="to-light" onClick={() => setThemePreference('light')}>{themePreference}</button>
              <button id="to-dark" className="active" onClick={() => setThemePreference('dark')}>{themePreference}</button>
            </nav>
            <div className="theme-toggle-group" role="group">
              <button id="segment-dark" className="active" onClick={() => setThemePreference('dark')}>dark</button>
            </div>
          </div>
        );
      }
      createRoot(document.getElementById('root')).render(<ThemeProbe />);
      // Headless Chrome's virtual clock does not advance compositor-driven
      // transitions or view transitions, so pin both to their end state.
      Object.defineProperty(document, 'startViewTransition', { value: undefined, configurable: true });
      const freeze = document.createElement('style');
      freeze.textContent = '* { transition: none !important; }';
      document.head.appendChild(freeze);
      const check = (condition, message) => { if (!condition) throw new Error(message); };
      const settle = () => new Promise(resolve => setTimeout(resolve, 20));
      const wait = async predicate => {
        for (let attempt = 0; attempt < 200; attempt++) {
          const value = predicate();
          if (value) return value;
          await settle();
        }
        throw new Error('Timed out waiting for: ' + predicate.toString());
      };
      const style = element => getComputedStyle(element);
      try {
        const root = document.documentElement;
        const lightButton = await wait(() => document.querySelector('#to-light'));
        const darkButton = document.querySelector('#to-dark');
        lightButton.click();
        await wait(() => root.getAttribute('data-theme') === 'light');
        await settle();
        check(localStorage.getItem('vrcs.theme') === 'light', 'light preference is persisted');
        check(style(root).colorScheme.includes('light'), 'light color scheme applies');
        check(style(root).getPropertyValue('--canvas').trim() === '#f5f8fb', 'light canvas token resolves');
        check(style(document.body).backgroundColor === 'rgb(245, 248, 251)', 'body paints the light canvas');

        darkButton.click();
        await wait(() => root.getAttribute('data-theme') === 'dark');
        await wait(() => style(darkButton).backgroundColor === 'rgb(69, 74, 86)');
        await wait(() => style(darkButton).color === 'rgb(127, 216, 255)');
        await settle();
        check(localStorage.getItem('vrcs.theme') === 'dark', 'dark preference is persisted');
        check(style(root).colorScheme.includes('dark'), 'dark color scheme applies');
        check(style(root).getPropertyValue('--canvas').trim() === '#282c33', 'dark canvas token resolves');
        check(style(root).getPropertyValue('--text').trim() === '#dce0e5', 'dark text token resolves');
        check(style(document.body).backgroundColor === 'rgb(40, 44, 51)', 'body paints the dark canvas');
        check(style(darkButton).getPropertyValue('--surface-hover').trim().startsWith('rgba(255, 255, 255, 0.07'), 'dark hover overlay token resolves');
        check(style(root).getPropertyValue('--primary').trim() === '#3fb9ec', 'dark primary token is toned down');
        check(style(root).getPropertyValue('--primary-ink').trim() === '#7fd8ff', 'dark accent ink is light');
        check(style(darkButton).backgroundColor === 'rgb(69, 74, 86)', 'active settings tab uses the dark selected surface');
        check(style(darkButton).color === 'rgb(127, 216, 255)', 'active settings tab uses the light accent ink');
        const segment = document.querySelector('#segment-dark');
        check(style(segment).backgroundColor === 'rgb(69, 74, 86)', 'active theme segment uses the dark selected surface');
        check(style(segment).color === 'rgb(127, 216, 255)', 'active theme segment uses the light accent ink');
        document.getElementById('result').textContent = 'PASS';
      } catch (error) { document.getElementById('result').textContent = 'FAIL: ' + error.message; }
    ` },
    bundle: true, write: false, outfile: 'app-theme-browser-test.js', format: 'esm', platform: 'browser', jsx: 'automatic',
    define: { 'import.meta.env.VITE_VRCS_SESSION_TOKEN': '""' },
  });

  const directory = mkdtempSync(join(testDirectory, '.app-theme-browser-'));
  try {
    const html = join(directory, 'test.html');
    const content = join(directory, 'content.html');
    const javascript = bundle.outputFiles.find(file => file.path.endsWith('.js')).text;
    const stylesheet = bundle.outputFiles.find(file => file.path.endsWith('.css')).text;
    const errorCapture = '<script>const result = document.getElementById("result"); const reportError = message => { result.textContent = "FAIL: " + message; }; window.addEventListener("error", event => reportError(event.message)); window.addEventListener("unhandledrejection", event => reportError(String(event.reason))); new MutationObserver(() => parent.postMessage(result.textContent, "*")).observe(result, { childList: true });</script>';
    writeFileSync(content, '<style>' + stylesheet + '</style><div id="root"></div><pre id="result">PENDING</pre>'
      + errorCapture + '<script type="module">' + javascript.replace(/<\/script/gi, '<\\/script') + '</script>');
    writeFileSync(html, '<iframe src="content.html" style="display:block;border:0;width:1200px;height:900px"></iframe><pre id="result">PENDING</pre>'
      + '<script>window.addEventListener("message", event => { if (event.source === document.querySelector("iframe").contentWindow) document.getElementById("result").textContent = event.data; });</script>');
    const result = spawnSync(browser, [
      '--headless=new', '--dump-dom', '--virtual-time-budget=10000', '--disable-gpu', '--no-first-run',
      '--disable-background-networking', '--disable-extensions', '--window-size=1200,900',
      '--user-data-dir=' + join(directory, 'profile'), pathToFileURL(html).href,
    ], { encoding: 'utf8', timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, />PASS</, 'dark theme probe failed: ' + result.stdout.match(/<pre id="result">[^<]*/)?.[0]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
