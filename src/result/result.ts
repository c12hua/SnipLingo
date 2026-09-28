import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

interface TranslationPayload {
  original_text: string;
  translated_text: string;
}

interface SelectionRect {
  x: number;
  y: number;
  width: number;
  height: number;
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
let currentAppLang = "zh-CN";

async function loadI18n() {
  try {
    const config = await invoke<any>("get_config");
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
  } catch (e) {
    console.error("加载语言设置失败:", e);
  }
}
loadI18n();

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
      await invoke("close_result_window");
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
    btnErrorAction.onclick = async () => {
      if (currentOriginalText) {
        await executeRetry(currentOriginalText);
      } else if (lastRect) {
        await executeTranslation(lastRect);
      }
    };
  } else {
    errorTitle.textContent = currentAppLang === "en" ? "Error Occurred" : "处理异常";
    errorDesc.textContent = message || (currentAppLang === "en" ? "An unknown error occurred during image processing." : "图像处理发生未知异常。");
    btnErrorAction.textContent = currentAppLang === "en" ? "Close" : "关闭";
    btnErrorAction.onclick = async () => {
      await invoke("close_result_window");
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
}

btnToggleBreaks?.addEventListener("click", () => {
  isPreserveBreaks = !isPreserveBreaks;
  updateOriginalDisplay();
});

// 执行选区翻译
async function executeTranslation(rect: SelectionRect) {
  lastRect = rect;
  showState("loading");

  try {
    const res = await invoke<TranslationPayload>("translate_selection", { rect });
    rawOriginalText = res.original_text;
    isPreserveBreaks = rawOriginalText.includes("\n");
    updateOriginalDisplay();

    currentTranslatedText = res.translated_text;
    translatedTextEl.textContent = res.translated_text;
    showState("success");
  } catch (err: any) {
    renderError(err);
  }
}

// 原文重试翻译
async function executeRetry(text: string) {
  showState("loading");
  try {
    const res = await invoke<TranslationPayload>("retry_translate", { originalText: text });
    currentTranslatedText = res.translated_text;
    translatedTextEl.textContent = res.translated_text;
    showState("success");
  } catch (err: any) {
    renderError(err);
  }
}

// 监听来自后端的 OCR 渐进式就绪通知
listen<string>("ocr-ready", (event) => {
  rawOriginalText = event.payload;
  isPreserveBreaks = rawOriginalText.includes("\n");
  updateOriginalDisplay();

  // 切换为结构视图，原文立即呈现
  showState("success");
  const translatingMsg = currentAppLang === "en" ? "Translating..." : "翻译中...";
  translatedTextEl.innerHTML = `<span class="translation-skeleton"><span class="mini-spinner"></span> ${translatingMsg}</span>`;
});

// 监听来自 overlay 的翻译请求
listen<SelectionRect>("start-translate-flow", async (event) => {
  await executeTranslation(event.payload);
});

// 监听来自贴图右键翻译的异步结果
listen<TranslationPayload>("translation-result", (event) => {
  rawOriginalText = event.payload.original_text;
  isPreserveBreaks = rawOriginalText.includes("\n");
  updateOriginalDisplay();

  currentTranslatedText = event.payload.translated_text;
  translatedTextEl.textContent = event.payload.translated_text;
  showState("success");
});

listen<ErrorPayload>("translation-error", (event) => {
  renderError(event.payload);
});

// 复制译文
btnCopyTranslation.addEventListener("click", async () => {
  if (!currentTranslatedText) return;
  try {
    await navigator.clipboard.writeText(currentTranslatedText);
  } catch {
    // 后备方案
  }
  btnCopyTranslation.classList.add("copied");
  copyBtnText.textContent = currentAppLang === "en" ? "Copied" : "已复制";
  setTimeout(() => {
    btnCopyTranslation.classList.remove("copied");
    copyBtnText.textContent = currentAppLang === "en" ? "Copy Translation" : currentAppLang === "zh-TW" ? "複製譯文" : "复制译文";
  }, 1800);
});

// 复制原文
btnCopyOriginal.addEventListener("click", async () => {
  if (!currentOriginalText) return;
  try {
    await navigator.clipboard.writeText(currentOriginalText);
  } catch {
    // 后备方案
  }
  btnCopyOriginal.textContent = currentAppLang === "en" ? "Copied" : "已复制";
  setTimeout(() => {
    btnCopyOriginal.textContent = currentAppLang === "en" ? "Copy" : currentAppLang === "zh-TW" ? "複製" : "复制";
  }, 1800);
});

// 关闭窗口
btnClose.addEventListener("click", async () => {
  await invoke("close_result_window");
});

window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    await invoke("close_result_window");
  }
});
