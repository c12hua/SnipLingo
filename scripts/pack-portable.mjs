import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';

const rootDir = process.cwd();
const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf-8'));
const version = tauriConf.version || '0.1.0';
const productName = tauriConf.productName || 'SnipLingo';

const releaseDir = path.join(rootDir, 'src-tauri', 'target', 'release');
const exePath = path.join(releaseDir, 'sniplingo.exe');
const resourcesSrcDir = path.join(rootDir, 'src-tauri', 'resources');

if (!fs.existsSync(exePath)) {
  console.error(`\x1b[31m[Error] 编译产物不存在: ${exePath}\x1b[0m`);
  console.error('请先执行 npm run tauri build 或 npm run build:portable 生成二进制文件。');
  process.exit(1);
}

const bundleDir = path.join(releaseDir, 'bundle');
const stagingDir = path.join(bundleDir, 'portable_staging');
const zipPath = path.join(bundleDir, `${productName}_${version}_portable.zip`);

if (!fs.existsSync(bundleDir)) {
  fs.mkdirSync(bundleDir, { recursive: true });
}

// 准备临时打包目录
if (fs.existsSync(stagingDir)) {
  fs.rmSync(stagingDir, { recursive: true, force: true });
}
fs.mkdirSync(stagingDir, { recursive: true });

console.log(`\x1b[36m[便携版打包] 正在收集 ${productName} v${version} 运行时资源...\x1b[0m`);

// 1. 复制二进制执行程序
fs.copyFileSync(exePath, path.join(stagingDir, 'sniplingo.exe'));

// 2. 复制 resources (包含 onnxruntime.dll 及离线模型)
if (fs.existsSync(resourcesSrcDir)) {
  const stagingResourcesDir = path.join(stagingDir, 'resources');
  fs.cpSync(resourcesSrcDir, stagingResourcesDir, { recursive: true });
}

// 3. 压缩为 ZIP
console.log(`\x1b[36m[便携版打包] 正在压缩生成便携包: ${path.basename(zipPath)}...\x1b[0m`);
if (fs.existsSync(zipPath)) {
  fs.rmSync(zipPath, { force: true });
}

if (process.platform === 'win32') {
  // 使用 PowerShell 内置 Compress-Archive 打包
  execSync(
    `powershell -NoProfile -Command "Compress-Archive -Path '${stagingDir}\\*' -DestinationPath '${zipPath}' -Force"`,
    { stdio: 'inherit' }
  );
} else {
  execSync(`cd "${stagingDir}" && zip -r "${zipPath}" .`, { stdio: 'inherit' });
}

// 清理临时打包目录
fs.rmSync(stagingDir, { recursive: true, force: true });

const stats = fs.statSync(zipPath);
const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);
console.log(`\x1b[32m[便携版打包完成] 产物已生成:\x1b[0m`);
console.log(`  -> ${zipPath} (${sizeMB} MB)\n`);
