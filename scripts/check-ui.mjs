// Run with Node 22.13+: node scripts/check-ui.mjs (no browser, network or test dependency).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import vm from "node:vm";

const source = file => stripTypeScriptTypes(readFileSync(new URL(`../src/${file}`, import.meta.url), "utf8"))
  .replace(/^import .*;\r?\n/gm, "");
const deferred = () => Promise.withResolvers();
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };

function overlay(overrides = {}, file = "overlay/overlay.ts") {
  const elements = new Map(), listeners = new Map(), events = new Map(), timers = new Map();
  const calls = [], emitted = [], handlers = new Map(Object.entries(overrides));
  let nextTimer = 0;
  const clock = {
    setTimeout(fn, delay) { const id = ++nextTimer; timers.set(id, { fn, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  };
  function element(id = "") {
    const el = {
      id, className: "", style: {}, dataset: {}, disabled: false, width: 0, height: 0,
      innerHTML: "", textContent: "", children: [], parent: null, handlers: new Map(),
      addEventListener(name, fn) { this.handlers.set(name, fn); },
      appendChild(child) { this.children.push(child); child.parent = this; },
      contains(target) { return target === this || this.children.some(child => child.contains(target)); },
      closest(selector) { return this.classList.contains(selector.slice(1)) ? this : this.parent?.closest(selector) ?? null; },
      getContext() { return {
        putImageData: data => { this.lastFrame = data; },
        drawImage() {}, beginPath() {}, roundRect() {}, fill() {},
        fillText(text, x, y) { calls.push({ name: "canvas-text", args: { text, x, y } }); },
        measureText: text => ({ width: text.length * 6 }),
      }; },
      toDataURL() { return "data:image/png;base64,test"; },
      removeAttribute(name) { delete this[name]; },
      hasAttribute(name) { return Object.hasOwn(this, name); },
      click() { return this.handlers.get("click")?.({ target: this, preventDefault() {}, stopPropagation() {} }); },
    };
    el.classList = {
      contains: name => el.className.split(/\s+/).includes(name),
      add: (...names) => { el.className = [...new Set([...el.className.split(/\s+/).filter(Boolean), ...names])].join(" "); },
      remove: (...names) => { el.className = el.className.split(/\s+/).filter(name => !names.includes(name)).join(" "); },
    };
    return el;
  }
  const get = id => {
    if (!elements.has(id)) elements.set(id, element(id));
    return elements.get(id);
  };
  const context = vm.createContext({
    console: { error() {} }, ...clock,
    window: { ...clock, innerWidth: 1920, innerHeight: 1080, getSelection: () => null,
      addEventListener: (name, fn) => listeners.set(name, fn) },
    document: { getElementById: get, createElement: () => element(), addEventListener() {},
      documentElement: { style: { setProperty() {} } } },
    navigator: { clipboard: { writeText: async text => handlers.get("clipboard")?.(text) } },
    localStorage: { getItem: () => null, setItem() {} },
    getCurrentWebviewWindow: () => ({ label: "pin", setAlwaysOnTop: async () => {}, startDragging: async () => {} }),
    Image: class { complete = true; },
    ImageData: class {
      constructor(bytes, width, height) { assert.equal(bytes.length, width * height * 4); Object.assign(this, { bytes, width, height }); }
    },
    requestAnimationFrame: fn => clock.setTimeout(fn, 0),
    listen: async (name, fn) => { events.set(name, fn); },
    emit: async (name, payload) => { emitted.push({ name, payload }); },
    invoke: async (name, args = {}) => {
      calls.push({ name, args });
      if (handlers.has(name)) return handlers.get(name)(args);
      if (name === "get_config") return { app_language: "zh-CN" };
      if (name === "get_capture_frame") return new ArrayBuffer(16);
      if (name === "show_result_window") return 77;
      if (name === "extract_text_from_selection") return "text";
      return null;
    },
  });
  vm.runInContext(source(file), context, { filename: file });
  const run = code => vm.runInContext(code, context);
  const fire = (name, payload) => events.get(name)({ payload });
  const begin = id => fire("screenshot-captured", { capture_id: id, width: 2, height: 2, scale_factor: 1, shell_in_front: false });
  const select = () => run("currentRect = { x: 10, y: 20, w: 100, h: 80 }; selectionBox.classList.remove('hidden')");
  const mouse = (target, button = 0) => listeners.get("mousedown")({ target, button, clientX: 30, clientY: 40, preventDefault() {} });
  return { run, fire, begin, select, mouse, get, element, calls, emitted, handlers, timers, listeners };
}
const result = text => ({ failed_blocks: 0, image_data: "data:image/png;base64,test", width: 100, height: 80, scale_factor: 1,
  blocks: [{ id: 0, original_text: "original", translated_text: text, rect: { x: 0, y: 0, width: 50, height: 20 }, font_size_px: 14, bg_color: "white", text_color: "black" }] });

// Old successful/rejected translations must not render, toast, or reset a newer busy button.
{
  const ui = overlay(); await ui.begin(1); ui.select();
  const first = deferred(), second = deferred(); let n = 0;
  ui.handlers.set("translate_in_place", () => (++n === 1 ? first.promise : second.promise));
  const old = ui.run("triggerTranslate()"); await flush();
  ui.run("resetSelection()"); ui.select();
  const current = ui.run("triggerTranslate()"); await flush();
  first.resolve(result("old")); await old;
  assert.equal(ui.run("currentTranslationResult"), null);
  assert.equal(ui.get("btn-translate").disabled, true);
  second.resolve({ ...result("new"), failed_blocks: 1, error: "rate limited" }); await current;
  assert.equal(ui.run("currentTranslationResult.blocks[0].translated_text"), "new");
  assert.match(ui.get("toast").className, /warning/);
  assert.match(ui.get("toast").textContent, /rate limited/);

  ui.run("resetSelection()"); ui.select();
  const rejected = deferred(); ui.handlers.set("translate_in_place", () => rejected.promise);
  const failed = ui.run("triggerTranslate()"); await flush();
  ui.run("resetSelection(); showToast('new state', 'info')");
  rejected.reject(new Error("stale failure")); await failed;
  assert.equal(ui.get("toast").textContent, "new state");
}

// Drawing, moving and resizing invalidate pending work. One right click only resets the selection.
{
  const ui = overlay(); await ui.begin(1);
  for (const mode of ["drawing", "moving", "resizing"]) {
    ui.select();
    const target = mode === "drawing" ? ui.get("mask-layer") : mode === "moving" ? ui.get("selection-box") : ui.element();
    if (mode === "resizing") { target.className = "resize-handle"; target.dataset.handle = "se"; }
    const before = ui.run("generation"); await ui.mouse(target);
    assert.ok(ui.run("generation") > before);
    assert.equal(ui.run("mode"), mode);
  }
  ui.select(); await ui.mouse(ui.get("mask-layer"), 2);
  ui.listeners.get("contextmenu")({ preventDefault() {} });
  assert.equal(ui.run("currentRect.w"), 0);
  assert.equal(ui.run("captureActive"), true);
  assert.ok(!ui.calls.some(call => call.name === "cancel_capture"));
}

// OCR close timers cannot close a later selection/capture, even when already queued to run.
{
  const ui = overlay(); await ui.begin(1); ui.select();
  await ui.run("triggerOcrText()");
  const timer = [...ui.timers.values()].find(timer => timer.delay === 450); assert.ok(timer);
  ui.run("resetSelection()"); ui.select(); timer.fn(); await flush();
  assert.ok(!ui.calls.some(call => call.name === "cancel_capture"));
  await ui.run("triggerOcrText()");
  const closing = [...ui.timers.values()].find(timer => timer.delay === 450); closing.fn(); await flush();
  assert.equal(ui.calls.at(-1).name, "cancel_capture");
  assert.equal(ui.calls.at(-1).args.captureId, 1);
  assert.equal(ui.get("freeze-layer").width, 0);
  assert.equal(ui.get("freeze-layer").height, 0);
}

// Late frame/config responses and old end events cannot repaint or clear the newer capture.
{
  const initialConfig = deferred(); let configs = 0;
  const ui = overlay({ get_config: () => ++configs === 1 ? initialConfig.promise : { app_language: "zh-CN" } });
  const frame = deferred(); ui.handlers.set("get_capture_frame", () => frame.promise);
  const old = ui.begin(1); await flush();
  ui.handlers.delete("get_capture_frame"); await ui.begin(2);
  initialConfig.resolve({ app_language: "en" }); frame.resolve(new ArrayBuffer(16)); await old; await flush();
  assert.equal(ui.run("currentAppLang"), "zh-CN");
  ui.fire("capture-ended", 1);
  assert.equal(ui.run("captureActive"), true);
  assert.equal(ui.get("freeze-layer").width, 2);
  ui.fire("capture-ended", 2);
  assert.equal(ui.run("captureActive"), false);
  assert.equal(ui.get("freeze-layer").width, 0);
  assert.equal(ui.get("freeze-layer").height, 0);
  await ui.begin(1); // Out-of-order old start must not revive an ended session.
  assert.equal(ui.run("captureActive"), false);
}

// Cancelled saves and copy/pin failures keep the selection; completed old saves leave the new UI intact.
{
  const ui = overlay(); await ui.begin(1); ui.select();
  await ui.run("triggerSave()");
  assert.equal(ui.run("currentRect.w"), 100);
  for (const [command, action] of [["copy_selection_to_clipboard", "triggerCopy()"], ["pin_screenshot", "triggerPin()"]]) {
    ui.handlers.set(command, () => Promise.reject(new Error("unavailable")));
    await ui.run(action);
    assert.equal(ui.run("captureActive"), true);
    assert.equal(ui.run("currentRect.w"), 100);
    assert.match(ui.get("toast").className, /warning/);
  }
  assert.ok(!ui.calls.some(call => ["close_capture", "cancel_capture"].includes(call.name)));
  const saving = deferred(); ui.handlers.set("save_selection_to_file", () => saving.promise);
  const pending = ui.run("triggerSave()"); await ui.begin(2); ui.select();
  saving.resolve("saved.png"); await pending;
  assert.equal(ui.run("captureId"), 2);
  assert.equal(ui.run("captureActive"), true);
  assert.equal(ui.run("currentRect.w"), 100);
}

// Popup handoff includes both identities; a delayed old handoff must not emit a new translation.
{
  const ui = overlay({ get_config: () => ({ in_place_translate: false }) });
  await ui.begin(1); ui.select(); await ui.run("triggerTranslate()");
  const flow = ui.emitted.find(event => event.name === "start-translate-flow");
  assert.equal(flow.payload.request_id, 77);
  assert.equal(flow.payload.capture_id, 1);
  assert.equal(flow.payload.rect.width, 100);
  assert.equal(ui.run("captureActive"), false);
  await ui.begin(2); ui.select();
  const showing = deferred(); ui.handlers.set("show_result_window", () => showing.promise);
  const old = ui.run("triggerTranslate()"); await flush(); await ui.begin(3);
  showing.resolve(78); await old;
  assert.equal(ui.emitted.length, 1);
  assert.ok(ui.calls.some(call => call.name === "close_result_window" && call.args.requestId === 78));
  assert.equal(ui.run("captureActive"), true);
}

// Pending configuration must not launch translation after a reset; late same-session frames stay released.
{
  const ui = overlay(); await ui.begin(1); ui.select();
  const config = deferred(); ui.handlers.set("get_config", () => config.promise);
  const translating = ui.run("triggerTranslate()"); ui.run("resetSelection()");
  config.resolve({ in_place_translate: true }); await translating;
  assert.ok(!ui.calls.some(call => call.name === "translate_in_place"));
  const frame = deferred(); ui.handlers.set("get_capture_frame", () => frame.promise);
  const starting = ui.begin(2); await flush(); ui.fire("capture-ended", 2);
  frame.resolve(new ArrayBuffer(16)); await starting;
  assert.equal(ui.get("freeze-layer").width, 0);
  assert.equal(ui.run("captureActive"), false);
}

// All result producers share request IDs, including progressive OCR and retry handoffs.
{
  const first = deferred(), config = deferred();
  const ui = overlay({ translate_selection: () => first.promise, get_config: () => config.promise }, "result/result.ts");
  const old = ui.fire("start-translate-flow", { request_id: 1, capture_id: 10, rect: { x: 0, y: 0, width: 20, height: 20 } });
  ui.handlers.delete("get_config");
  ui.fire("translation-started", { request_id: 2 });
  ui.fire("ocr-ready", { request_id: 2, text: "current original" });
  assert.equal(ui.get("btn-copy-original").disabled, false);
  assert.equal(ui.get("btn-copy-translation").disabled, true);
  const current = { request_id: 2, original_text: "current original", translated_text: "current translation" };
  ui.fire("translation-result", current);
  first.resolve({ request_id: 1, original_text: "old", translated_text: "old" });
  config.resolve({ app_language: "en" }); await old; await flush();
  ui.fire("ocr-ready", { request_id: 2, text: "late OCR" });
  ui.fire("translation-error", { request_id: 1, error: "old error" });
  assert.equal(ui.run("currentTranslatedText"), "current translation");
  assert.equal(ui.run("currentOriginalText"), "current original");
  assert.equal(ui.run("currentAppLang"), "zh-CN");
  assert.equal(ui.get("btn-copy-translation").disabled, false);
  ui.handlers.set("clipboard", () => Promise.reject(new Error("clipboard busy")));
  await ui.get("btn-copy-translation").click();
  assert.equal(ui.get("btn-copy-translation").classList.contains("copied"), false);
  assert.match(ui.get("copy-btn-text").textContent, /失败/);

  const showing = deferred(); ui.handlers.set("show_result_window", () => showing.promise);
  const retry = ui.run("retryTranslation()");
  ui.fire("translation-started", { request_id: 5 });
  showing.resolve(4); await retry;
  assert.equal(ui.run("latestRequestId"), 5);
  assert.ok(!ui.calls.some(call => call.name === "retry_translate"));
  ui.fire("translation-closed", 5);
  ui.fire("translation-started", { request_id: 5 });
  ui.fire("translation-result", { ...current, request_id: 5 });
  assert.equal(ui.run("requestOpen"), false);
  assert.equal(ui.run("currentTranslatedText"), "");
}

// Closing during retry-ID allocation cancels the eventual request, not another newer window.
{
  const ui = overlay({}, "result/result.ts");
  ui.fire("translation-started", { request_id: 1 });
  ui.fire("translation-result", { request_id: 1, original_text: "retry me", translated_text: "old" });
  const showing = deferred(); ui.handlers.set("show_result_window", () => showing.promise);
  const retry = ui.run("retryTranslation()");
  await ui.run("closeResult()");
  ui.fire("translation-started", { request_id: 2 });
  showing.resolve(2); await retry;
  assert.equal(ui.run("requestOpen"), false);
  assert.ok(ui.calls.some(call => call.name === "close_result_window" && call.args.requestId === 2));
  assert.ok(!ui.calls.some(call => call.name === "retry_translate"));
}

// A late OCR notification preserves retry text without hiding an already displayed error.
{
  const ui = overlay({}, "result/result.ts");
  ui.fire("translation-started", { request_id: 1 });
  ui.fire("translation-error", { request_id: 1, error: { type: "NetworkError", message: "offline" } });
  ui.fire("ocr-ready", { request_id: 1, text: "retry text" });
  assert.equal(ui.run("currentOriginalText"), "retry text");
  assert.equal(ui.get("error-state").classList.contains("hidden"), false);
  ui.handlers.set("retry_translate", () => Promise.reject({ type: "NetworkError", message: "still offline" }));
  await ui.run("retryTranslation()");
  assert.equal(ui.run("currentOriginalText"), "retry text");
  assert.ok(ui.calls.some(call => call.name === "retry_translate" && call.args.originalText === "retry text"));
}

// Reused pin windows retain no image after closing; late initial data cannot restore it.
{
  const loading = deferred();
  const ui = overlay({ get_pin_data: () => loading.promise }, "pin/pin.ts");
  const data = { label: "pin", image_data: "new image", width: 20, height: 20, has_shadow: true, opacity: 70 };
  ui.fire("load-pin", data);
  loading.resolve({ ...data, image_data: "old image" }); await flush();
  assert.equal(ui.get("pin-image").src, "new image");
  ui.fire("clear-pin", "pin_1");
  assert.equal(ui.get("pin-image").src, "new image");
  ui.fire("pin-toast", "saved");
  ui.fire("clear-pin", "pin");
  assert.equal(ui.get("pin-image").hasAttribute("src"), false);
  assert.equal(ui.timers.size, 0);
  ui.fire("pin-toast", "late toast");
  assert.equal(ui.get("toast").classList.contains("hidden"), true);
}

// Preserve source/output line breaks through display, copying, merge toggles and retries.
{
  const original = "First line\nSecond line\n\nNext paragraph";
  const translated = "第一行译文\n第二行译文\n\n下一段";
  let copied;
  const ui = overlay({ clipboard: text => { copied = text; } }, "result/result.ts");
  ui.fire("translation-started", { request_id: 1 });
  ui.fire("ocr-ready", { request_id: 1, text: original });
  assert.equal(ui.get("original-text").textContent, original);
  await ui.get("btn-copy-original").click();
  assert.equal(copied, original);
  await ui.get("btn-toggle-breaks").click();
  ui.fire("translation-result", { request_id: 1, original_text: original, translated_text: translated });
  assert.equal(ui.run("isPreserveBreaks"), false);
  assert.equal(ui.run("rawOriginalText"), original);
  assert.equal(ui.get("translated-text").textContent, translated);
  await ui.get("btn-copy-translation").click();
  assert.equal(copied, translated);
  ui.handlers.set("retry_translate", args => ({ request_id: args.requestId, original_text: args.originalText, translated_text: translated }));
  await ui.run("retryTranslation()");
  assert.equal(ui.calls.find(call => call.name === "retry_translate").args.originalText, original);
  assert.equal(ui.run("isPreserveBreaks"), false);
  await ui.get("btn-toggle-breaks").click();
  assert.equal(ui.get("original-text").textContent, original);
}

// Canvas handles explicit newlines (including empty lines), just like the displayed cards.
{
  const ui = overlay();
  const image = { ...result("a\n\nb\rc\r\nd😀"), height: 200 };
  await ui.run(`generateTranslatedImageDataUrl(${JSON.stringify(image)})`);
  const drawn = ui.calls.filter(call => call.name === "canvas-text").map(call => call.args);
  assert.deepEqual(drawn.map(item => item.text), ["a", "", "b", "c", "d😀"]);
  for (let i = 1; i < drawn.length; i++) assert.ok(drawn[i].y > drawn[i - 1].y);
  const css = readFileSync(new URL("../src/overlay/overlay.css", import.meta.url), "utf8");
  assert.match(css.match(/\.translation-card\s*\{[^}]*\}/)[0], /white-space:\s*pre-wrap/);
}

console.log("UI regression checks passed (lifecycle, cleanup, multiline display/copy/retry and Canvas).");
