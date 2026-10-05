import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface TranslationPayload {
  request_id: number;
  original_text: string;
  translated_text: string;
}

interface SelectionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface SelectionRequest {
  request_id: number;
  capture_id: number;
  rect: SelectionRect;
}

interface ErrorPayload {
  type: "CaptureFailed" | "NoTextDetected" | "MissingApiKey" | "NetworkError";
  message?: string;
}

const appWindow = getCurrentWebviewWindow();
// 不强制该窗口在最上层
appWindow.setAlwaysOnTop(false).catch(() => {});

const loadingState = document.getElementById("loading-state") as HTMLDivElement;
const successState = document.getElementById("success-state") as HTMLDivElement;
const errorState = document.getElementById("error-state") as HTMLDivElement;

const headerTitleText = document.getElementById("header-title-text") as HTMLSpanElement | null;
const sectionTitleOrig = document.getElementById("section-title-orig") as HTMLSpanElement | null;
const sectionTitleTrans = document.getElementById("section-title-trans") as HTMLSpanElement | null;
const loadingTextEl = document.getElementById("loading-text") as HTMLParagraphElement | null;

const originalTextEl = document.getElementById("original-text") as HTMLDivElement;
const translatedTextEl = document.getElementById("translated-text") as HTMLDivElement;
const copyBtnText = document.getElementById("copy-btn-text") as HTMLSpanElement;

const btnClose = document.getElementById("btn-close") as HTMLButtonElement;
const btnCopyOriginal = document.getElementById("btn-copy-original") as HTMLButtonElement;
const btnToggleBreaks = document.getElementById("btn-toggle-breaks") as HTMLButtonElement | null;
const btnCopyTranslation = document.getElementById("btn-copy-translation") as HTMLButtonElement;
const btnErrorAction = document.getElementById("btn-error-action") as HTMLButtonElement;

const zoomLevelText = document.getElementById("zoom-level-text") as HTMLSpanElement;
const btnZoomIn = document.getElementById("btn-zoom-in") as HTMLButtonElement;
const btnZoomOut = document.getElementById("btn-zoom-out") as HTMLButtonElement;

const errorTitle = document.getElementById("error-title") as HTMLHeadingElement;
const errorDesc = document.getElementById("error-desc") as HTMLParagraphElement;

let currentOriginalText = "";
let currentTranslatedText = "";
let rawOriginalText = "";
let isPreserveBreaks = false;
let lastRect: SelectionRect | null = null;
let lastCaptureId: number | null = null;
let currentAppLang = "zh-CN";
let latestRequestId = 0;
let closeGeneration = 0;
let requestOpen = false;
let translationFinished = false;
let originalCopyTimer: number | undefined;
let translationCopyTimer: number | undefined;

function isCurrentRequest(requestId: number) {
  return requestOpen && requestId === latestRequestId;
}

function resetResult() {
  currentOriginalText = "";
  currentTranslatedText = "";
  rawOriginalText = "";
  lastRect = null;
  lastCaptureId = null;
  isPreserveBreaks = false;
  translationFinished = false;
  originalTextEl.textContent = "";
  translatedTextEl.textContent = "";
  btnCopyOriginal.disabled = true;
  btnCopyTranslation.disabled = true;
  btnErrorAction.disabled = false;
  btnErrorAction.onclick = null;
  clearTimeout(originalCopyTimer);
  clearTimeout(translationCopyTimer);
  originalCopyTimer = undefined;
  translationCopyTimer = undefined;
  btnCopyTranslation.classList.remove("copied");
  copyBtnText.textContent = currentAppLang === "en" ? "Copy Translation" : currentAppLang === "zh-TW" ? "複製譯文" : "复制译文";
  btnCopyOriginal.textContent = currentAppLang === "en" ? "Copy" : currentAppLang === "zh-TW" ? "複製" : "复制";
  btnCopyOriginal.title = currentAppLang === "en" ? "Copy Original Text" : currentAppLang === "zh-TW" ? "複製原文" : "复制原文";
  btnCopyTranslation.title = currentAppLang === "en" ? "Copy Translated Text" : currentAppLang === "zh-TW" ? "複製譯文" : "复制译文";
}

function startRequest(requestId: number) {
  if (requestId <= latestRequestId) return isCurrentRequest(requestId);
  latestRequestId = requestId;
  requestOpen = true;
  resetResult();
  showState("loading");
  void loadI18n(requestId);
  return true;
}

async function loadI18n(requestId: number) {
  try {
    const config = await invoke<any>("get_config");
    if (!isCurrentRequest(requestId)) return;
    currentAppLang = config.app_language || "zh-CN";
    if (currentAppLang === "en") {
      if (headerTitleText) headerTitleText.textContent = "Side-by-Side Translation";
      if (sectionTitleOrig) sectionTitleOrig.textContent = "Original";
      if (sectionTitleTrans) sectionTitleTrans.textContent = "Translation";
      if (copyBtnText) copyBtnText.textContent = "Copy Translation";
      if (btnCopyOriginal) btnCopyOriginal.textContent = "Copy";
      if (btnCopyOriginal) btnCopyOriginal.title = "Copy Original Text";
      if (btnCopyTranslation) btnCopyTranslation.title = "Copy Translated Text";
      if (loadingTextEl) loadingTextEl.textContent = "Recognizing and translating...";
    } else if (currentAppLang === "zh-TW") {
      if (headerTitleText) headerTitleText.textContent = "對照翻譯";
      if (sectionTitleOrig) sectionTitleOrig.textContent = "原文";
      if (sectionTitleTrans) sectionTitleTrans.textContent = "譯文";
      if (copyBtnText) copyBtnText.textContent = "複製譯文";
      if (btnCopyOriginal) btnCopyOriginal.textContent = "複製";
      if (btnCopyOriginal) btnCopyOriginal.title = "複製原文";
      if (btnCopyTranslation) btnCopyTranslation.title = "複製譯文";
      if (loadingTextEl) loadingTextEl.textContent = "正在識別文字並翻譯中...";
    } else {
      if (headerTitleText) headerTitleText.textContent = "对照翻译";
      if (sectionTitleOrig) sectionTitleOrig.textContent = "原文";
      if (sectionTitleTrans) sectionTitleTrans.textContent = "译文";
      if (copyBtnText) copyBtnText.textContent = "复制译文";
      if (btnCopyOriginal) btnCopyOriginal.textContent = "复制";
      if (btnCopyOriginal) btnCopyOriginal.title = "复制原文";
      if (btnCopyTranslation) btnCopyTranslation.title = "复制译文";
      if (loadingTextEl) loadingTextEl.textContent = "正在识别文字并翻译中...";
    }
    if (rawOriginalText) updateOriginalDisplay();
  } catch (e) {
    if (isCurrentRequest(requestId)) console.error("加载语言设置失败:", e);
  }
}

// 字体无级缩放功能 (Ctrl + 滚轮 或 微调按钮)
let zoomPercent = parseInt(localStorage.getItem("sniplingo_font_zoom") || "100", 10);

function setZoom(percent: number) {
  zoomPercent = Math.max(70, Math.min(220, Math.round(percent)));
  zoomLevelText.textContent = `${zoomPercent}%`;
  document.documentElement.style.setProperty("--font-scale", `${zoomPercent / 100}`);
  localStorage.setItem("sniplingo_font_zoom", zoomPercent.toString());
}

// 初始化字体缩放比例
setZoom(zoomPercent);

btnZoomIn.addEventListener("click", () => setZoom(zoomPercent + 10));
btnZoomOut.addEventListener("click", () => setZoom(zoomPercent - 10));
zoomLevelText.addEventListener("click", () => setZoom(100));

// Ctrl + 鼠标滚轮进行无级缩放
window.addEventListener(
  "wheel",
  (e) => {
    if (e.ctrlKey) {
      e.preventDefault();
      const delta = e.deltaY < 0 ? 5 : -5;
      setZoom(zoomPercent + delta);
    }
  },
  { passive: false }
);

// 显示指定状态面板
function showState(state: "loading" | "success" | "error") {
  loadingState.classList.add("hidden");
  successState.classList.add("hidden");
  errorState.classList.add("hidden");

  if (state === "loading") loadingState.classList.remove("hidden");
  if (state === "success") successState.classList.remove("hidden");
  if (state === "error") errorState.classList.remove("hidden");
}

// 渲染错误信息
function renderError(err: ErrorPayload | any) {
  translationFinished = true;
  btnErrorAction.disabled = false;
  showState("error");
  const errType = err?.type || "NetworkError";
  const message = err?.message || (typeof err === "string" ? err : JSON.stringify(err));

  if (errType === "NoTextDetected") {
    errorTitle.textContent = currentAppLang === "en" ? "No Text Detected" : "未检测到有效文字";
    errorDesc.textContent = currentAppLang === "en"
      ? "No recognizable text found in selection. Please try selecting a different area."
      : "选区内未发现可识别的文字内容，请重新划选包含字符的区域。";
    btnErrorAction.textContent = currentAppLang === "en" ? "OK" : "知道了";
    btnErrorAction.onclick = async () => {
      await closeResult();
    };
  } else if (errType === "MissingApiKey") {
    errorTitle.textContent = currentAppLang === "en" ? "API Key Missing" : "尚未配置 API Key";
    errorDesc.textContent = message || (currentAppLang === "en" ? "Valid API Key not found, please configure it in Settings." : "未找到有效的翻译 API Key，请点击下方前往设置。");
    btnErrorAction.textContent = currentAppLang === "en" ? "Configure API Key" : "前往配置 API Key";
    btnErrorAction.onclick = async () => {
      await invoke("open_settings_window");
    };
  } else if (errType === "NetworkError") {
    errorTitle.textContent = currentAppLang === "en" ? "Network Connection Failed" : "网络连接失败";
    errorDesc.textContent = message || (currentAppLang === "en" ? "Unable to connect to translation API, please check network or proxy." : "无法连接至翻译接口，请检查您的网络连接或代理配置。");
    btnErrorAction.textContent = currentAppLang === "en" ? "Retry" : "重试翻译";
    btnErrorAction.onclick = retryTranslation;
  } else {
    errorTitle.textContent = currentAppLang === "en" ? "Error Occurred" : "处理异常";
    errorDesc.textContent = message || (currentAppLang === "en" ? "An unknown error occurred during image processing." : "图像处理发生未知异常。");
    btnErrorAction.textContent = currentAppLang === "en" ? "Close" : "关闭";
    btnErrorAction.onclick = async () => {
      await closeResult();
    };
  }
}

function updateOriginalDisplay() {
  if (isPreserveBreaks) {
    originalTextEl.textContent = rawOriginalText;
    currentOriginalText = rawOriginalText;
    if (btnToggleBreaks) {
      btnToggleBreaks.textContent = currentAppLang === "en" ? "Merge Paragraphs" : currentAppLang === "zh-TW" ? "合併段落" : "合并段落";
      btnToggleBreaks.title = currentAppLang === "en" ? "Currently line-by-line, click to merge paragraphs" : "当前为逐行保持换行，点击切换为合并段落";
    }
  } else {
    const merged = rawOriginalText
      .split("\n\n")
      .map((p) => p.split("\n").map((l) => l.trim()).filter(Boolean).join(" "))
      .join("\n\n");
    originalTextEl.textContent = merged;
    currentOriginalText = merged;
    if (btnToggleBreaks) {
      btnToggleBreaks.textContent = currentAppLang === "en" ? "Preserve Breaks" : currentAppLang === "zh-TW" ? "保持換行" : "保持换行";
      btnToggleBreaks.title = currentAppLang === "en" ? "Currently merged, click to preserve line breaks" : "当前为合并段落，点击切换为逐行换行";
    }
  }
  btnCopyOriginal.disabled = !requestOpen || !currentOriginalText;
}

btnToggleBreaks?.addEventListener("click", () => {
  isPreserveBreaks = !isPreserveBreaks;
  updateOriginalDisplay();
});

// 选区返回值与贴图事件共用结果呈现，旧请求不能覆盖新窗口。
function renderTranslation(res: TranslationPayload) {
  if (!isCurrentRequest(res.request_id)) return;
  translationFinished = true;
  if (!rawOriginalText) isPreserveBreaks = res.original_text.includes("\n");
  rawOriginalText = res.original_text;
  updateOriginalDisplay();
  currentTranslatedText = res.translated_text;
  translatedTextEl.textContent = res.translated_text;
  btnCopyTranslation.disabled = !currentTranslatedText;
  showState("success");
}

async function executeTranslation(request: SelectionRequest) {
  if (!startRequest(request.request_id)) return;
  lastRect = request.rect;
  lastCaptureId = request.capture_id;
  try {
    const res = await invoke<TranslationPayload>("translate_selection", {
      rect: request.rect,
      captureId: request.capture_id,
      requestId: request.request_id,
    });
    if (isCurrentRequest(request.request_id)) renderTranslation(res);
  } catch (err: any) {
    if (isCurrentRequest(request.request_id)) renderError(err);
  }
}

async function retryTranslation() {
  if (!requestOpen) return;
  const text = rawOriginalText;
  const preserveBreaks = isPreserveBreaks;
  const rect = lastRect;
  const captureId = lastCaptureId;
  if (!text && (!rect || captureId === null)) return;
  let requestId = latestRequestId;
  const closing = closeGeneration;
  btnErrorAction.disabled = true;
  showState("loading");
  try {
    requestId = await invoke<number>("show_result_window", text ? {} : { captureId });
    if (closing !== closeGeneration) {
      if (requestId >= latestRequestId) {
        latestRequestId = requestId;
        requestOpen = false;
        resetResult();
      }
      await invoke("close_result_window", { requestId });
      return;
    }
    if (!startRequest(requestId)) return;
    if (text) {
      rawOriginalText = text;
      isPreserveBreaks = preserveBreaks;
      updateOriginalDisplay();
      const res = await invoke<TranslationPayload>("retry_translate", { originalText: text, requestId });
      if (isCurrentRequest(requestId)) renderTranslation(res);
    } else if (rect && captureId !== null) {
      await executeTranslation({ request_id: requestId, capture_id: captureId, rect });
    }
  } catch (err: any) {
    if (isCurrentRequest(requestId)) renderError(err);
  }
}

listen<{ request_id: number }>("translation-started", (event) => {
  startRequest(event.payload.request_id);
});

listen<number>("translation-closed", (event) => {
  if (event.payload < latestRequestId) return;
  latestRequestId = event.payload;
  closeGeneration++;
  requestOpen = false;
  resetResult();
});

// OCR 原文先呈现，译文到齐前不可复制；迟到的 OCR 事件不能盖掉最终结果。
listen<{ request_id: number; text: string }>("ocr-ready", (event) => {
  if (!isCurrentRequest(event.payload.request_id)) return;
  if (translationFinished && currentOriginalText) return;
  if (!rawOriginalText) isPreserveBreaks = event.payload.text.includes("\n");
  rawOriginalText = event.payload.text;
  updateOriginalDisplay();
  // 错误可能先于 OCR 事件抵达：补存重试原文，但不覆盖错误界面。
  if (translationFinished) return;
  showState("success");
  const translatingMsg = currentAppLang === "en" ? "Translating..." : "翻译中...";
  translatedTextEl.innerHTML = `<span class="translation-skeleton"><span class="mini-spinner"></span> ${translatingMsg}</span>`;
});

listen<SelectionRequest>("start-translate-flow", async (event) => {
  await executeTranslation(event.payload);
});

listen<TranslationPayload>("translation-result", (event) => {
  renderTranslation(event.payload);
});

listen<{ request_id: number; error: ErrorPayload }>("translation-error", (event) => {
  if (isCurrentRequest(event.payload.request_id)) renderError(event.payload.error);
});

// 复制反馈只属于发起时的请求，失败不再显示“已复制”。
btnCopyTranslation.addEventListener("click", async () => {
  if (!requestOpen || !currentTranslatedText) return;
  const requestId = latestRequestId;
  clearTimeout(translationCopyTimer);
  try {
    await navigator.clipboard.writeText(currentTranslatedText);
    if (!isCurrentRequest(requestId)) return;
    btnCopyTranslation.classList.add("copied");
    btnCopyTranslation.title = currentAppLang === "en" ? "Copy Translated Text" : currentAppLang === "zh-TW" ? "複製譯文" : "复制译文";
    copyBtnText.textContent = currentAppLang === "en" ? "Copied" : currentAppLang === "zh-TW" ? "已複製" : "已复制";
    translationCopyTimer = window.setTimeout(() => {
      if (!isCurrentRequest(requestId)) return;
      btnCopyTranslation.classList.remove("copied");
      copyBtnText.textContent = currentAppLang === "en" ? "Copy Translation" : currentAppLang === "zh-TW" ? "複製譯文" : "复制译文";
    }, 1800);
  } catch (err) {
    if (!isCurrentRequest(requestId)) return;
    btnCopyTranslation.classList.remove("copied");
    copyBtnText.textContent = currentAppLang === "en" ? "Copy failed" : currentAppLang === "zh-TW" ? "複製失敗" : "复制失败";
    btnCopyTranslation.title = `${copyBtnText.textContent}: ${err}`;
  }
});

btnCopyOriginal.addEventListener("click", async () => {
  if (!requestOpen || !currentOriginalText) return;
  const requestId = latestRequestId;
  clearTimeout(originalCopyTimer);
  try {
    await navigator.clipboard.writeText(currentOriginalText);
    if (!isCurrentRequest(requestId)) return;
    btnCopyOriginal.title = currentAppLang === "en" ? "Copy Original Text" : currentAppLang === "zh-TW" ? "複製原文" : "复制原文";
    btnCopyOriginal.textContent = currentAppLang === "en" ? "Copied" : currentAppLang === "zh-TW" ? "已複製" : "已复制";
    originalCopyTimer = window.setTimeout(() => {
      if (!isCurrentRequest(requestId)) return;
      btnCopyOriginal.textContent = currentAppLang === "en" ? "Copy" : currentAppLang === "zh-TW" ? "複製" : "复制";
    }, 1800);
  } catch (err) {
    if (!isCurrentRequest(requestId)) return;
    btnCopyOriginal.textContent = currentAppLang === "en" ? "Copy failed" : currentAppLang === "zh-TW" ? "複製失敗" : "复制失败";
    btnCopyOriginal.title = `${btnCopyOriginal.textContent}: ${err}`;
  }
});

async function closeResult() {
  const requestId = latestRequestId;
  if (!requestId) return;
  closeGeneration++;
  requestOpen = false;
  resetResult();
  try {
    await invoke("close_result_window", { requestId });
  } catch (err) {
    if (latestRequestId === requestId) {
      renderError({ type: "CaptureFailed", message: String(err) });
    }
  }
}

btnClose.addEventListener("click", closeResult);
window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") await closeResult();
});
