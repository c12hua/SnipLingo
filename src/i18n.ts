export type Language = "zh-CN" | "en" | "zh-TW";

export const translations: Record<Language, Record<string, string>> = {
  "zh-CN": {
    "nav.general": "常规",
    "nav.capture": "截屏",
    "nav.pin": "贴图",
    "nav.hotkeys": "快捷键",
    "nav.translation": "翻译",
    "nav.ocr": "OCR设置",
    "status.running": "后台常驻中",

    "header.general.title": "常规设置",
    "label.language": "显示语言",
    "label.autostart": "开机启动",
    "label.silent_start": "静默启动",
    "desc.silent_start": "启动后直接最小化至系统托盘，不弹出主窗口",

    "header.capture.title": "截屏设置",
    "section.actions": "选区动作快捷键",
    "label.action_copy": "复制",
    "label.action_ocr": "获取文本",
    "label.action_qrcode": "识别二维码",
    "label.action_translate": "翻译",
    "label.action_pin": "钉住选区",

    "header.pin.title": "桌面贴图设置",
    "label.pin_shadow": "贴图边框",
    "label.pin_opacity": "贴图不透明度",

    "header.hotkeys.title": "快捷键管理",
    "section.global_hotkeys": "全局快捷键",
    "label.hotkey": "截图唤起快捷键",
    
    "header.translation.title": "翻译设置",
    "section.trans_basic": "基础设置",
    "label.source_lang": "原文语言",
    "label.target_lang": "译文语言",
    "lang.auto": "自动检测",
    "lang.zh_cn": "简体中文 (Simplified Chinese)",
    "lang.zh_tw": "繁体中文 (Traditional Chinese)",
    "lang.en": "英语 (English)",
    "lang.ja": "日语 (Japanese)",
    "lang.ko": "韩语 (Korean)",
    "lang.fr": "法语 (French)",
    "lang.de": "德语 (German)",
    "lang.es": "西班牙语 (Spanish)",
    "lang.ru": "俄语 (Russian)",
    "lang.it": "意大利语 (Italian)",
    "label.in_place_translate": "截图直接显示译文",
    "section.trans_engine": "翻译引擎配置",
    "label.provider": "服务提供商",
    "label.baidu_appid": "百度 App ID",
    "desc.baidu_appid": "登录百度翻译开放平台开发者页获取",
    "label.baidu_secret": "密钥 (Secret Key)",
    "desc.baidu_secret": "百度翻译开放平台的开发者密钥",
    "label.base_url": "API 接口地址 (Base URL)",
    "label.model": "模型名称 (Model)",
    "btn.test_api": "一键测试连接",

    "header.ocr.title": "OCR 文字识别设置",
    "section.ocr_format": "文字识别与格式优化",
    "label.ocr_engine": "OCR 识别引擎",
    "ocr.engine_windows": "Windows Media OCR",
    "ocr.engine_paddle": "PaddleOCR",
    "label.ocr_lang": "优先识别语言",
    "label.preserve_breaks": "保持代码与排版换行",
    "label.enhance_contrast": "暗色主题与低对比度增强",
    "section.qrcode": "二维码识别",
    "label.qrcode_mode": "识别结果处理",
    "qrcode.mode_copy": "复制内容到剪贴板",
    "qrcode.mode_open": "网址用默认浏览器打开",

    "btn.reset_default": "恢复默认设置",
    "status.saved": "已保存设置",
    "status.restored": "已恢复默认设置"
  },

  "zh-TW": {
    "nav.general": "常規",
    "nav.capture": "截圖",
    "nav.pin": "貼圖",
    "nav.hotkeys": "快捷鍵",
    "nav.translation": "翻譯",
    "nav.ocr": "OCR設定",
    "status.running": "後台常駐中",

    "header.general.title": "常規設定",
    "label.language": "顯示語言",
    "label.autostart": "開機啟動",
    "label.silent_start": "靜默啟動",
    "desc.silent_start": "啟動後直接最小化至系統托盤，不彈出主視窗",

    "header.capture.title": "截圖設定",
    "section.actions": "選區動作快捷鍵",
    "label.action_copy": "複製",
    "label.action_ocr": "獲取文字",
    "label.action_qrcode": "識別二維碼",
    "label.action_translate": "翻譯",
    "label.action_pin": "釘住選區",

    "header.pin.title": "桌面貼圖設定",
    "label.pin_shadow": "貼圖邊框",
    "label.pin_opacity": "貼圖不透明度",

    "header.hotkeys.title": "快捷鍵管理",
    "section.global_hotkeys": "全域快捷鍵",
    "label.hotkey": "截圖喚起快捷鍵",

    "header.translation.title": "翻譯設定",
    "section.trans_basic": "基礎設定",
    "label.source_lang": "原文語言",
    "label.target_lang": "譯文語言",
    "lang.auto": "自動檢測",
    "lang.zh_cn": "簡體中文 (Simplified Chinese)",
    "lang.zh_tw": "繁體中文 (Traditional Chinese)",
    "lang.en": "英語 (English)",
    "lang.ja": "日語 (Japanese)",
    "lang.ko": "韓語 (Korean)",
    "lang.fr": "法語 (French)",
    "lang.de": "德語 (German)",
    "lang.es": "西班牙語 (Spanish)",
    "lang.ru": "俄語 (Russian)",
    "lang.it": "義大利語 (Italian)",
    "label.in_place_translate": "截圖直接顯示譯文",
    "section.trans_engine": "翻譯引擎配置",
    "label.provider": "服務提供商",
    "label.baidu_appid": "百度 App ID",
    "desc.baidu_appid": "登入百度翻譯開放平台開發者頁獲取",
    "label.baidu_secret": "金鑰 (Secret Key)",
    "desc.baidu_secret": "百度翻譯開放平台的開發者金鑰",
    "label.base_url": "API 介面位址 (Base URL)",
    "label.model": "模型名稱 (Model)",
    "btn.test_api": "一鍵測試連線",

    "header.ocr.title": "OCR 文字辨識設定",
    "section.ocr_format": "文字辨識與格式最佳化",
    "label.ocr_engine": "OCR 辨識引擎",
    "ocr.engine_windows": "Windows Media OCR",
    "ocr.engine_paddle": "PaddleOCR",
    "label.ocr_lang": "優先辨識語言",
    "label.preserve_breaks": "保持代碼與排版換行",
    "label.enhance_contrast": "暗色主題與低對比度增強",
    "section.qrcode": "二維碼識別",
    "label.qrcode_mode": "識別結果處理",
    "qrcode.mode_copy": "複製內容到剪貼簿",
    "qrcode.mode_open": "網址用預設瀏覽器開啟",

    "btn.reset_default": "恢復預設設定",
    "status.saved": "已保存設定",
    "status.restored": "已恢復預設設定"
  },

  "en": {
    "nav.general": "General",
    "nav.capture": "Capture",
    "nav.pin": "Pin",
    "nav.hotkeys": "Hotkeys",
    "nav.translation": "Translation",
    "nav.ocr": "OCR",
    "status.running": "Running",

    "header.general.title": "General Settings",
    "label.language": "Display Language",
    "label.autostart": "Launch on Startup",
    "label.silent_start": "Silent Startup",
    "desc.silent_start": "Start minimized to system tray without opening the main window",

    "header.capture.title": "Capture Settings",
    "section.actions": "Action Shortcuts",
    "label.action_copy": "Copy",
    "label.action_ocr": "Extract Text",
    "label.action_qrcode": "Recognize QR Code",
    "label.action_translate": "Translate",
    "label.action_pin": "Pin Selection",

    "header.pin.title": "Desktop Pin Settings",
    "label.pin_shadow": "Pin Border",
    "label.pin_opacity": "Pin Opacity",

    "header.hotkeys.title": "Hotkey Management",
    "section.global_hotkeys": "Global Hotkeys",
    "label.hotkey": "Capture Hotkey",

    "header.translation.title": "Translation Settings",
    "section.trans_basic": "Basic Settings",
    "label.source_lang": "Source Language",
    "label.target_lang": "Target Language",
    "lang.auto": "Auto Detect",
    "lang.zh_cn": "Simplified Chinese",
    "lang.zh_tw": "Traditional Chinese",
    "lang.en": "English",
    "lang.ja": "Japanese",
    "lang.ko": "Korean",
    "lang.fr": "French",
    "lang.de": "German",
    "lang.es": "Spanish",
    "lang.ru": "Russian",
    "lang.it": "Italian",
    "label.in_place_translate": "In-Place Screenshot Translation",
    "section.trans_engine": "Engine Configuration",
    "label.provider": "Service Provider",
    "label.baidu_appid": "Baidu App ID",
    "desc.baidu_appid": "Get from Baidu Translate Developer Console",
    "label.baidu_secret": "Secret Key",
    "desc.baidu_secret": "Developer secret key for Baidu Translate",
    "label.base_url": "API Base URL",
    "label.model": "Model Name",
    "btn.test_api": "Test Connection",

    "header.ocr.title": "OCR Settings",
    "section.ocr_format": "Recognition & Formatting",
    "label.ocr_engine": "OCR Engine",
    "ocr.engine_windows": "Windows Media OCR",
    "ocr.engine_paddle": "PaddleOCR",
    "label.ocr_lang": "Primary Language",
    "label.preserve_breaks": "Preserve Line Breaks",
    "label.enhance_contrast": "Dark Theme & Low Contrast Boost",
    "section.qrcode": "QR Code Recognition",
    "label.qrcode_mode": "Result Action",
    "qrcode.mode_copy": "Copy content to clipboard",
    "qrcode.mode_open": "Open URLs in default browser",

    "btn.reset_default": "Reset Defaults",
    "status.saved": "Settings Saved",
    "status.restored": "Settings Restored"
  }
};

export function applyLanguage(lang: Language) {
  const dict = translations[lang] || translations["zh-CN"];
  const elements = document.querySelectorAll<HTMLElement>("[data-i18n]");

  elements.forEach((el) => {
    const key = el.dataset.i18n;
    if (key && dict[key]) {
      el.textContent = dict[key];
    }
  });

  // 更新全局 html lang 属性
  document.documentElement.lang = lang;
}
