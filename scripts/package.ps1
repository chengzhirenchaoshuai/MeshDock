# 收集打包产物：由 pnpm package 在 tauri build 之后调用
# 产物统一放到项目根目录 release/ 下：安装版 exe + 免安装版单文件 exe，旧版本保留
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$version = (Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$exe = "$root\src-tauri\target\release\meshdock.exe"
$setup = "$root\src-tauri\target\release\bundle\nsis\MeshDock_${version}_x64-setup.exe"
foreach ($f in $exe, $setup) {
  if (-not (Test-Path $f)) { throw "找不到 $f，请先运行 pnpm tauri build" }
}

$out = "$root\release"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $setup $out -Force

# 免安装版：单个 exe，前端已内置；数据默认放在 %APPDATA%\com.meshdock.app，
# 若在 exe 旁手动建 data 目录则改为便携模式（见 src-tauri/src/paths.rs）
$portable = "$out\MeshDock_${version}_x64-portable.exe"
Copy-Item $exe $portable -Force
"已生成："
"  $out\MeshDock_${version}_x64-setup.exe"
"  $portable"
