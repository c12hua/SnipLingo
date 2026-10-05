import { invoke } from "@tauri-apps/api/core";
import { applyLanguage, Language, translations } from "./i18n";

interface AppConfig {
  app_language?: string;
  auto_start?: boolean;
  silent_start?: boolean;
  hotkey?: string;
  action_copy_shortcut?: string;
  action_ocr_shortcut?: string;
  action_qrcode_shortcut?: string;
  action_translate_shortcut?: string;
  action_pin_shortcut?: string;
  pin_shadow?: boolean;
  pin_opacity?: number;
  ocr_engine?: string;
  ocr_lang?: string;
  preserve_line_breaks?: boolean;
  enhance_contrast?: boolean;
  qrcode_open_in_browser?: boolean;
  in_place_translate?: boolean;
  provider: string;
  api_key: string;
  secret_key?: string;
  base_url?: string;
  model?: string;
  openai_api_key?: string;
  openai_base_url?: string;
  openai_model?: string;
  deepl_api_key?: string;
  baidu_app_id?: string;
  baidu_secret_key?: string;
  source_lang?: string;
  target_lang: string;
}

// 标志位：初始化加载配置期间，不触发自动保存
let isInitializing = true;

declare const __APP_VERSION__: string;
const brandVersionEl = document.querySelector(".brand-version");
if (brandVersionEl && typeof __APP_VERSION__ !== "undefined") {
  brandVersionEl.textContent = `v${__APP_VERSION__}`;
}

// 1. 侧边栏导航 Tab 切换
const navButtons = document.querySelectorAll<HTMLButtonElement>(".nav-item");
const tabPanels = document.querySelectorAll<HTMLElement>(".tab-panel");

navButtons.forEach((btn) => {
  btn.addEventListener("click", () => {
    const targetTabId = btn.dataset.tab;
    if (!targetTabId) return;

    navButtons.forEach((b) => b.classList.remove("active"));
    tabPanels.forEach((p) => p.classList.remove("active"));

    btn.classList.add("active");
    const activePanel = document.getElementById(targetTabId);
    if (activePanel) {
      activePanel.classList.add("active");
    }
  });
});

// 2. 常规面板 (General) DOM
const appLanguageSelect = document.getElementById("app-language-select") as HTMLSelectElement;
const autoStartCheckbox = document.getElementById("auto-start-checkbox") as HTMLInputElement;
const silentStartCheckbox = document.getElementById("silent-start-checkbox") as HTMLInputElement;
const btnResetGeneral = document.getElementById("btn-reset-general") as HTMLButtonElement;

// 3. 截屏动作快捷键 DOM
const actionCopyInput = document.getElementById("action-copy-input") as HTMLInputElement;
const btnResetActionCopy = document.getElementById("btn-reset-action-copy") as HTMLButtonElement;
const actionOcrInput = document.getElementById("action-ocr-input") as HTMLInputElement;
const btnResetActionOcr = document.getElementById("btn-reset-action-ocr") as HTMLButtonElement;
const actionQrCodeInput = document.getElementById("action-qrcode-input") as HTMLInputElement;
const btnResetActionQrCode = document.getElementById("btn-reset-action-qrcode") as HTMLButtonElement;
const actionTranslateInput = document.getElementById("action-translate-input") as HTMLInputElement;
const btnResetActionTranslate = document.getElementById("btn-reset-action-translate") as HTMLButtonElement;
const actionPinInput = document.getElementById("action-pin-input") as HTMLInputElement;
const btnResetActionPin = document.getElementById("btn-reset-action-pin") as HTMLButtonElement;
const btnResetCapture = document.getElementById("btn-reset-capture") as HTMLButtonElement;

// 4. 贴图面板 (Pin) DOM
const pinShadowCheckbox = document.getElementById("pin-shadow-checkbox") as HTMLInputElement;
const pinOpacitySlider = document.getElementById("pin-opacity-slider") as HTMLInputElement;
const pinOpacityBadge = document.getElementById("pin-opacity-badge") as HTMLSpanElement;
const btnResetPin = document.getElementById("btn-reset-pin") as HTMLButtonElement;

// 5. 全局快捷键 DOM
const hotkeyInput = document.getElementById("hotkey-input") as HTMLInputElement;
const btnResetHotkey = document.getElementById("btn-reset-hotkey") as HTMLButtonElement;
const btnResetHotkeys = document.getElementById("btn-reset-hotkeys") as HTMLButtonElement;

// 6. 翻译设置 DOM
const sourceLangSelect = document.getElementById("source-lang-select") as HTMLSelectElement;
const targetLangSelect = document.getElementById("target-lang-select") as HTMLSelectElement;
const inPlaceTranslateCheckbox = document.getElementById("in-place-translate-checkbox") as HTMLInputElement;
const providerSelect = document.getElementById("provider-select") as HTMLSelectElement;
const baiduFields = document.getElementById("baidu-fields") as HTMLDivElement;
const genericKeyItem = document.getElementById("generic-key-item") as HTMLDivElement;
const openaiExtraFields = document.getElementById("openai-extra-fields") as HTMLDivElement;

const apiKeyLabel = document.getElementById("api-key-label") as HTMLLabelElement;
const apiKeyInput = document.getElementById("api-key-input") as HTMLInputElement;
const btnToggleKey = document.getElementById("btn-toggle-key") as HTMLButtonElement;

const baiduAppIdInput = document.getElementById("baidu-appid-input") as HTMLInputElement;
const baiduSecretInput = document.getElementById("baidu-secret-input") as HTMLInputElement;
const btnToggleBaiduSecret = document.getElementById("btn-toggle-baidu-secret") as HTMLButtonElement;

const baseUrlInput = document.getElementById("base-url-input") as HTMLInputElement;
const modelInput = document.getElementById("model-input") as HTMLInputElement;
const btnTestApi = document.getElementById("btn-test-api") as HTMLButtonElement;
const testResultBox = document.getElementById("test-result-box") as HTMLDivElement;
const btnResetTranslation = document.getElementById("btn-reset-translation") as HTMLButtonElement;

// 7. OCR 设置 DOM
const ocrEngineSelect = document.getElementById("ocr-engine-select") as HTMLSelectElement;
const ocrLangSelect = document.getElementById("ocr-lang-select") as HTMLSelectElement;
const preserveLineBreaksCheckbox = document.getElementById("preserve-line-breaks-checkbox") as HTMLInputElement;
const enhanceContrastCheckbox = document.getElementById("enhance-contrast-checkbox") as HTMLInputElement;
const qrcodeModeSelect = document.getElementById("qrcode-mode-select") as HTMLSelectElement;
const btnResetOcr = document.getElementById("btn-reset-ocr") as HTMLButtonElement;

// 通用快捷键录制工具函数
function setupHotkeyRecorder(
  inputEl: HTMLInputElement,
  resetBtnEl: HTMLButtonElement,
  defaultKey: string,
  onKeyRecorded?: (key: string) => void
) {
  let isRecording = false;
  let savedKey = defaultKey;

  const startRecord = () => {
    isRecording = true;
    inputEl.classList.add("recording");
    inputEl.value = "按下快捷键...";
  };

  const cancelRecord = () => {
    if (isRecording) {
      isRecording = false;
      inputEl.classList.remove("recording");
      inputEl.value = savedKey;
    }
  };

  inputEl.addEventListener("focus", startRecord);
  inputEl.addEventListener("click", startRecord);
  inputEl.addEventListener("blur", () => {
    setTimeout(cancelRecord, 200);
  });

  inputEl.addEventListener("keydown", (e) => {
    if (!isRecording) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      cancelRecord();
      inputEl.blur();
      return;
    }

    const isPureModifier = ["Control", "Alt", "Shift", "Meta"].includes(e.key);
    if (isPureModifier) {
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      if (e.metaKey) parts.push("Super");
      inputEl.value = parts.length > 0 ? parts.join("+") + "+..." : "按下快捷键...";
      return;
    }

    let mainKey = "";
    if (/^F([1-9]|1[0-2])$/i.test(e.key)) {
      mainKey = e.key.toUpperCase();
    } else if (e.code.startsWith("Key")) {
      mainKey = e.code.replace("Key", "").toUpperCase();
    } else if (e.code.startsWith("Digit")) {
      mainKey = e.code.replace("Digit", "");
    } else if (e.key === "Enter" || e.code === "Enter") {
      mainKey = "Enter";
    } else if (e.key === " " || e.code === "Space") {
      mainKey = "Space";
    } else if (["Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown"].includes(e.key)) {
      mainKey = e.key;
    } else if (e.key.length === 1) {
      mainKey = e.key.toUpperCase();
    } else {
      mainKey = e.key;
    }

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push("Super");
    parts.push(mainKey);

    const combo = parts.join("+");
    savedKey = combo;
    inputEl.value = combo;
    isRecording = false;
    inputEl.classList.remove("recording");
    inputEl.blur();

    if (onKeyRecorded) {
      onKeyRecorded(combo);
    }
  });

  resetBtnEl.addEventListener("click", () => {
    savedKey = defaultKey;
    inputEl.value = defaultKey;
    inputEl.classList.remove("recording");
    if (onKeyRecorded) {
      onKeyRecorded(defaultKey);
    }
  });

  return {
    setKey: (k: string) => {
      savedKey = k;
      inputEl.value = k;
    },
    getKey: () => savedKey,
  };
}

// 初始化 4 个快捷键录制项
const hotkeyRecorder = setupHotkeyRecorder(hotkeyInput, btnResetHotkey, "F4", () => {
  if (!isInitializing) saveCurrentConfig("panel-hotkeys");
});
const copyShortcutRecorder = setupHotkeyRecorder(actionCopyInput, btnResetActionCopy, "Ctrl+C", () => {
  if (!isInitializing) saveCurrentConfig("panel-capture");
});
const ocrShortcutRecorder = setupHotkeyRecorder(actionOcrInput, btnResetActionOcr, "Ctrl+T", () => {
  if (!isInitializing) saveCurrentConfig("panel-capture");
});
const qrcodeShortcutRecorder = setupHotkeyRecorder(actionQrCodeInput, btnResetActionQrCode, "Ctrl+Q", () => {
  if (!isInitializing) saveCurrentConfig("panel-capture");
});
const translateShortcutRecorder = setupHotkeyRecorder(actionTranslateInput, btnResetActionTranslate, "Ctrl+S", () => {
  if (!isInitializing) saveCurrentConfig("panel-capture");
});
const pinShortcutRecorder = setupHotkeyRecorder(actionPinInput, btnResetActionPin, "Ctrl+P", () => {
  if (!isInitializing) saveCurrentConfig("panel-capture");
});

// 密码显示/隐藏切换
function setupPasswordToggle(btn: HTMLButtonElement, input: HTMLInputElement) {
  btn.addEventListener("click", () => {
    if (input.type === "password") {
      input.type = "text";
      btn.textContent = "🔒";
    } else {
      input.type = "password";
      btn.textContent = "👁";
    }
  });
}
setupPasswordToggle(btnToggleKey, apiKeyInput);
setupPasswordToggle(btnToggleBaiduSecret, baiduSecretInput);

// 多引擎独立凭据记忆缓存（确保切换服务商时不丢失已配置的凭据）
const providerState = {
  openai: {
    apiKey: "",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o-mini",
  },
  deepl: {
    apiKey: "",
  },
  baidu: {
    appId: "",
    secretKey: "",
  },
};

let lastSelectedProvider = "google";

function syncCurrentInputsToState(provider: string) {
  if (provider === "openai") {
    providerState.openai.apiKey = apiKeyInput.value.trim();
    providerState.openai.baseUrl = baseUrlInput.value.trim() || "https://api.openai.com/v1";
    providerState.openai.model = modelInput.value.trim() || "gpt-4o-mini";
  } else if (provider === "deepl") {
    providerState.deepl.apiKey = apiKeyInput.value.trim();
  } else if (provider === "baidu") {
    providerState.baidu.appId = baiduAppIdInput.value.trim();
    providerState.baidu.secretKey = baiduSecretInput.value.trim();
  }
}

function syncStateToUIInputs(provider: string) {
  if (provider === "openai") {
    apiKeyInput.value = providerState.openai.apiKey;
    baseUrlInput.value = providerState.openai.baseUrl || "https://api.openai.com/v1";
    modelInput.value = providerState.openai.model || "gpt-4o-mini";
  } else if (provider === "deepl") {
    apiKeyInput.value = providerState.deepl.apiKey;
  } else if (provider === "baidu") {
    baiduAppIdInput.value = providerState.baidu.appId;
    baiduSecretInput.value = providerState.baidu.secretKey;
  }
}

// 服务提供商字段联动
function updateProviderFields(provider: string) {
  baiduFields.classList.add("hidden");
  genericKeyItem.classList.add("hidden");
  openaiExtraFields.classList.add("hidden");

  if (provider === "google") {
    // 谷歌翻译免配置，无额外输入字段
  } else if (provider === "baidu") {
    baiduFields.classList.remove("hidden");
  } else if (provider === "deepl") {
    genericKeyItem.classList.remove("hidden");
    apiKeyLabel.textContent = "DeepL API 授权密钥 (Authentication Key)";
    apiKeyInput.placeholder = "例如 12345678-abcd-...:fx";
  } else if (provider === "openai") {
    genericKeyItem.classList.remove("hidden");
    openaiExtraFields.classList.remove("hidden");
    apiKeyLabel.textContent = "API Key";
    apiKeyInput.placeholder = "sk-...";
  }
}

providerSelect.addEventListener("change", () => {
  // 1. 先把当前面板填写的输入同步保存到原提供商的状态缓存中
  syncCurrentInputsToState(lastSelectedProvider);

  const newProvider = providerSelect.value;
  lastSelectedProvider = newProvider;

  // 2. 切换 UI 字段显示
  updateProviderFields(newProvider);

  // 3. 将新提供商之前保存的凭据回显到 UI 输入框中
  syncStateToUIInputs(newProvider);

  // 4. 自动持久化保存
  if (!isInitializing) saveCurrentConfig("panel-translation");
});

// 收集当前界面配置
function collectConfigFromUI(): AppConfig {
  const currentProvider = providerSelect.value;
  syncCurrentInputsToState(currentProvider);

  let activeApiKey = "";
  let activeSecretKey: string | undefined = undefined;
  let activeBaseUrl: string | undefined = undefined;
  let activeModel: string | undefined = undefined;

  if (currentProvider === "openai") {
    activeApiKey = providerState.openai.apiKey;
    activeBaseUrl = providerState.openai.baseUrl;
    activeModel = providerState.openai.model;
  } else if (currentProvider === "deepl") {
    activeApiKey = providerState.deepl.apiKey;
  } else if (currentProvider === "baidu") {
    activeApiKey = providerState.baidu.appId;
    activeSecretKey = providerState.baidu.secretKey;
  }

  return {
    app_language: appLanguageSelect.value,
    auto_start: autoStartCheckbox.checked,
    silent_start: silentStartCheckbox ? silentStartCheckbox.checked : true,
    hotkey: hotkeyRecorder.getKey(),
    action_copy_shortcut: copyShortcutRecorder.getKey(),
    action_ocr_shortcut: ocrShortcutRecorder.getKey(),
    action_qrcode_shortcut: qrcodeShortcutRecorder.getKey(),
    action_translate_shortcut: translateShortcutRecorder.getKey(),
    action_pin_shortcut: pinShortcutRecorder.getKey(),
    pin_shadow: pinShadowCheckbox.checked,
    pin_opacity: parseInt(pinOpacitySlider.value, 10) || 100,
    ocr_engine: ocrEngineSelect ? ocrEngineSelect.value : "windows",
    ocr_lang: ocrLangSelect.value,
    preserve_line_breaks: preserveLineBreaksCheckbox.checked,
    enhance_contrast: enhanceContrastCheckbox.checked,
    qrcode_open_in_browser: qrcodeModeSelect ? qrcodeModeSelect.value === "open" : false,
    in_place_translate: inPlaceTranslateCheckbox ? inPlaceTranslateCheckbox.checked : true,
    provider: currentProvider,
    api_key: activeApiKey,
    secret_key: activeSecretKey,
    base_url: activeBaseUrl,
    model: activeModel,
    openai_api_key: providerState.openai.apiKey,
    openai_base_url: providerState.openai.baseUrl,
    openai_model: providerState.openai.model,
    deepl_api_key: providerState.deepl.apiKey,
    baidu_app_id: providerState.baidu.appId,
    baidu_secret_key: providerState.baidu.secretKey,
    source_lang: sourceLangSelect ? sourceLangSelect.value : "auto",
    target_lang: targetLangSelect ? targetLangSelect.value : "zh-CN",
  };
}

// 保存配置并更新对应面板的反馈指示器
const feedbackTimeouts: Record<string, ReturnType<typeof setTimeout>> = {};

async function saveCurrentConfig(targetPanelId?: string, feedbackMsg?: string) {
  if (isInitializing) return;

  const currentLang = (appLanguageSelect.value as Language) || "zh-CN";
  const dict = translations[currentLang] || translations["zh-CN"];
  const msg = feedbackMsg || dict["status.saved"];

  const config = collectConfigFromUI();
  try {
    await invoke("save_config_cmd", { config });

    // 确定要在哪个分类面板下展示保存状态
    const panelId = targetPanelId || document.querySelector(".tab-panel.active")?.id;
    if (panelId) {
      const panel = document.getElementById(panelId);
      const statusEl = panel?.querySelector<HTMLElement>(".save-status");
      if (statusEl) {
        statusEl.textContent = msg;
        statusEl.classList.remove("hidden");

        if (feedbackTimeouts[panelId]) {
          clearTimeout(feedbackTimeouts[panelId]);
        }
        feedbackTimeouts[panelId] = setTimeout(() => {
          statusEl.classList.add("hidden");
        }, 1800);
      }
    }
  } catch (err) {
    console.error("保存配置失败:", err);
  }
}

// 防抖保存（针对滑动条和连续文本输入）
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
function debouncedSaveConfig(delay = 400, targetPanelId?: string) {
  if (isInitializing) return;
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(() => {
    saveCurrentConfig(targetPanelId);
  }, delay);
}

// 滑块值动态变化同步与防抖保存
pinOpacitySlider.addEventListener("input", () => {
  pinOpacityBadge.textContent = `${pinOpacitySlider.value}%`;
  debouncedSaveConfig(350, "panel-pin");
});

// 常规即时生效监听
appLanguageSelect.addEventListener("change", () => {
  applyLanguage(appLanguageSelect.value as Language);
  saveCurrentConfig("panel-general");
});
autoStartCheckbox.addEventListener("change", () => saveCurrentConfig("panel-general"));
silentStartCheckbox.addEventListener("change", () => saveCurrentConfig("panel-general"));

// 贴图即时生效监听
pinShadowCheckbox.addEventListener("change", () => saveCurrentConfig("panel-pin"));

// OCR 即时生效监听
if (ocrEngineSelect) {
  ocrEngineSelect.addEventListener("change", () => {
    saveCurrentConfig("panel-ocr");
  });
}
ocrLangSelect.addEventListener("change", () => saveCurrentConfig("panel-ocr"));
preserveLineBreaksCheckbox.addEventListener("change", () => saveCurrentConfig("panel-ocr"));
enhanceContrastCheckbox.addEventListener("change", () => saveCurrentConfig("panel-ocr"));
qrcodeModeSelect?.addEventListener("change", () => saveCurrentConfig("panel-ocr"));

// 翻译文本框与下拉选择即时保存监听
sourceLangSelect?.addEventListener("change", () => saveCurrentConfig("panel-translation"));
targetLangSelect?.addEventListener("change", () => saveCurrentConfig("panel-translation"));
inPlaceTranslateCheckbox?.addEventListener("change", () => saveCurrentConfig("panel-translation"));
apiKeyInput.addEventListener("input", () => debouncedSaveConfig(450, "panel-translation"));
baiduAppIdInput.addEventListener("input", () => debouncedSaveConfig(450, "panel-translation"));
baiduSecretInput.addEventListener("input", () => debouncedSaveConfig(450, "panel-translation"));
baseUrlInput.addEventListener("input", () => debouncedSaveConfig(450, "panel-translation"));
modelInput.addEventListener("input", () => debouncedSaveConfig(450, "panel-translation"));

// ========== 各大类“恢复默认”专属按钮 ==========
function getRestoredMsg(): string {
  const currentLang = (appLanguageSelect.value as Language) || "zh-CN";
  return translations[currentLang]?.["status.restored"] || "已恢复默认";
}

// 1. 常规恢复默认
btnResetGeneral.addEventListener("click", async () => {
  appLanguageSelect.value = "zh-CN";
  applyLanguage("zh-CN");
  autoStartCheckbox.checked = false;
  if (silentStartCheckbox) silentStartCheckbox.checked = true;
  await saveCurrentConfig("panel-general", getRestoredMsg());
});

// 2. 截屏动作快捷键恢复默认
btnResetCapture.addEventListener("click", async () => {
  copyShortcutRecorder.setKey("Ctrl+C");
  ocrShortcutRecorder.setKey("Ctrl+T");
  qrcodeShortcutRecorder.setKey("Ctrl+Q");
  translateShortcutRecorder.setKey("Ctrl+S");
  pinShortcutRecorder.setKey("Ctrl+P");
  await saveCurrentConfig("panel-capture", getRestoredMsg());
});

// 3. 贴图设置恢复默认
btnResetPin.addEventListener("click", async () => {
  pinShadowCheckbox.checked = false;
  pinOpacitySlider.value = "100";
  pinOpacityBadge.textContent = "100%";
  await saveCurrentConfig("panel-pin", getRestoredMsg());
});

// 4. 全局快捷键恢复默认
btnResetHotkeys.addEventListener("click", async () => {
  hotkeyRecorder.setKey("F4");
  await saveCurrentConfig("panel-hotkeys", getRestoredMsg());
});

// 5. 翻译配置恢复默认
btnResetTranslation.addEventListener("click", async () => {
  if (sourceLangSelect) {
    sourceLangSelect.value = "auto";
  }
  if (targetLangSelect) {
    targetLangSelect.value = "zh-CN";
  }
  if (inPlaceTranslateCheckbox) {
    inPlaceTranslateCheckbox.checked = true;
  }
  providerSelect.value = "google";
  lastSelectedProvider = "google";
  updateProviderFields("google");
  apiKeyInput.value = "";
  baiduAppIdInput.value = "";
  baiduSecretInput.value = "";
  baseUrlInput.value = "https://api.openai.com/v1";
  modelInput.value = "gpt-4o-mini";
  providerState.openai.apiKey = "";
  providerState.openai.baseUrl = "https://api.openai.com/v1";
  providerState.openai.model = "gpt-4o-mini";
  providerState.deepl.apiKey = "";
  providerState.baidu.appId = "";
  providerState.baidu.secretKey = "";
  await saveCurrentConfig("panel-translation", getRestoredMsg());
});

// 6. OCR 设置恢复默认
btnResetOcr.addEventListener("click", async () => {
  if (ocrEngineSelect) {
    ocrEngineSelect.value = "windows";
  }
  ocrLangSelect.value = "auto";
  preserveLineBreaksCheckbox.checked = false;
  enhanceContrastCheckbox.checked = true;
  if (qrcodeModeSelect) qrcodeModeSelect.value = "copy";
  await saveCurrentConfig("panel-ocr", getRestoredMsg());
});

// 一键测试连接
btnTestApi.addEventListener("click", async () => {
  const config = collectConfigFromUI();

  testResultBox.className = "test-result-box";
  testResultBox.textContent = "正在发送测试请求并验证连接...";
  testResultBox.classList.remove("hidden");
  btnTestApi.disabled = true;

  try {
    const result = await invoke<string>("test_api_connection", { config });
    testResultBox.className = "test-result-box success";
    testResultBox.textContent = `连接成功！测试结果: "${result}"`;
  } catch (err) {
    testResultBox.className = "test-result-box error";
    testResultBox.textContent = `连接失败: ${err}`;
  } finally {
    btnTestApi.disabled = false;
  }
});

// 页面初始化：读取后端持久化配置并回显
async function init() {
  isInitializing = true;

  try {
    const config = await invoke<AppConfig>("get_config");

    // 1. 常规配置与动态国际化语言应用
    const lang = (config.app_language as Language) || "zh-CN";
    appLanguageSelect.value = lang;
    applyLanguage(lang);
    autoStartCheckbox.checked = !!config.auto_start;
    if (silentStartCheckbox) {
      silentStartCheckbox.checked = config.silent_start !== false;
    }

    // 2. 截屏快捷键配置
    copyShortcutRecorder.setKey(config.action_copy_shortcut || "Ctrl+C");
    ocrShortcutRecorder.setKey(config.action_ocr_shortcut || "Ctrl+T");
    qrcodeShortcutRecorder.setKey(config.action_qrcode_shortcut || "Ctrl+Q");
    translateShortcutRecorder.setKey(config.action_translate_shortcut || "Ctrl+S");
    pinShortcutRecorder.setKey(config.action_pin_shortcut || "Ctrl+P");

    // 3. 贴图配置
    pinShadowCheckbox.checked = !!config.pin_shadow;
    const op = typeof config.pin_opacity === "number" ? config.pin_opacity : 100;
    pinOpacitySlider.value = op.toString();
    pinOpacityBadge.textContent = `${op}%`;

    // 4. 全局快捷键
    const mainHotkey = config.hotkey || "F4";
    hotkeyRecorder.setKey(mainHotkey);

    // 5. OCR 与排版设置
    if (ocrEngineSelect) {
      ocrEngineSelect.value = config.ocr_engine || "windows";
    }
    ocrLangSelect.value = config.ocr_lang || "auto";
    preserveLineBreaksCheckbox.checked = !!config.preserve_line_breaks;
    enhanceContrastCheckbox.checked = config.enhance_contrast !== false;
    if (qrcodeModeSelect) {
      qrcodeModeSelect.value = config.qrcode_open_in_browser ? "open" : "copy";
    }

    // 6. 翻译设置与多引擎独立凭据回显
    if (sourceLangSelect) {
      sourceLangSelect.value = config.source_lang || "auto";
    }
    if (targetLangSelect) {
      targetLangSelect.value = config.target_lang || "zh-CN";
    }
    if (inPlaceTranslateCheckbox) {
      inPlaceTranslateCheckbox.checked = config.in_place_translate !== false;
    }

    // 从持久化配置中还原各引擎凭据状态（优先独立字段，平滑兼容旧版通用字段）
    providerState.openai.apiKey = config.openai_api_key ?? (config.provider === "openai" ? config.api_key : "") ?? "";
    providerState.openai.baseUrl = config.openai_base_url || config.base_url || "https://api.openai.com/v1";
    providerState.openai.model = config.openai_model || config.model || "gpt-4o-mini";

    providerState.deepl.apiKey = config.deepl_api_key ?? (config.provider === "deepl" ? config.api_key : "") ?? "";

    providerState.baidu.appId = config.baidu_app_id ?? (config.provider === "baidu" ? config.api_key : "") ?? "";
    providerState.baidu.secretKey = config.baidu_secret_key ?? config.secret_key ?? "";

    const activeProv = config.provider || "google";
    providerSelect.value = activeProv;
    lastSelectedProvider = activeProv;

    updateProviderFields(activeProv);
    syncStateToUIInputs(activeProv);
  } catch (err) {
    console.error("加载配置文件失败:", err);
  } finally {
    // 延迟少许解除初始化锁，确保任何初始回显事件不会触发多余的保存
    setTimeout(() => {
      isInitializing = false;
    }, 100);
  }
}

init();
