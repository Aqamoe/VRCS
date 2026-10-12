import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

for (const viewport of [{ width: 600, height: 760 }, { width: 380, height: 400 }]) {
test(`system browser validates VQA layout and interactions at ${viewport.width}×${viewport.height}`, {
  timeout: 45000, skip: !browser && "No installed Chrome or Edge executable was found.",
}, async () => {
  const bundle = await build({
    stdin: { resolveDir: join(testDirectory, ".."), loader: "tsx", contents: `
      import React from 'react';
      import { createRoot } from 'react-dom/client';
      import i18next from 'i18next';
      import { initReactI18next } from 'react-i18next';
      import en from './src/i18n/locales/en-US.json';
      import zh from './src/i18n/locales/zh-CN.json';
      import ja from './src/i18n/locales/ja-JP.json';
      import { VisionWindow } from './src/vision/VisionWindow';
      import './src/styles.css';
      await i18next.use(initReactI18next).init({ lng: 'en-US', resources: {
        'en-US': { translation: en.translation }, 'zh-CN': { translation: zh.translation }, 'ja-JP': { translation: ja.translation },
      } });
      const calls = [], order = [], revoked = [], errors = [];
      let copiedText = '';
      Object.defineProperty(navigator, 'clipboard', { value: {
        writeText: async text => { copiedText = text; },
      }, configurable: true });
      window.addEventListener('error', event => errors.push(event.message));
      window.addEventListener('securitypolicyviolation', event => errors.push('CSP: ' + event.violatedDirective));
      const count = command => calls.filter(call => call.command === command).length;
      const settle = () => new Promise(resolve => setTimeout(resolve, 20));
      const checkLayout = () => {
        const content = document.querySelector('.vision-window-content');
        check(content.scrollWidth <= content.clientWidth + 1, 'Content does not overflow horizontally');
        check(document.documentElement.scrollWidth <= innerWidth, 'Window does not overflow horizontally');
        const question = document.querySelector('.vision-window-question textarea');
        if (question) check(getComputedStyle(question).resize === 'none', 'Question editor cannot be resized by dragging');
        const toolbar = document.querySelector('.vision-window-toolbar').getBoundingClientRect();
        for (const button of document.querySelectorAll('.vision-window-actions button')) {
          const rect = button.getBoundingClientRect();
          check(rect.left >= 0 && rect.right <= innerWidth && rect.top >= toolbar.top && rect.bottom <= innerHeight, 'Toolbar actions remain inside the window');
          check(button.textContent.trim() === '' && button.querySelector('svg'), 'Toolbar actions show icons without text');
          check(button.getAttribute('aria-label') && button.getAttribute('data-tooltip'), 'Icon actions retain accessible names and tooltips');
          check(rect.width === 44 && rect.height === 44 && getComputedStyle(button).borderRadius === '50%', 'Toolbar actions use the main dock circular button dimensions');
        }
      };
      let status = { available: true, state: 'idle', session_id: 0, revision: 1, image_id: null,
        question: '', answer: '', error: null, error_detail: null, shortcut_error: null,
        shortcut: 'Ctrl+Alt+V', profile_name: 'Images', model: 'image-model' };
      const png = Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=='), c => c.charCodeAt(0)).buffer;
      window.nativeInvoke = async (command, args) => {
        calls.push({ command, args });
        if (command === 'desktop_vision_status') { order.push('snapshot'); return status; }
        if (command === 'desktop_vision_image') return png;
      };
      window.nativeListen = async (name, callback) => {
        order.push(name);
        window.receive = next => callback({ payload: next });
        return () => { window.receive = null; };
      };
      const revoke = URL.revokeObjectURL.bind(URL);
      URL.revokeObjectURL = url => { revoked.push(url); revoke(url); };
      const root = createRoot(document.getElementById('root'));
      root.render(<VisionWindow />);
      const check = (condition, message) => { if (!condition) throw new Error(message); };
      const wait = async predicate => {
        for (let attempt = 0; attempt < 200; attempt++) {
          if (predicate()) return;
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error('Timed out waiting for the frontend');
      };
      const emit = async patch => {
        status = { ...status, revision: status.revision + 1, ...patch };
        window.receive(status);
        await settle();
      };
      const click = async label => {
        const button = document.querySelector('button[aria-label="' + label + '"]')
          ?? Array.from(document.querySelectorAll('button')).find(button => button.textContent.trim() === label);
        check(button && !button.disabled, 'Action is available: ' + label);
        button.click();
        await settle();
      };
      try {
        await wait(() => document.querySelector('button[aria-label="Capture VRChat"]')?.disabled === false);
        check(count('desktop_vision_capture') === 0 && count('desktop_vision_start_recording') === 0 && count('desktop_vision_ask') === 0, 'Opening an empty session performs no capture, recording or send');
        check(!document.querySelector('textarea, img'), 'First screenshot has no existing image or editor');
        check(innerWidth === ${viewport.width} && innerHeight === ${viewport.height}, 'Browser uses the requested viewport: ' + innerWidth + 'x' + innerHeight);
        checkLayout();
        await click('Capture VRChat');
        check(calls.filter(call => ['desktop_vision_cancel', 'desktop_vision_capture'].includes(call.command)).map(call => call.command).join(',') === 'desktop_vision_cancel,desktop_vision_capture', 'First screenshot is available without an existing image');
        await emit({ state: 'error', error: 'desktop_vision.game_not_found' });
        check(document.querySelector('[role="alert"]')?.textContent.includes(en.translation.visionWindow.errors.game_not_found), 'Explain a missing game window');
        await click('Capture VRChat');
        check(count('desktop_vision_capture') === 2, 'First screenshot can be retried after failure');
        await emit({ state: 'editing', session_id: 1, revision: 0, image_id: 7, error: null });
        await wait(() => document.querySelector('textarea') && document.querySelector('img')?.naturalWidth === 1);
        const textarea = document.querySelector('textarea');
        const send = document.querySelector('button[aria-label="Send question"]');
        const imageUrl = document.querySelector('img').src;
        check(imageUrl.startsWith('blob:'), 'Actual PNG preview loads under the native image CSP');
        const key = options => {
          const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...options });
          textarea.dispatchEvent(event);
          return event;
        };
        const fill = async value => {
          Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(textarea, value);
          textarea.dispatchEvent(new Event('input', { bubbles: true }));
          await settle();
        };
        check(order.join(',') === 'desktop-vision-status-changed,snapshot', 'Subscribe before snapshot');
        check(count('desktop_vision_ask') === 0, 'No automatic send');
        check(!textarea.hasAttribute('maxlength'), 'Do not impose a UTF-16 limit');
        check(!document.getElementById('vision-question-hint') && !textarea.hasAttribute('aria-describedby'), 'Removed shortcut hint has no visible text or accessibility reference');
        check(key({ key: 'Enter' }).defaultPrevented && count('desktop_vision_ask') === 0, 'Enter cannot send an empty question');
        await fill('First line');
        textarea.focus();
        textarea.setSelectionRange(textarea.value.length, textarea.value.length);
        check(!key({ key: 'Enter', shiftKey: true }).defaultPrevented, 'Shift+Enter permits the browser newline edit');
        check(document.execCommand('insertText', false, '\\n'), 'Browser inserts a newline');
        await settle();
        check(textarea.value === 'First line\\n' && count('desktop_vision_ask') === 0, 'Shift+Enter adds a newline without sending');
        await fill('😀'.repeat(4096));
        check(!send.disabled && !document.getElementById('vision-question-limit'), '4096 astral scalars are allowed');
        await fill('😀'.repeat(4097));
        check(send.disabled && textarea.getAttribute('aria-invalid') === 'true', '4097 scalars block sending');
        check(document.getElementById('vision-question-limit')?.textContent.includes('4096'), 'Explain the scalar limit');
        check(key({ key: 'Enter' }).defaultPrevented, 'Enter does not insert a newline when sending is blocked');
        check(count('desktop_vision_ask') === 0, 'Enter cannot bypass the limit');
        await fill('  ' + '😀'.repeat(4096) + '\\n ');
        check(!send.disabled && !textarea.hasAttribute('aria-invalid'), 'Trim whitespace before counting');
        key({ key: 'Enter', isComposing: true });
        key({ key: 'Enter', keyCode: 229 });
        textarea.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
        key({ key: 'Enter' });
        key({ key: 'Escape' });
        textarea.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
        check(count('desktop_vision_ask') === 0 && count('desktop_vision_close') === 0, 'IME cannot send or close');
        check(key({ key: 'Enter' }).defaultPrevented, 'Enter sends without inserting a newline');
        await wait(() => calls.some(call => call.command === 'desktop_vision_ask'));
        const request = calls.find(call => call.command === 'desktop_vision_ask');
        check(request.args.imageId === 7 && request.args.question === '😀'.repeat(4096), 'Send the trimmed draft and current image ID');
        await emit({ state: 'requesting' });
        check(send.disabled, 'Busy state blocks sending');
        for (const code of ['selection_busy', 'busy', 'recording_timeout', 'no_image', 'stale_image', 'core_unavailable', 'recognition_failed', 'request_failed']) {
          await emit({ state: 'error', error: 'desktop_vision.' + code });
          check(document.querySelector('[role="alert"]')?.textContent.includes(en.translation.visionWindow.errors[code]), 'Localize native error: ' + code);
          check(textarea.value === '  ' + '😀'.repeat(4096) + '\\n ', 'Errors preserve the editable draft');
        }
        check(count('desktop_vision_image') === 1, 'Status updates do not refetch the preview');
        check(count('desktop_vision_capture') === 2 && count('desktop_vision_start_recording') === 0, 'Status and editing perform no implicit capture or microphone');
        await emit({ state: 'editing', error: null });
        await click('Start speaking');
        check(count('desktop_vision_start_recording') === 1, 'Recording starts explicitly');
        await emit({ state: 'recording', question: 'Voice draft' });
        check(textarea.disabled && textarea.value === 'Voice draft', 'Show recognized speech while recording');
        check(document.querySelector('button[aria-label="Finish recognition"]').getAttribute('aria-pressed') === 'true', 'Recording action exposes its active state');
        checkLayout();
        await click('Finish recognition');
        check(count('desktop_vision_finish_recording') === 1, 'Recognition finishes explicitly');
        await emit({ state: 'finalizing_asr' });
        check(send.disabled && count('desktop_vision_ask') === 1, 'Finalizing recognition cannot send');
        await emit({ state: 'editing', question: 'Recognized question' });
        check(!textarea.disabled && textarea.value === 'Recognized question' && count('desktop_vision_ask') === 1, 'Recognition returns an editable draft without sending');
        await fill('Edited voice\\nQuestion');
        const beforeReselect = calls.length;
        await click('Select another area');
        check(calls.slice(beforeReselect).map(call => call.command).join(',') === 'desktop_vision_cancel,desktop_vision_capture', 'Reselection cancels work before capture');
        await emit({ state: 'selecting' });
        check(send.disabled, 'Selection blocks sending');
        await click('Cancel current work');
        await emit({ session_id: 2, revision: 0, state: 'editing', image_id: 7 });
        check(textarea.value === 'Edited voice\\nQuestion' && document.querySelector('img').src === imageUrl, 'Cancelled reselection restores the old image and editable draft');
        check(count('desktop_vision_image') === 1 && revoked.length === 0 && count('desktop_vision_ask') === 1, 'Cancellation neither refetches the preview nor sends');
        await emit({ state: 'complete', answer: 'Final answer' });
        await click('Copy answer');
        check(copiedText === 'Final answer', 'Answer header copies the current answer');
        check(document.querySelector('.vision-window-copy').textContent.trim() === 'Copied', 'Copy action shows confirmation');
        await click('Ask another question');
        check(document.activeElement === textarea, 'Ask again focuses the question editor');
        await emit({ answer: 'A long answer\\n'.repeat(100), profile_name: 'VeryLongServiceName'.repeat(20), model: 'very-long-model-name'.repeat(20) });
        for (const language of ['en-US', 'zh-CN', 'ja-JP']) {
          await i18next.changeLanguage(language);
          document.documentElement.lang = language;
          await settle();
          checkLayout();
          const content = document.querySelector('.vision-window-content');
          check(!content.textContent.includes(i18next.t('visionWindow.states.complete')), 'Answer-ready status is not shown above the content');
          check(!content.textContent.includes(status.profile_name) && !content.textContent.includes(status.model), 'Service and model information is not shown');
          check(content.scrollHeight > content.clientHeight, 'Long answers scroll within the content region');
          content.scrollTop = content.scrollHeight;
          checkLayout();
        }
        key({ key: 'Escape' });
        await settle();
        check(count('desktop_vision_close') === 1, 'Escape closes the native session');
        await emit({ session_id: 3, revision: 0, state: 'idle', image_id: null, question: '', answer: '' });
        check(!document.querySelector('textarea, img') && !document.getElementById('root').textContent.includes('Final answer'), 'Close clears the displayed image, question and answer');
        check(revoked.length === 1 && revoked[0] === imageUrl, 'Close revokes the preview URL');
        root.unmount();
        check(revoked.length === 1 && window.receive === null, 'Unmount releases the subscription without revoking twice');
        check(errors.length === 0, 'No browser errors: ' + errors.join(', '));
        parent.document.getElementById('result').textContent = 'PASS';
      } catch (error) { parent.document.getElementById('result').textContent = 'FAIL: ' + error.message; }
    ` },
    plugins: [{ name: "native-contract", setup(build) {
      build.onResolve({ filter: /^@tauri-apps\/api\/(core|event)$/ }, args => ({ path: args.path, namespace: "mock-native" }));
      build.onLoad({ filter: /.*/, namespace: "mock-native" }, args => ({ contents: args.path.endsWith("/core")
        ? "export const isTauri = () => true; export const invoke = (command, args) => window.nativeInvoke(command, args);"
        : "export const listen = (name, callback) => window.nativeListen(name, callback);" }));
    } }],
    bundle: true, write: false, outfile: "vision-system-test.js", format: "esm", platform: "browser", jsx: "automatic",
  });
  const directory = mkdtempSync(join(testDirectory, ".vision-system-browser-"));
  try {
    const html = join(directory, "test.html");
    const javascript = bundle.outputFiles.find(file => file.path.endsWith(".js")).text;
    const stylesheet = bundle.outputFiles.find(file => file.path.endsWith(".css")).text;
    const csp = JSON.parse(readFileSync(join(testDirectory, "../src-tauri/tauri.conf.json"), "utf8")).app.security.csp;
    const scriptPolicy = /(?:^|;)\s*script-src\s+([^;]*)/.exec(csp);
    const defaultSources = /(?:^|;)\s*default-src\s+([^;]*)/.exec(csp)?.[1] ?? "'self'";
    const harnessCsp = scriptPolicy ? csp.replace(scriptPolicy[0], scriptPolicy[0] + " 'unsafe-inline'")
      : csp + "; script-src " + defaultSources + " 'unsafe-inline'";
    const escapedCsp = harnessCsp.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
    // An iframe gives an exact viewport even when the system browser enforces a minimum window size.
    const frame = '<style>' + stylesheet + '</style><div id="root"></div><script type="module">'
      + javascript.replace(/<\/script/gi, "<\\/script") + '</script>';
    const escapedFrame = frame.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
    writeFileSync(html, '<meta http-equiv="Content-Security-Policy" content="' + escapedCsp
      + '"><iframe style="border:0;width:' + viewport.width + 'px;height:' + viewport.height
      + 'px" srcdoc="' + escapedFrame + '"></iframe><pre id="result">PENDING</pre>');
    const result = spawnSync(browser, [
      "--headless=new", "--dump-dom", "--virtual-time-budget=10000", "--disable-gpu", "--no-first-run",
      `--window-size=${viewport.width},${viewport.height}`, "--force-device-scale-factor=1",
      "--disable-background-networking", "--disable-extensions", "--user-data-dir=" + join(directory, "profile"),
      pathToFileURL(html).href,
    ], { encoding: "utf8", timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    const report = result.stdout.match(/<pre id="result">([^<]*)<\/pre>/)?.[1];
    assert.equal(report, "PASS", report || result.stderr);
  } finally { rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 }); }
});
}
