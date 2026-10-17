# 收集打包产物：由 pnpm package 在 tauri build 之后调用
# 产物统一放到项目根目录 release/ 下：安装版 exe + 免安装版 zip，旧版本保留
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

# 免安装版：data 目录存在即为便携模式
$stage = "$out\MeshDock"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\data" | Out-Null
Copy-Item $exe "$stage\MeshDock.exe"
# 放一个说明文件，避免压缩时空目录被丢弃
Set-Content "$stage\data\说明.txt" -Encoding utf8 -Value @"
此文件夹存在时，MeshDock 以便携模式运行：设置和界面缓存都保存在这里。
删除此文件夹则改用 %APPDATA%\com.meshdock.app。

免安装版需要电脑上已有 WebView2（Windows 10 一般自带）；没有时请改用安装版。
"@
$zip = "$out\MeshDock_${version}_x64-portable.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
Remove-Item -Recurse -Force $stage
"已生成："
"  $out\MeshDock_${version}_x64-setup.exe"
"  $zip"
