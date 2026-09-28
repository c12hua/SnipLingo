import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface PinData {
  label: string;
  image_data: string;
  width: number;
  height: number;
  has_shadow: boolean;
  opacity?: number;
}

const appWindow = getCurrentWebviewWindow();
const pinWrapper = document.getElementById("pin-wrapper") as HTMLDivElement;
const pinImage = document.getElementById("pin-image") as HTMLImageElement;
const toastEl = document.getElementById("toast") as HTMLDivElement;

let toastTimer: number | undefined;
function showToast(msg: string) {
  if (toastTimer) clearTimeout(toastTimer);
  toastEl.textContent = msg;
  toastEl.classList.remove("hidden");
  toastTimer = window.setTimeout(() => toastEl.classList.add("hidden"), 1600);
}

// 1. 初始化与贴图数据应用
function applyPinData(data: PinData) {
  if (data && data.image_data) {
    pinImage.src = data.image_data;
    if (data.has_shadow) {
      pinWrapper.classList.add("has-shadow");
    } else {
      pinWrapper.classList.remove("has-shadow");
    }
    const op = typeof data.opacity === "number" ? data.opacity / 100 : 1;
    pinWrapper.style.opacity = op.toString();
  }
}

async function initPin() {
  try {
    const data = await invoke<PinData>("get_pin_data", { label: appWindow.label });
    applyPinData(data);
  } catch (err) {
    console.error("加载贴图数据失败:", err);
  }
}

// 监听复用贴图窗口时的刷新事件：严格校验目标 label 属于当前窗口，绝不覆盖其他贴图
listen<PinData>("load-pin", (event) => {
  if (event.payload && event.payload.label === appWindow.label) {
    applyPinData(event.payload);
  }
});

// 监听复制与提示事件
listen<string>("pin-copied", (event) => {
  if (event.payload === appWindow.label) {
    showToast("图片已复制到剪贴板");
  }
});

listen<string>("pin-toast", (event) => {
  showToast(event.payload);
});

// 2. 鼠标左键按下时启动窗口平滑拖拽移动（与 HTML data-tauri-drag-region 双重保障）
window.addEventListener("mousedown", async (e) => {
  if (e.button === 0) { // 左键
    try {
      await appWindow.startDragging();
    } catch {
      // 忽略由原生 drag-region 处理引起的并发调用提示
    }
  }
});

// 3. 鼠标右键唤出原生操作系统级菜单（彻底解决小尺寸截图被容器边界裁剪截断的问题）
window.addEventListener("contextmenu", async (e) => {
  e.preventDefault();
  try {
    await invoke("show_pin_context_menu", { label: appWindow.label });
  } catch (err) {
    console.error("唤出原生右键菜单失败:", err);
  }
});

// 4. 快捷键支持：ESC / Delete 销毁，Ctrl+C 复制，Ctrl+S / T 翻译
window.addEventListener("keydown", async (e) => {
  if (e.key === "Escape" || e.key === "Delete") {
    e.preventDefault();
    await invoke("destroy_pin_window", { label: appWindow.label });
  } else if ((e.ctrlKey || e.metaKey) && (e.key === "c" || e.key === "C")) {
    e.preventDefault();
    try {
      await invoke("copy_pin_image_cmd", { label: appWindow.label });
    } catch (err) {
      showToast("复制失败: " + err);
    }
  } else if ((e.ctrlKey || e.metaKey) && (e.key === "s" || e.key === "S")) {
    e.preventDefault();
    try {
      await invoke("save_pin_image_cmd", { label: appWindow.label });
    } catch (err) {
      showToast("保存失败: " + err);
    }
  } else if (((e.ctrlKey || e.metaKey) && (e.key === "t" || e.key === "T")) || e.key === "t" || e.key === "T") {
    e.preventDefault();
    try {
      await invoke("translate_pin_cmd", { label: appWindow.label });
    } catch (err) {
      showToast("翻译失败: " + err);
    }
  }
});

initPin();
