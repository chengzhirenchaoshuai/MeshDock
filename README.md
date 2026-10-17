# MeshDock

给自建 Headscale 组网用的中文图形客户端：一眼看懂每台设备连上了没有、走的是哪条路、快不快，常用操作做成按钮。

本工具**不修改、不替换官方客户端**，只调用官方命令行 `tailscale.exe`，读取结果后用中文展示。

## 安装

两种版本任选其一：

- **安装版** `MeshDock_<版本>_x64-setup.exe`：装到当前用户，不需要管理员权限。电脑上没有 WebView2 时，安装程序会自动下载安装。卸载在“设置 → 应用”里找 MeshDock。
- **免安装版** `MeshDock_<版本>_x64-portable.zip`：解压后运行 `MeshDock.exe`。程序旁有 `data` 文件夹时为便携模式，设置和界面缓存都存在里面，删掉整个文件夹即可清除。需要电脑上已有 WebView2（Windows 10 一般自带）。

前提：已安装 Tailscale 官方客户端（默认位置 `C:\Program Files\Tailscale\tailscale.exe`；装在别处时可用环境变量 `MESHDOCK_TAILSCALE` 指定）。

## 功能

- **设备列表**：每台设备的在线状态、连接方式（局域网直连 / IPv6 直连 / 公网直连 / 中继 / 正在打洞 / 在线·空闲 / 离线）、收发流量、最后在线时间；点击 IP 复制。每 10 秒自动刷新。
- **测速**：单台“测试”或“全部测试”，显示实测路径和延迟。
- **网络体检**：检查 UDP、IPv6、NAT 类型、到广州中继的延迟，翻译成人话。
- **在家 / 出门模式**：显示并切换 accept-routes；标记“我现在在家”后，模式开错会醒目提示并可一键改回。
- **托盘**：圆点颜色表示整体状态（绿=已连接、灰=未连接、红=控制端连不上），右键中文菜单。

## 使用提示

- Windows 10 会把新出现的托盘图标收进任务栏右侧的“^”里。把 MeshDock 图标拖到任务栏上，就能一直看到状态圆点。
- 关闭主窗口只是隐藏到托盘，要退出请在托盘图标上右键选“退出”。
- 本机设置保存在 `%APPDATA%\com.meshdock.app\settings.json`（免安装版在 `data\settings.json`），只记录“是否在家”，不保存任何密钥。

## 文档

- [需求说明](docs/需求说明.md)
- [实施计划](docs/实施计划.md)（含“待实测清单”）
- [验收记录](docs/验收记录.md)

## 开发

环境：Rust 1.98+、Node 24+、pnpm 12+，Windows 10 22H2 及以上。

```powershell
pnpm install          # 安装前端依赖
pnpm tauri dev        # 开发模式运行
pnpm package          # 打包：安装版和免安装版都输出到项目根目录 release\
```

技术栈：Tauri 2 + Vue 3 + TypeScript + Tailwind CSS 4。
