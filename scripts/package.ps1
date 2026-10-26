# 打包：由 pnpm package 调用。先用更新签名私钥执行 tauri build，再把产物统一放到项目根目录 release/ 下：
# 安装版 exe、免安装版单文件 exe，以及自动更新用的 latest.json（旧版本保留）
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$version = (Get-Content "$root\src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
$repo = "chengzhirenchaoshuai/MeshDock"

# 更新签名私钥：默认在 %USERPROFILE%\.tauri\meshdock.key（不进仓库），也可事先设好环境变量
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  $key = Join-Path $env:USERPROFILE ".tauri\meshdock.key"
  if (-not (Test-Path $key)) { throw "找不到更新签名私钥 $key，见 README 中的「发布新版本」" }
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content $key -Raw
}
if ($null -eq $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "" }

Push-Location $root
try {
  pnpm tauri build
  if ($LASTEXITCODE -ne 0) { throw "tauri build 失败" }
} finally {
  Pop-Location
}

$exe = "$root\src-tauri\target\release\meshdock.exe"
$setupName = "MeshDock_${version}_x64-setup.exe"
$setup = "$root\src-tauri\target\release\bundle\nsis\$setupName"
foreach ($f in $exe, $setup, "$setup.sig") {
  if (-not (Test-Path $f)) { throw "找不到 $f" }
}

$out = "$root\release"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $setup $out -Force

# 免安装版：单个 exe，前端已内置；数据默认放在 %APPDATA%\com.meshdock.app，
# 若在 exe 旁手动建 data 目录则改为便携模式（见 src-tauri/src/paths.rs）
$portable = "$out\MeshDock_${version}_x64-portable.exe"
Copy-Item $exe $portable -Force

# 更新内容：取 CHANGELOG 中本版本那一节（到下一个“## [”为止）
$changelog = Get-Content "$root\CHANGELOG.md" -Raw -Encoding UTF8
$m = [regex]::Match($changelog, "(?ms)^## \[$([regex]::Escape($version))\][^\n]*\n(.*?)(?=^## \[|\z)")
$notes = if ($m.Success) { $m.Groups[1].Value.Trim() } else { "" }

# latest.json：随发布一起上传，程序读取 releases/latest/download/latest.json 判断有没有新版本
$latest = [ordered]@{
  version   = $version
  notes     = $notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = (Get-Content "$setup.sig" -Raw).Trim()
      url       = "https://github.com/$repo/releases/download/v$version/$setupName"
    }
  }
}
$latestDir = "$out\$version"
New-Item -ItemType Directory -Force $latestDir | Out-Null
$json = $latest | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText("$latestDir\latest.json", $json, (New-Object System.Text.UTF8Encoding $false))

"已生成："
"  $out\$setupName"
"  $portable"
"  $latestDir\latest.json（发布时一起上传）"
