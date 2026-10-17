# MeshDock

给自建 Headscale 组网用的中文图形客户端：一眼看懂每台设备连上了没有、走的是哪条路、快不快，常用操作做成按钮。

本工具**不修改、不替换官方客户端**，只调用官方命令行 `tailscale.exe`，读取结果后用中文展示。

## 使用提示

- Windows 10 会把新出现的托盘图标收进任务栏右侧的“^”里。把 MeshDock 图标拖到任务栏上，就能一直看到状态圆点。
- 关闭主窗口只是隐藏到托盘，要退出请在托盘图标上右键选“退出”。

## 文档

- [需求说明](docs/需求说明.md)
- [实施计划](docs/实施计划.md)

## 开发

环境：Rust 1.98+、Node 24+、pnpm 12+，Windows 10 22H2 及以上。

```powershell
pnpm install          # 安装前端依赖
pnpm tauri dev        # 开发模式运行
pnpm tauri build      # 打包 NSIS 安装包
```

技术栈：Tauri 2 + Vue 3 + TypeScript + Tailwind CSS 4。
