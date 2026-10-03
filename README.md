<div align="center">

<img src="docs/images/logo_final_preview.png" width="140" alt="SnipLingo Logo" />

# SnipLingo

**适用于 Windows 的截图、OCR、翻译与桌面贴图工具**

[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-blue.svg?style=flat-square)](https://www.microsoft.com/windows)
[![Tauri](https://img.shields.io/badge/Tauri-v2-orange.svg?style=flat-square&logo=tauri)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-1.77%2B-red.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.x-blue.svg?style=flat-square&logo=typescript)](https://www.typescriptlang.org/)
[![License](https://img.shields.io/badge/license-MIT-green.svg?style=flat-square)](LICENSE)

[**简体中文**](./README.md) | [**English**](./README_EN.md)

</div>

---

## 简介

SnipLingo 是一款 Windows 桌面工具，可以截取屏幕区域、识别其中的文字并进行翻译。翻译结果既可以显示在独立窗口中，也可以覆盖在截图原文的位置，方便阅读网页、文档和其他屏幕内容。

识别完成的截图还可以固定为桌面贴图。项目提供 Windows 原生 OCR 和 PaddleOCR 两种识别方式，并可连接 Google、百度、DeepL 或兼容 OpenAI 格式的翻译接口。

---

## 功能

### 1. 屏幕截图
- 默认按 `F4` 打开截图选区。
- 支持多显示器，并处理不同分辨率和 DPI 缩放下的坐标。

### 2. 文字识别
- 可选择 Windows Media OCR（系统提供的离线识别）或 PaddleOCR。
- 提供对比度增强和文本清理，帮助处理暗色背景、段落换行等常见情况。

### 3. 翻译
- 可将译文覆盖在截图文字位置，并在原图与译图之间切换。
- 内置 Google 翻译，也支持百度翻译、DeepL 和 OpenAI 兼容接口，包括 DeepSeek、OpenRouter、Kimi、Ollama 等服务。
- 各服务商的密钥、地址和模型设置分别保存，切换服务商时不会覆盖其他配置。
- 翻译结果会保存在内存缓存中，重复查询可直接返回缓存内容。

### 4. 桌面贴图
- 将截图固定为桌面浮动窗口，窗口由应用预先创建以缩短显示等待。
- 支持缩放、透明度调节、复制、保存和重新翻译。

### 5. 图像保存
- 可通过系统文件对话框将截图或贴图保存为 PNG、JPEG 或 BMP。

### 6. 系统集成
- 支持开机启动并最小化到系统托盘。
- 可自定义全局截图快捷键，也可以恢复默认设置。
- 界面支持简体中文、繁体中文和英文。

---

## 目录结构

```text
SnipLingo/
├── docs/            # 项目文档与图片
├── src/             # TypeScript 前端界面
│   ├── overlay/     # 截图遮罩与选区工具栏
│   ├── pin/         # 桌面贴图
│   ├── result/      # 翻译结果窗口
│   └── main.ts      # 设置界面与配置管理
├── src-tauri/       # Rust / Tauri 后端
│   ├── src/commands # 前后端 IPC 命令
│   ├── src/core     # 截图、OCR、翻译与系统集成
│   ├── Cargo.toml   # Rust 依赖与构建配置
│   └── tauri.conf.json # 窗口和应用配置
├── package.json     # 前端依赖与脚本
└── vite.config.ts   # Vite 构建配置
```

---

## 技术栈

| 领域 | 技术 | 用途 |
| :--- | :--- | :--- |
| **应用框架** | **Tauri v2** | 使用 Windows WebView2 构建桌面应用。 |
| **前端** | **TypeScript + Vite** | 以原生 DOM 和 CSS 实现多页面界面。 |
| **后端** | **Rust 1.77+** | 处理截图、OCR、系统调用及应用逻辑。 |
| **屏幕捕获** | **xcap** | 获取显示器信息并捕获屏幕图像。 |
| **文字识别** | **Windows.Media.Ocr / PP-OCRv6** | 提供系统原生及 PaddleOCR 识别引擎。 |
| **网络请求** | **Reqwest + Tokio** | 发送翻译请求并复用 HTTP 连接。 |

---

## 开始使用

### 环境要求

- **操作系统**：Windows 10 / 11（64 位）
- **Node.js**：`>= 18.0.0`
- **Rust 工具链**：`>= 1.77.2`，推荐 `stable-x86_64-pc-windows-msvc`
- **C++ 构建工具**：Visual Studio 2022 Build Tools，并安装“使用 C++ 的桌面开发”工作负载
- **WebView2**：Windows 10 / 11 通常已包含运行时

### 获取代码并安装依赖

```powershell
git clone https://github.com/c12hua/SnipLingo.git
cd SnipLingo
npm install
```

### 开发运行

```powershell
npm run tauri dev
```

启动后按默认快捷键 `F4` 打开截图界面。

### 运行测试

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

### 编译打包

- **完整打包（同时生成安装包与便携包）**：
  ```powershell
  npm run build:all
  ```
- **仅生成安装包**：
  ```powershell
  npm run tauri build
  ```
- **仅生成便携包**：
  ```powershell
  npm run build:portable
  ```

构建产物位于：
- **安装包**：`src-tauri/target/release/bundle/nsis/SnipLingo_0.1.1_x64-setup.exe`
- **便携包**：`src-tauri/target/release/bundle/SnipLingo_0.1.1_portable.zip`（已包含完整离线模型，解压即用）

---

## 配置文件

用户配置存储在 `%APPDATA%\SnipLingo\config.json`。翻译服务商配置和应用偏好保存在本地；更新或重新安装时通常会保留这些设置。

---

## 参与贡献

欢迎提交问题反馈和 Pull Request。常用流程如下：

1. Fork 本仓库。
2. 创建分支：`git checkout -b feature/your-feature`。
3. 提交修改：`git commit -m 'feat: your commit message'`。
4. 运行测试：`cargo test --manifest-path src-tauri/Cargo.toml`。
5. 推送分支并创建 Pull Request。

---

## 许可证

本项目采用 [MIT](LICENSE) 许可证。
