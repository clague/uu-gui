# uu-gui

中文 | [English](README.en.md)

第三方网易 UU（uu 加速器） PC 控制面板，通过**锐捷 Reyee 路由器**下发加速，Rust +
[iced](https://github.com/iced-rs/iced) 编写，`tiny-skia` CPU 渲染（无 GPU 驱动依赖）。

> ⚠️ **免责声明** — 本工具与网易 / 锐捷的**私有逆向协议**通信（`router.uu.163.com`
> 与路由器本地 `luci` 接口），使用了从 UU 官方 Android App 提取的请求头 / 加密
> 常量。仅供个人在自己的账号与自己的路由器上使用。使用它可能违反相应
> EULA；服务端接口变更可能随时使其失效。与网易 / 锐捷无任何关联。

## 功能

- **设备加速** — UU 账号短信验证码登录 → 列出路由器上全部设备 → 选择游戏 /
  大区 / 服务器 → 开始 / 停止加速 → 每设备"自动加速"开关。
- **路由器插件开关** — 本机登录锐捷路由器管理端（当前适配 X60 Pro 系列），
  切换其内置 UU 插件总开关。
- 无边框 M3 暗色窗口，保留原生缩放边框、DWM 阴影与 Win11 圆角；标题栏在应用内自绘。
- 无 GPU 要求：整个界面 CPU 渲染（tiny-skia）。

## 构建

```
rustup update stable
cargo build --release
target\release\uugui.exe
```

构建完全自包含：本仓库将项目依赖的 iced / winit 本地 fork 内置在
[`vendor/iced-src`](vendor/iced-src) 与 [`vendor/winit-src`](vendor/winit-src)
（每个目录里的 `PATCHES.md` 记录了修改内容与原因）。

## 隐私说明

配置保存在你自己机器上的 `~/.uu-gui/config.json` — 本仓库不会上传任何数据。
该文件存有路由器地址、管理密码（base64 编码，非加密）、UU 手机号、会话 token
与昵称。不要提交或分享该文件。

## 目录结构

```
src/            应用本体（Elm 风格 iced 程序）
vendor/iced-src iced 的本地 fork（性能 + 窗口补丁，见 PATCHES.md）
vendor/winit-src winit 的本地 fork（标题栏矩形修正，见 PATCHES.md）
assets/         Font Awesome (brands) 图标字体，CC BY 4.0
```
