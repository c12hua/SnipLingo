import { emit, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface CapturePayload {
  width: number;
  height: number;
  scale_factor: number;
}

type InteractionMode = "idle" | "drawing" | "moving" | "resizing";

interface OcrRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface InPlaceBlock {
  id: number;
  original_text: string;
  translated_text: string;
  rect: OcrRect;
  font_size_px: number;
  bg_color: string;
  text_color: string;
}

interface InPlaceTranslationResult {
  blocks: InPlaceBlock[];
  image_data: string;
  width: number;
  height: number;
  scale_factor: number;
}

const captureContainer = document.getElementById("capture-container") as HTMLDivElement;
const maskLayer = document.getElementById("mask-layer") as HTMLDivElement;
const selectionBox = document.getElementById("selection-box") as HTMLDivElement;
const dimensionBadge = document.getElementById("dimension-badge") as HTMLDivElement;
const translationLayer = document.getElementById("translation-layer") as HTMLDivElement;

const toolbar = document.getElementById("toolbar") as HTMLDivElement;
const toolbarNormal = document.getElementById("toolbar-normal") as HTMLDivElement;
const toolbarTranslate = document.getElementById("toolbar-translate") as HTMLDivElement;

const btnCopy = document.getElementById("btn-copy") as HTMLButtonElement;
const btnOcr = document.getElementById("btn-ocr") as HTMLButtonElement;
const btnPin = document.getElementById("btn-pin") as HTMLButtonElement;
const btnTranslate = document.getElementById("btn-translate") as HTMLButtonElement;
const btnSave = document.getElementById("btn-save") as HTMLButtonElement | null;
const btnCancel = document.getElementById("btn-cancel") as HTMLButtonElement;

const btnToggleView = document.getElementById("btn-toggle-view") as HTMLButtonElement;
const btnCopyTransImg = document.getElementById("btn-copy-trans-img") as HTMLButtonElement;
const btnCopyOrigImg = document.getElementById("btn-copy-orig-img") as HTMLButtonElement;
const btnCopyTransText = document.getElementById("btn-copy-trans-text") as HTMLButtonElement;
const btnTransPin = document.getElementById("btn-trans-pin") as HTMLButtonElement;
const btnTransSave = document.getElementById("btn-trans-save") as HTMLButtonElement | null;
const btnTransCancel = document.getElementById("btn-trans-cancel") as HTMLButtonElement;

const TRANSLATE_ICON_HTML = `<svg class="tb-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-label="翻译"><circle cx="12" cy="12" r="8.5"/><path d="M3.7 12h16.6M12 3.5c-2.4 2.2-3.5 5-3.5 8.5s1.1 6.3 3.5 8.5c2.4-2.2 3.5-5 3.5-8.5s-1.1-6.3-3.5-8.5Z"/></svg>`;

function updateBtnTag(btn: HTMLButtonElement, tagText: string) {
  const tagEl = btn.querySelector<HTMLElement>(".tb-tag");
  if (tagEl) tagEl.textContent = tagText;
}

const selectionCopyBubble = document.getElementById("selection-copy-bubble") as HTMLDivElement | null;
const btnBubbleCopy = document.getElementById("btn-bubble-copy") as HTMLButtonElement | null;

const toastEl = document.getElementById("toast") as HTMLDivElement;
const btnForceExit = document.getElementById("btn-force-exit") as HTMLButtonElement | null;

let scaleFactor = 1;
let mode: InteractionMode = "idle";
let activeHandle: string = "";

let startX = 0;
let startY = 0;
let dragMouseStartX = 0;
let dragMouseStartY = 0;
let initialRect = { x: 0, y: 0, w: 0, h: 0 };
let currentRect = { x: 0, y: 0, w: 0, h: 0 };
let toastTimeout: number | undefined;

let currentTranslationResult: InPlaceTranslationResult | null = null;
let isShowingTranslation = true;
let isTranslatingInPlace = false;
let currentAppLang = "zh-CN";

function hideSelectionBubble() {
  if (selectionCopyBubble) {
    selectionCopyBubble.classList.add("hidden");
  }
}

function dismissTextSelection() {
  const selection = window.getSelection();
  if (selection && selection.rangeCount > 0) {
    selection.removeAllRanges();
  }
  hideSelectionBubble();
}

function showSelectionBubble(x: number, y: number) {
  if (!selectionCopyBubble) return;
  selectionCopyBubble.style.left = `${x}px`;
  selectionCopyBubble.style.top = `${y}px`;
  selectionCopyBubble.classList.remove("hidden");
}

function showToast(msg: string, type: "info" | "warning" | "success" = "info", duration = 2500) {
  if (toastTimeout) clearTimeout(toastTimeout);
  toastEl.textContent = msg;
  toastEl.className = `toast ${type}`;
  toastTimeout = window.setTimeout(() => {
    toastEl.className = "toast hidden";
  }, duration);
}

// 监听 Rust 后端截屏事件
listen<CapturePayload>("screenshot-captured", (event) => {
  const payload = event.payload;
  scaleFactor = payload.scale_factor || 1;

  // 加载并同步最新动作快捷键
  loadActionShortcuts();

  // 极速重置交互状态
  resetSelection();

  // 确保视口就绪后立即无缝呈现，彻底消除任何阶段性闪烁展开现象
  requestAnimationFrame(() => {
    if (captureContainer) {
      captureContainer.classList.add("ready");
    }
  });
});

function clearTranslationLayer() {
  currentTranslationResult = null;
  isShowingTranslation = true;
  isTranslatingInPlace = false;
  hideSelectionBubble();
  if (translationLayer) {
    translationLayer.innerHTML = "";
    translationLayer.classList.add("hidden");
  }
  if (toolbarNormal && toolbarTranslate) {
    toolbarTranslate.classList.add("hidden");
    toolbarNormal.classList.remove("hidden");
  }
  if (btnTranslate) {
    btnTranslate.disabled = false;
    btnTranslate.innerHTML = TRANSLATE_ICON_HTML;
    btnTranslate.title = getTranslateBtnTitle();
  }
}

function resetSelection() {
  mode = "idle";
  activeHandle = "";
  currentRect = { x: 0, y: 0, w: 0, h: 0 };
  clearTranslationLayer();
  selectionBox.classList.remove("drawing");
  selectionBox.classList.add("hidden");
  toolbar.classList.add("hidden");
  maskLayer.classList.remove("has-selection");
}

async function closeOverlay() {
  if (captureContainer) {
    captureContainer.classList.remove("ready");
  }
  resetSelection();
  await invoke("close_capture");
}

async function cancelOverlay() {
  if (captureContainer) {
    captureContainer.classList.remove("ready");
  }
  resetSelection();
  await invoke("cancel_capture");
}

function updateSelectionDOM(x: number, y: number, w: number, h: number) {
  selectionBox.style.left = `${x}px`;
  selectionBox.style.top = `${y}px`;
  selectionBox.style.width = `${w}px`;
  selectionBox.style.height = `${h}px`;

  // 物理像素尺寸提示
  const physW = Math.round(w * scaleFactor);
  const physH = Math.round(h * scaleFactor);
  dimensionBadge.textContent = `${physW} × ${physH} px`;

  // 避免尺寸徽标超出顶部屏幕
  if (y < 28) {
    dimensionBadge.style.top = "6px";
  } else {
    dimensionBadge.style.top = "-26px";
  }
}

function positionToolbar(x: number, y: number, w: number, h: number) {
  if (w < 10 || h < 10) {
    toolbar.classList.add("hidden");
    return;
  }

  toolbar.classList.remove("hidden");
  const tbW = toolbar.offsetWidth || 300;
  const tbH = toolbar.offsetHeight || 42;

  let tbX = x + w - tbW;
  let tbY = y + h + 10;

  // 边缘自适应避让
  if (tbX < 10) tbX = 10;
  if (tbX + tbW > window.innerWidth - 10) tbX = window.innerWidth - tbW - 10;

  if (tbY + tbH > window.innerHeight - 10) {
    // 底部空间不够，放置在选区内部下方或上方
    tbY = y - tbH - 10;
    if (tbY < 10) {
      tbY = y + h - tbH - 10; // 选区内右下角
    }
  }

  toolbar.style.left = `${tbX}px`;
  toolbar.style.top = `${tbY}px`;
}

// 鼠标按下：
// - 鼠标右键：取消选区或退出截图
// - 鼠标左键：拖拽手柄、平移选区或新建选区
window.addEventListener("mousedown", async (e) => {
  // 鼠标右键 (button === 2)
  if (e.button === 2) {
    const selectedText = window.getSelection()?.toString();
    if (selectedText && selectedText.trim().length > 0) {
      e.preventDefault();
      try {
        await navigator.clipboard.writeText(selectedText);
        const preview = selectedText.trim().length > 20
          ? selectedText.trim().slice(0, 20) + "..."
          : selectedText.trim();
        showToast(
          currentAppLang === "en" ? `Text copied: "${preview}"` : `已复制选中文本: "${preview}"`,
          "success",
          1500
        );
      } catch (err) {
        console.error("复制选中文本失败:", err);
      }
      dismissTextSelection();
      return;
    }

    e.preventDefault();
    if (currentRect.w >= 10 && currentRect.h >= 10 && !selectionBox.classList.contains("hidden")) {
      resetSelection();
    } else {
      await cancelOverlay();
    }
    return;
  }

  // 仅响应鼠标左键 (button === 0)
  if (e.button !== 0) return;

  if (
    toolbar.contains(e.target as Node) ||
    toastEl.contains(e.target as Node) ||
    (selectionCopyBubble && selectionCopyBubble.contains(e.target as Node)) ||
    (btnForceExit && btnForceExit.contains(e.target as Node))
  ) {
    return;
  }

  const target = e.target as HTMLElement;

  // 1. 检查是否点击了 8 方向拉伸手柄
  const handleEl = target.closest(".resize-handle") as HTMLElement | null;
  if (handleEl && !selectionBox.classList.contains("hidden")) {
    dismissTextSelection();
    if (currentTranslationResult) {
      clearTranslationLayer();
    }
    mode = "resizing";
    activeHandle = handleEl.dataset.handle || "";
    dragMouseStartX = e.clientX;
    dragMouseStartY = e.clientY;
    initialRect = { ...currentRect };
    toolbar.classList.add("hidden");
    return;
  }

  // 2. 检查是否点击在已生成的选区内部（整体平移）
  if (
    selectionBox.contains(target) &&
    !selectionBox.classList.contains("hidden") &&
    currentRect.w >= 10 &&
    currentRect.h >= 10
  ) {
    if (target.closest(".translation-card")) {
      // 允许用户选中/复制卡片中的文本，不打断平移交互
      return;
    }
    if (currentTranslationResult) {
      clearTranslationLayer();
    }
    mode = "moving";
    dragMouseStartX = e.clientX;
    dragMouseStartY = e.clientY;
    initialRect = { ...currentRect };
    toolbar.classList.add("hidden");
    return;
  }

  // 3. 点击在选区外部（底图/蒙版），开始绘制全新的选区
  if (currentTranslationResult) {
    clearTranslationLayer();
  }
  mode = "drawing";
  startX = e.clientX;
  startY = e.clientY;
  currentRect = { x: startX, y: startY, w: 0, h: 0 };

  toolbar.classList.add("hidden");
  maskLayer.classList.add("has-selection");
  selectionBox.classList.add("drawing");
  selectionBox.classList.remove("hidden");
  updateSelectionDOM(startX, startY, 0, 0);
});

// 鼠标移动：根据当前状态分别处理
window.addEventListener("mousemove", (e) => {
  if (mode === "idle") return;

  const curX = e.clientX;
  const curY = e.clientY;

  if (mode === "drawing") {
    const x = Math.min(startX, curX);
    const y = Math.min(startY, curY);
    const w = Math.min(window.innerWidth - x, Math.abs(curX - startX));
    const h = Math.min(window.innerHeight - y, Math.abs(curY - startY));

    currentRect = { x, y, w, h };
    updateSelectionDOM(x, y, w, h);
  } else if (mode === "moving") {
    const dx = curX - dragMouseStartX;
    const dy = curY - dragMouseStartY;

    let newX = initialRect.x + dx;
    let newY = initialRect.y + dy;

    // 边界严密约束在屏幕可视区域内
    newX = Math.max(0, Math.min(window.innerWidth - initialRect.w, newX));
    newY = Math.max(0, Math.min(window.innerHeight - initialRect.h, newY));

    currentRect = { x: newX, y: newY, w: initialRect.w, h: initialRect.h };
    updateSelectionDOM(newX, newY, initialRect.w, initialRect.h);
  } else if (mode === "resizing") {
    const dx = curX - dragMouseStartX;
    const dy = curY - dragMouseStartY;

    let left = initialRect.x;
    let right = initialRect.x + initialRect.w;
    let top = initialRect.y;
    let bottom = initialRect.y + initialRect.h;

    // 按手柄方向调整对应边界，最小宽度/高度保证为 10px
    if (activeHandle.includes("w")) {
      left = Math.max(0, Math.min(right - 10, initialRect.x + dx));
    }
    if (activeHandle.includes("e")) {
      right = Math.min(window.innerWidth, Math.max(left + 10, initialRect.x + initialRect.w + dx));
    }
    if (activeHandle.includes("n")) {
      top = Math.max(0, Math.min(bottom - 10, initialRect.y + dy));
    }
    if (activeHandle.includes("s")) {
      bottom = Math.min(window.innerHeight, Math.max(top + 10, initialRect.y + initialRect.h + dy));
    }

    currentRect = {
      x: left,
      y: top,
      w: right - left,
      h: bottom - top,
    };
    updateSelectionDOM(currentRect.x, currentRect.y, currentRect.w, currentRect.h);
  }
});

// 鼠标松开：结束当前操作，停靠悬浮工具栏
window.addEventListener("mouseup", (e) => {
  if (e.button !== 0) return;
  if (mode === "idle") return;

  const prevMode = mode;
  mode = "idle";
  selectionBox.classList.remove("drawing");

  if (prevMode === "drawing" && (currentRect.w < 10 || currentRect.h < 10)) {
    // 选区过小视作误触，重置
    resetSelection();
    return;
  }

  positionToolbar(currentRect.x, currentRect.y, currentRect.w, currentRect.h);
});

// 右键快捷菜单拦截：有选中文本时复制，无选区时直接退出截图
window.addEventListener("contextmenu", async (e) => {
  e.preventDefault();
  const selectedText = window.getSelection()?.toString();
  if (selectedText && selectedText.trim().length > 0) {
    try {
      await navigator.clipboard.writeText(selectedText);
      const preview = selectedText.trim().length > 20
        ? selectedText.trim().slice(0, 20) + "..."
        : selectedText.trim();
      showToast(
        currentAppLang === "en" ? `Text copied: "${preview}"` : `已复制选中文本: "${preview}"`,
        "success",
        1500
      );
    } catch {}
    hideSelectionBubble();
    return;
  }
  if (currentRect.w >= 10 && currentRect.h >= 10 && !selectionBox.classList.contains("hidden")) {
    resetSelection();
  } else {
    await cancelOverlay();
  }
});

// 双击选区内部：快捷触发翻译 (双击卡片文字除外，允许双击选词)
selectionBox.addEventListener("dblclick", async (e) => {
  if (toolbar.contains(e.target as Node)) return;
  if ((e.target as HTMLElement).closest(".translation-card")) return;
  e.stopPropagation();
  await triggerTranslate();
});

// 双击空白背景：退出截图
maskLayer.addEventListener("dblclick", async (e) => {
  if (e.target === maskLayer) {
    await cancelOverlay();
  }
});

// 点击右上角退出胶囊
btnForceExit?.addEventListener("click", async (e) => {
  e.stopPropagation();
  await cancelOverlay();
});

// 触发复制到剪贴板流程
async function triggerCopy() {
  if (currentRect.w < 10 || currentRect.h < 10) return;

  if (currentTranslationResult?.image_data) {
    try {
      await invoke("copy_translated_image_cmd", { dataUrl: currentTranslationResult.image_data });
      showToast("原图已复制到剪贴板", "success", 1200);
      setTimeout(async () => {
        await closeOverlay();
      }, 300);
    } catch (err) {
      console.error("复制图片到剪贴板失败:", err);
      showToast("复制图片失败: " + err, "warning");
      setTimeout(async () => {
        await closeOverlay();
      }, 1500);
    }
    return;
  }

  const rect = {
    x: Math.round(currentRect.x * scaleFactor),
    y: Math.round(currentRect.y * scaleFactor),
    width: Math.round(currentRect.w * scaleFactor),
    height: Math.round(currentRect.h * scaleFactor),
  };

  try {
    await invoke("copy_selection_to_clipboard", { rect });
    showToast("图片已复制到剪贴板", "success", 1200);
    setTimeout(async () => {
      await closeOverlay();
    }, 400);
  } catch (err) {
    console.error("复制图片到剪贴板失败:", err);
    showToast("复制图片失败: " + err, "warning");
    setTimeout(async () => {
      await closeOverlay();
    }, 1500);
  }
}

// 触发获取文本
async function triggerOcrText() {
  if (currentRect.w < 10 || currentRect.h < 10) return;

  const rect = {
    x: Math.round(currentRect.x * scaleFactor),
    y: Math.round(currentRect.y * scaleFactor),
    width: Math.round(currentRect.w * scaleFactor),
    height: Math.round(currentRect.h * scaleFactor),
  };

  showToast("正在提取文本...", "info", 3000);

  try {
    const text = await invoke<string>("extract_text_from_selection", { rect });
    const cleanOneLine = text.trim().replace(/\s+/g, " ");
    const preview = cleanOneLine.length > 20 ? cleanOneLine.slice(0, 20) + "..." : cleanOneLine;
    showToast(`文本已复制到剪贴板: "${preview}" (${text.length}字) `, "success", 1500);
    setTimeout(async () => {
      await closeOverlay();
    }, 450);
  } catch (err: any) {
    console.error("获取文本失败:", err);
    showToast("未检测到有效文字", "warning", 2000);
  }
}

// 触发翻译流程
async function triggerTranslate() {
  if (currentRect.w < 10 || currentRect.h < 10) return;
  if (isTranslatingInPlace) return;

  const rect = {
    x: Math.round(currentRect.x * scaleFactor),
    y: Math.round(currentRect.y * scaleFactor),
    width: Math.round(currentRect.w * scaleFactor),
    height: Math.round(currentRect.h * scaleFactor),
  };

  const centerLogX = currentRect.x + currentRect.w / 2;
  const centerLogY = currentRect.y + currentRect.h / 2;

  // 读取配置，判断是否开启直接就地覆盖显示译文 (in_place_translate)
  let inPlace = true;
  try {
    const config = await invoke<any>("get_config");
    if (config && config.in_place_translate === false) {
      inPlace = false;
    }
  } catch (err) {
    console.error("读取配置失败:", err);
  }

  if (!inPlace) {
    // 弹窗显示模式 (关闭全屏选区，弹出结果窗口)
    await closeOverlay();
    const posX = Math.round(centerLogX * scaleFactor - 230);
    const posY = Math.round(centerLogY * scaleFactor - 180);
    await invoke("show_result_window", {
      x: Math.max(20, posX),
      y: Math.max(20, posY),
    });
    await emit("start-translate-flow", rect);
    return;
  }

  // 就地截图翻译模式
  try {
    isTranslatingInPlace = true;
    btnTranslate.disabled = true;
    btnTranslate.innerHTML = `<span class="tb-spinner"></span>`;
    btnTranslate.title = currentAppLang === "en" ? "Translating..." : "正在翻译中...";
    showToast(currentAppLang === "en" ? "Recognizing and translating..." : "正在识别文字与翻译...", "info", 5000);

    const result = await invoke<InPlaceTranslationResult>("translate_in_place", { rect });
    currentTranslationResult = result;
    isShowingTranslation = true;

    // 渲染覆盖译文卡片
    renderInPlaceBlocks(result);

    // 切换至译文操作工具条
    toolbarNormal.classList.add("hidden");
    toolbarTranslate.classList.remove("hidden");
    updateToggleViewBtnText();
    requestAnimationFrame(() => {
      positionToolbar(currentRect.x, currentRect.y, currentRect.w, currentRect.h);
    });

    showToast(currentAppLang === "en" ? "Translation complete" : "翻译完成", "success", 1200);
  } catch (err: any) {
    console.error("就地翻译失败:", err);
    const msg = err?.message || (typeof err === "string" ? err : "未检测到有效文字");
    showToast(msg, "warning", 2500);
  } finally {
    isTranslatingInPlace = false;
    btnTranslate.disabled = false;
    btnTranslate.innerHTML = TRANSLATE_ICON_HTML;
    btnTranslate.title = getTranslateBtnTitle();
  }
}

// 渲染覆盖在截图选区上的译文卡片
function renderInPlaceBlocks(result: InPlaceTranslationResult) {
  translationLayer.innerHTML = "";
  translationLayer.classList.remove("hidden");

  for (const block of result.blocks) {
    const card = document.createElement("div");
    card.className = "translation-card";

    // 物理坐标换算到逻辑像素
    const logX = block.rect.x / scaleFactor;
    const logY = block.rect.y / scaleFactor;
    const logW = block.rect.width / scaleFactor;
    const logH = block.rect.height / scaleFactor;
    const logFontSize = Math.max(11, Math.min(36, Math.round(block.font_size_px / scaleFactor)));

    card.style.left = `${logX}px`;
    card.style.top = `${logY}px`;
    card.style.minWidth = `${logW}px`;
    card.style.maxWidth = `${Math.max(logW * 1.5, currentRect.w - logX)}px`;
    card.style.minHeight = `${logH}px`;
    card.style.backgroundColor = block.bg_color;
    card.style.color = block.text_color;
    card.style.fontSize = `${logFontSize}px`;
    card.textContent = block.translated_text;
    card.title = currentAppLang === "en"
      ? "Highlight text to copy (Ctrl+C)"
      : "划选文字后按 Ctrl+C 即可复制选中文本";

    translationLayer.appendChild(card);
  }
}

// 使用 HTML5 Canvas 合成底层原图与译文卡片
async function generateTranslatedImageDataUrl(result: InPlaceTranslationResult): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = result.width;
  canvas.height = result.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("无法创建 Canvas 2D 上下文");

  // 1. 绘制底层原图
  const img = new Image();
  img.src = result.image_data;
  if (!img.complete) {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (img.complete) resolve();
        else reject(new Error("底图加载超时"));
      }, 3000);
      img.onload = () => {
        clearTimeout(timer);
        resolve();
      };
      img.onerror = (e) => {
        clearTimeout(timer);
        reject(new Error("底图解码失败: " + e));
      };
    });
  }
  ctx.drawImage(img, 0, 0);

  // 2. 绘制每个译文卡片
  for (const block of result.blocks) {
    const rx = block.rect.x;
    const ry = block.rect.y;
    const rw = block.rect.width;
    const rh = block.rect.height;

    // 绘制卡片背景矩形
    ctx.fillStyle = block.bg_color;
    const radius = 4;
    ctx.beginPath();
    if (typeof (ctx as any).roundRect === "function") {
      (ctx as any).roundRect(rx, ry, rw, rh, radius);
    } else {
      ctx.rect(rx, ry, rw, rh);
    }
    ctx.fill();

    // 绘制译文文本
    ctx.fillStyle = block.text_color;
    const fontSize = Math.max(12, Math.round(block.font_size_px));
    ctx.font = `${fontSize}px -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif`;
    ctx.textBaseline = "top";

    const padding = 4;
    const maxTextW = Math.max(rw - padding * 2, fontSize);
    const lineHeight = fontSize * 1.35;
    const text = block.translated_text;

    let curLine = "";
    let curY = ry + padding;

    for (let i = 0; i < text.length; i++) {
      const testLine = curLine + text[i];
      const metrics = ctx.measureText(testLine);
      if (metrics.width > maxTextW && curLine.length > 0) {
        ctx.fillText(curLine, rx + padding, curY);
        curLine = text[i];
        curY += lineHeight;
      } else {
        curLine = testLine;
      }
    }
    if (curLine.length > 0) {
      ctx.fillText(curLine, rx + padding, curY);
    }
  }

  return canvas.toDataURL("image/png");
}

function updateToggleViewBtnText() {
  if (isShowingTranslation) {
    btnToggleView.classList.remove("active");
    if (currentAppLang === "en") {
      btnToggleView.title = "Show Original";
      btnCopyTransText.title = "Copy Translated Text";
    } else if (currentAppLang === "zh-TW") {
      btnToggleView.title = "顯示原圖";
      btnCopyTransText.title = "複製譯文文字";
    } else {
      btnToggleView.title = "显示原图";
      btnCopyTransText.title = "复制译文文本";
    }
  } else {
    btnToggleView.classList.add("active");
    if (currentAppLang === "en") {
      btnToggleView.title = "Show Translation";
      btnCopyTransText.title = "Copy Original Text";
    } else if (currentAppLang === "zh-TW") {
      btnToggleView.title = "顯示譯圖";
      btnCopyTransText.title = "複製原文文字";
    } else {
      btnToggleView.title = "显示译图";
      btnCopyTransText.title = "复制原文文本";
    }
  }
}

// 触发钉在桌面 (贴图) 流程
async function triggerPin() {
  if (currentRect.w < 10 || currentRect.h < 10) return;

  const rect = {
    x: Math.round(currentRect.x * scaleFactor),
    y: Math.round(currentRect.y * scaleFactor),
    width: Math.round(currentRect.w * scaleFactor),
    height: Math.round(currentRect.h * scaleFactor),
  };

  await closeOverlay();

  try {
    await invoke("pin_screenshot", { rect });
  } catch (err) {
    console.error("钉住截图失败:", err);
  }
}

// 触发保存选区图片到本地文件流程 (点击保存后遮罩立即退出让出桌面，若取消则自动恢复现场)
async function triggerSave() {
  if (currentRect.w < 10 || currentRect.h < 10) return;

  const rect = {
    x: Math.round(currentRect.x * scaleFactor),
    y: Math.round(currentRect.y * scaleFactor),
    width: Math.round(currentRect.w * scaleFactor),
    height: Math.round(currentRect.h * scaleFactor),
  };

  try {
    const savedPath = await invoke<string | null>("save_selection_to_file", { rect });
    if (savedPath) {
      // 成功保存：由于遮罩已在后端平滑隐藏，前端直接重置选区状态完成本次会话
      resetSelection();
    }
  } catch (err: any) {
    console.error("保存图片失败:", err);
    const failMsg = currentAppLang === "en" ? `Save failed: ${err}` : `保存失败: ${err}`;
    showToast(failMsg, "warning", 2000);
  }
}

// 触发保存译图到本地文件流程 (点击保存后遮罩立即退出让出桌面，若取消则自动恢复现场)
async function triggerTransSave() {
  if (currentRect.w < 10 || currentRect.h < 10) return;

  try {
    let savedPath: string | null = null;
    if (isShowingTranslation && currentTranslationResult) {
      const dataUrl = await generateTranslatedImageDataUrl(currentTranslationResult);
      savedPath = await invoke<string | null>("save_data_url_to_file", { dataUrl });
    } else {
      const rect = {
        x: Math.round(currentRect.x * scaleFactor),
        y: Math.round(currentRect.y * scaleFactor),
        width: Math.round(currentRect.w * scaleFactor),
        height: Math.round(currentRect.h * scaleFactor),
      };
      savedPath = await invoke<string | null>("save_selection_to_file", { rect });
    }

    if (savedPath) {
      // 成功保存：由于遮罩已在后端平滑隐藏，前端直接重置选区状态完成本次会话
      resetSelection();
    }
  } catch (err: any) {
    console.error("保存图片失败:", err);
    const failMsg = currentAppLang === "en" ? `Save failed: ${err}` : `保存失败: ${err}`;
    showToast(failMsg, "warning", 2000);
  }
}

let actionCopyShortcut = "Ctrl+C";
let actionOcrShortcut = "Ctrl+T";
let actionTranslateShortcut = "Ctrl+S";
let actionPinShortcut = "Ctrl+P";

function getTranslateBtnTitle(): string {
  if (currentAppLang === "en") return `Translate (${actionTranslateShortcut} / Enter)`;
  if (currentAppLang === "zh-TW") return `翻譯 (${actionTranslateShortcut} / 回車)`;
  return `翻译 (${actionTranslateShortcut} / 回车)`;
}

function matchesShortcut(e: KeyboardEvent, shortcutStr: string): boolean {
  if (!shortcutStr) return false;
  const parts = shortcutStr.split("+").map((p) => p.trim().toLowerCase());
  const hasCtrl = parts.includes("ctrl");
  const hasAlt = parts.includes("alt");
  const hasShift = parts.includes("shift");
  const hasSuper = parts.includes("super") || parts.includes("meta");

  if (hasCtrl !== (e.ctrlKey || (e.metaKey && !hasSuper))) return false;
  if (hasAlt !== e.altKey) return false;
  if (hasShift !== e.shiftKey) return false;
  if (hasSuper !== e.metaKey) return false;

  const mainPart = parts.find((p) => !["ctrl", "alt", "shift", "super", "meta"].includes(p));
  if (!mainPart) return false;

  if (mainPart.startsWith("f") && !isNaN(Number(mainPart.slice(1)))) {
    return e.key.toLowerCase() === mainPart;
  }
  if (mainPart === "enter" || mainPart === "return") {
    return e.key === "Enter";
  }
  if (mainPart === "space") {
    return e.key === " ";
  }
  return e.key.toLowerCase() === mainPart;
}

async function loadActionShortcuts() {
  try {
    const config = await invoke<any>("get_config");
    if (config.action_copy_shortcut) actionCopyShortcut = config.action_copy_shortcut;
    if (config.action_ocr_shortcut) actionOcrShortcut = config.action_ocr_shortcut;
    if (config.action_translate_shortcut) actionTranslateShortcut = config.action_translate_shortcut;
    if (config.action_pin_shortcut) actionPinShortcut = config.action_pin_shortcut;

    const lang = config.app_language || "zh-CN";
    currentAppLang = lang;

    if (lang === "en") {
      btnCopy.title = `Copy (${actionCopyShortcut})`;
      if (btnOcr) {
        btnOcr.title = `Extract Text (${actionOcrShortcut})`;
      }
      btnPin.title = `Pin (${actionPinShortcut})`;
      btnTranslate.title = `Translate (${actionTranslateShortcut} / Enter)`;
      if (btnSave) btnSave.title = "Save Screenshot";
      btnCancel.title = "Cancel (ESC)";

      btnToggleView.title = isShowingTranslation ? "Show Original" : "Show Translation";
      btnCopyTransImg.title = `Copy Translated Image (${actionCopyShortcut})`;
      btnCopyOrigImg.title = `Copy Original (Ctrl+Shift+C / ${actionCopyShortcut} in original view)`;
      btnCopyTransText.title = "Copy Translated Text";
      btnTransPin.title = "Pin to Desktop";
      if (btnTransSave) btnTransSave.title = "Save Current Image";
      btnTransCancel.title = "Exit (ESC)";
      updateBtnTag(btnCopyTransImg, "T");
      updateBtnTag(btnCopyOrigImg, "O");
    } else if (lang === "zh-TW") {
      btnCopy.title = `複製 (${actionCopyShortcut})`;
      if (btnOcr) {
        btnOcr.title = `獲取文字 (${actionOcrShortcut})`;
      }
      btnPin.title = `釘住 (${actionPinShortcut})`;
      btnTranslate.title = `翻譯 (${actionTranslateShortcut} / 回車)`;
      if (btnSave) btnSave.title = "儲存截圖";
      btnCancel.title = "取消 (ESC)";

      btnToggleView.title = isShowingTranslation ? "顯示原圖" : "顯示譯圖";
      btnCopyTransImg.title = `複製覆蓋譯文後的圖片 (${actionCopyShortcut})`;
      btnCopyOrigImg.title = `複製未翻譯的原圖選區 (Ctrl+Shift+C / 切換原圖後 ${actionCopyShortcut})`;
      btnCopyTransText.title = "複製翻譯出的文字內容";
      btnTransPin.title = "將當前選區釘在桌面";
      if (btnTransSave) btnTransSave.title = "儲存當前圖片";
      btnTransCancel.title = "完成並退出 (ESC)";
      updateBtnTag(btnCopyTransImg, "譯");
      updateBtnTag(btnCopyOrigImg, "原");
    } else {
      btnCopy.title = `复制 (${actionCopyShortcut})`;
      if (btnOcr) {
        btnOcr.title = `获取文本 (${actionOcrShortcut})`;
      }
      btnPin.title = `钉住 (${actionPinShortcut})`;
      btnTranslate.title = `翻译 (${actionTranslateShortcut} / 回车)`;
      if (btnSave) btnSave.title = "保存截图";
      btnCancel.title = "取消 (ESC)";

      btnToggleView.title = isShowingTranslation ? "显示原图" : "显示译图";
      btnCopyTransImg.title = `复制覆盖译文后的图片 (${actionCopyShortcut})`;
      btnCopyOrigImg.title = `复制未翻译的原图选区 (Ctrl+Shift+C / 切换原图后 ${actionCopyShortcut})`;
      btnCopyTransText.title = "复制翻译出的文本内容";
      btnTransPin.title = "将当前选区钉在桌面";
      if (btnTransSave) btnTransSave.title = "保存当前图片";
      btnTransCancel.title = "完成并退出 (ESC)";
      updateBtnTag(btnCopyTransImg, "译");
      updateBtnTag(btnCopyOrigImg, "原");
    }
    updateToggleViewBtnText();
  } catch (err) {
    console.error("加载快捷键配置失败:", err);
  }
}
loadActionShortcuts();

// 键盘快捷键监听：
// ESC 取消 | 自定义复制截图 | 自定义获取文本 (OCR) | 自定义钉在桌面 | 自定义翻译 (或 Enter)
window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape") {
    await cancelOverlay();
    return;
  }

  // 快捷键 Ctrl+Shift+C: 无论在何种视图下，直接复制未翻译的原图选区
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "c" || e.key === "C")) {
    e.preventDefault();
    dismissTextSelection();
    btnCopyOrigImg.click();
    return;
  }

  // 复制快捷键：若用户鼠标高亮选中了具体译文字符，优先复制选中的文字
  if (matchesShortcut(e, actionCopyShortcut) || ((e.ctrlKey || e.metaKey) && (e.key === "c" || e.key === "C"))) {
    const selectedText = window.getSelection()?.toString();
    if (selectedText && selectedText.trim().length > 0) {
      e.preventDefault();
      try {
        await navigator.clipboard.writeText(selectedText);
        const preview = selectedText.trim().length > 20
          ? selectedText.trim().slice(0, 20) + "..."
          : selectedText.trim();
        showToast(
          currentAppLang === "en" ? `Text copied: "${preview}"` : `已复制选中文本: "${preview}"`,
          "success",
          1500
        );
      } catch (err) {
        console.error("复制选中文本失败:", err);
      }
      dismissTextSelection();
      return;
    }

    if (matchesShortcut(e, actionCopyShortcut)) {
      e.preventDefault();
      dismissTextSelection();
      if (currentTranslationResult) {
        if (isShowingTranslation) {
          btnCopyTransImg.click();
        } else {
          btnCopyOrigImg.click();
        }
      } else {
        await triggerCopy();
      }
      return;
    }
  }

  // 获取文本 (OCR 提取并复制)
  if (matchesShortcut(e, actionOcrShortcut)) {
    e.preventDefault();
    dismissTextSelection();
    if (currentTranslationResult) {
      btnCopyTransText.click();
    } else {
      await triggerOcrText();
    }
    return;
  }

  // 钉在桌面
  if (matchesShortcut(e, actionPinShortcut)) {
    e.preventDefault();
    dismissTextSelection();
    await triggerPin();
    return;
  }

  // 翻译选区 (支持配置的快捷键或默认 Enter 回车)
  if (matchesShortcut(e, actionTranslateShortcut) || e.key === "Enter") {
    e.preventDefault();
    dismissTextSelection();
    if (!currentTranslationResult) {
      await triggerTranslate();
    }
    return;
  }

  // 保存当前截图 / 译图 (Ctrl+Shift+S)
  if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "s" || e.key === "S")) {
    e.preventDefault();
    dismissTextSelection();
    if (currentTranslationResult) {
      await triggerTransSave();
    } else {
      await triggerSave();
    }
    return;
  }
});

// 点击工具栏时，高优先级打断任何正在进行的选词低优先级状态
toolbar.addEventListener("mousedown", () => {
  dismissTextSelection();
});

btnCancel.addEventListener("click", async () => {
  dismissTextSelection();
  await cancelOverlay();
});

btnSave?.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerSave();
});

btnCopy.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerCopy();
});

btnOcr?.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerOcrText();
});

btnPin.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerPin();
});

btnTranslate.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerTranslate();
});

// 译文操作工具条事件监听
btnToggleView.addEventListener("click", () => {
  if (!currentTranslationResult) return;
  dismissTextSelection();
  isShowingTranslation = !isShowingTranslation;
  if (isShowingTranslation) {
    translationLayer.classList.remove("hidden");
  } else {
    translationLayer.classList.add("hidden");
  }
  updateToggleViewBtnText();
});

btnCopyTransImg.addEventListener("click", async () => {
  if (!currentTranslationResult) return;
  // 高优先级截图动作立即打断并清除选中文本状态
  dismissTextSelection();
  try {
    btnCopyTransImg.disabled = true;
    showToast(currentAppLang === "en" ? "Copying translated image..." : "正在合成译图并复制...", "info", 2000);
    const dataUrl = await generateTranslatedImageDataUrl(currentTranslationResult);
    await invoke("copy_translated_image_cmd", { dataUrl });
    showToast(currentAppLang === "en" ? "Translated image copied!" : "合成译图已复制到剪贴板", "success", 1200);
    setTimeout(async () => {
      await closeOverlay();
    }, 250);
  } catch (err) {
    console.error("复制译图失败:", err);
    showToast("复制译图失败: " + err, "warning", 2500);
  } finally {
    btnCopyTransImg.disabled = false;
  }
});

btnCopyOrigImg.addEventListener("click", async () => {
  dismissTextSelection();
  try {
    btnCopyOrigImg.disabled = true;
    if (currentTranslationResult?.image_data) {
      showToast(currentAppLang === "en" ? "Copying original image..." : "正在复制原图...", "info", 1500);
      await invoke("copy_translated_image_cmd", { dataUrl: currentTranslationResult.image_data });
    } else {
      const rect = {
        x: Math.round(currentRect.x * scaleFactor),
        y: Math.round(currentRect.y * scaleFactor),
        width: Math.round(currentRect.w * scaleFactor),
        height: Math.round(currentRect.h * scaleFactor),
      };
      await invoke("copy_selection_to_clipboard", { rect });
    }
    showToast(currentAppLang === "en" ? "Original image copied!" : "原图已复制到剪贴板", "success", 1200);
    setTimeout(async () => {
      await closeOverlay();
    }, 250);
  } catch (err) {
    console.error("复制原图失败:", err);
    showToast("复制原图失败: " + err, "warning", 2500);
  } finally {
    btnCopyOrigImg.disabled = false;
  }
});

btnCopyTransText.addEventListener("click", async () => {
  if (!currentTranslationResult || currentTranslationResult.blocks.length === 0) return;
  dismissTextSelection();

  const isTrans = isShowingTranslation;
  const fullText = currentTranslationResult.blocks
    .map((b) => (isTrans ? b.translated_text : b.original_text)?.trim())
    .filter((t) => t && t.length > 0)
    .join("\n\n");

  try {
    await navigator.clipboard.writeText(fullText);
    const successMsg = currentAppLang === "en"
      ? (isTrans ? "Translated text copied to clipboard" : "Original text copied to clipboard")
      : currentAppLang === "zh-TW"
      ? (isTrans ? "譯文已複製到剪貼簿" : "原文已複製到剪貼簿")
      : (isTrans ? "译文已复制到剪贴板" : "原文已复制到剪贴板");
    showToast(successMsg, "success", 1500);
  } catch (err) {
    console.error("复制文本失败:", err);
    showToast("复制失败", "warning");
  }
});

btnTransPin.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerPin();
});

btnTransSave?.addEventListener("click", async () => {
  dismissTextSelection();
  await triggerTransSave();
});

btnTransCancel.addEventListener("click", async () => {
  dismissTextSelection();
  await cancelOverlay();
});

// 鼠标选区变动监听：当在译文层高亮选中文字时，在光标上方展示快捷复制气泡
document.addEventListener("selectionchange", () => {
  if (!currentTranslationResult || !isShowingTranslation) {
    hideSelectionBubble();
    return;
  }
  const selection = window.getSelection();
  const selectedText = selection?.toString().trim();
  if (selectedText && selectedText.length > 0 && selection && selection.rangeCount > 0) {
    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      showSelectionBubble(rect.left + rect.width / 2, Math.max(10, rect.top - 6));
      return;
    }
  }
  hideSelectionBubble();
});

btnBubbleCopy?.addEventListener("click", async (e) => {
  e.stopPropagation();
  e.preventDefault();
  const selectedText = window.getSelection()?.toString();
  if (selectedText && selectedText.trim().length > 0) {
    try {
      await navigator.clipboard.writeText(selectedText);
      const preview = selectedText.trim().length > 20
        ? selectedText.trim().slice(0, 20) + "..."
        : selectedText.trim();
      showToast(
        currentAppLang === "en" ? `Text copied: "${preview}"` : `已复制选中文本: "${preview}"`,
        "success",
        1500
      );
    } catch (err) {
      console.error("复制选中文本失败:", err);
    }
  }
  dismissTextSelection();
});
