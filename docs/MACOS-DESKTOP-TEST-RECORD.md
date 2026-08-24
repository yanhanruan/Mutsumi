# macOS 桌面真机测试记录

> 测试窗口：2026-08-23 至 2026-08-24
>
> 分支：`feat/macos-adaptation`
>
> 测试起点：`a0f9d00 docs(macOS): add cross-device development handoff`
>
> 证据范围：未签名开发产物；不代表可分发、已签名或已 notarize

本文只记录已经实际执行的测试。自动化通过不替代需要观察窗口、Dock、显示器、
权限提示或真实 Intel 硬件的人工验收。

## 1. 测试环境

| 项目 | 值 |
| --- | --- |
| 设备 | MacBook Air (`Mac17,3`) |
| 芯片 | Apple M5，10 核，16 GB 内存 |
| 系统 | macOS 26.6.2 (`25G83`) |
| 本机架构 | `arm64` |
| 显示器 | Built-in Retina Display，逻辑分辨率 `1470 x 956`，`2.0x` |
| 可见工作区 | 原点 `(0, 78)`，大小 `1470 x 845` |
| 当前网络 | Wi-Fi (`en0`)；测试时存在 VPN 虚拟接口，能力层归类为 `other` |
| 当前输出设备 | MacBook Air 内建扬声器，双声道，48 kHz |
| Rust | stable 1.98.0；安装 `aarch64-apple-darwin` 与 `x86_64-apple-darwin` |
| Node.js | 本机 26.7.0；远程 CI 仍以 Node.js 22 为准 |
| Rosetta | 首次 x86_64 smoke 前未安装；安装 Apple Rosetta 2 后复验通过 |

环境记录不包含序列号、IP 地址、MAC 地址、账号、Keychain 内容或开发者凭据。

## 2. 自动化基线

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| Vue/Vitest | 通过 | 25 个测试文件，387 项通过 |
| 前端生产构建 | 通过 | `vue-tsc -b` 与 Vite production build 完成 |
| Rust 测试 | 通过 | 342 项通过，36 项按设计 ignored |
| 发布、bundle、生命周期纯契约 | 通过 | 57/57；包含本轮新增请求 slice 执行预检 |
| universal app | 通过 | Mach-O 恰好包含 `arm64` 与 `x86_64` |
| 最低系统版本 | 通过 | `LSMinimumSystemVersion=13.0` |
| Dock 前台资格 | 通过 | `APPL`，非 `LSUIElement`，非 `LSBackgroundOnly` |
| unsigned DMG | 通过 | `mutsumi_1.5.3_universal.dmg` 生成，`hdiutil verify` 通过 |
| arm64 生命周期 | 通过 | 前台启动、第二实例复用/激活、标准 Quit、socket 清理 |
| x86_64 Rosetta 生命周期 | 通过 | 确认 `LSArchitecture=x86_64`，其余生命周期契约同样通过 |

DMG 在受控文件沙箱中首次运行 `bundle_dmg.sh` 失败；同一命令在正常交互式用户
会话中完成 Finder 布局、压缩和完整性校验。该失败属于测试执行环境限制，不是
bundle 内容失败。远程 `macos-latest` CI 仍需通过 Draft PR 独立验证。

## 3. macOS 原生探针

带 live/ignored 标记的探针均被逐项显式选择；空闲时间与默认网络探针属于普通
测试，并额外通过定向运行记录真机结果。没有运行需要 DashScope 密钥的 live suite：

| 探针 | 结果 | 说明 |
| --- | --- | --- |
| CoreAudio 默认输出活动属性 | 通过 | 公开 API 可读取当前默认输出设备状态 |
| GPU 与物理磁盘详情 | 通过 | `system_profiler` JSON 与 `diskutil` plist 均产生有效结果 |
| Keychain 后端往返 | 通过 | 唯一测试条目完成写入、读取、删除并由清理守卫兜底 |
| LaunchAgent 后端往返 | 通过 | 唯一测试配置完成启用、状态读取、禁用和清理 |
| 空闲时间公开 API | 通过 | CoreGraphics 与 IOKit 查询返回有效样本 |
| `PreventUserIdleDisplaySleep` | 通过 | 临时 `caffeinate -d` 断言被正确识别 |
| 默认网络类型 | 通过 | 活跃 VPN 为主路由时返回 `other`，没有伪报 Wi-Fi |

## 4. 测试中发现并处理的事项

### 4.1 x86_64 smoke 缺少 Rosetta 时启动了错误 slice

首次执行 x86_64 生命周期 smoke 时，系统尚未安装 Rosetta。macOS 的
`open --arch x86_64` 没有直接报错，而是启动了 universal app 的 arm64 slice；
现有运行时契约随后以 `LSArchitecture=arm64` 拒绝了结果，因此没有产生假阳性。

本轮增加目标 slice 执行预检：

- 启动应用前直接用 `arch -<requested> /usr/bin/true` 验证请求的 slice 能否执行，
  不再用 `uname -m` 猜测物理硬件；因此脚本本身运行在 Rosetta 下也不会误判。
- x86_64 不可执行时明确提示需要 Intel Mac 或 Rosetta 2；arm64 不可执行时明确
  提示需要 Apple Silicon，且两种情况都不会启动 Mutsumi。
- 对两个目标架构的可执行/不可执行组合增加纯契约测试。

安装 Rosetta 后，x86_64 生命周期 smoke 已重新执行并通过。

### 4.2 依赖审计提示

`npm ci` 在 2026-08-23 的 advisory 数据下报告 4 个 high 项，路径落在
Vite/PostCSS/nanoid/brace-expansion 的构建与测试依赖图。它们没有在本次 macOS
验证提交中自动升级，避免把依赖维护混入平台验收；后续应以独立安全维护提交更新
锁文件并同时跑 Windows/macOS 回归。没有执行 `npm audit fix`。

## 5. 尚未完成，不能标记通过

- Finder、真实 Dock 点击和菜单栏入口的逐项视觉/焦点观察。
- 主窗口隐藏后的 Dock 恢复，以及设置、关于、更新窗口的优先恢复行为。
- 单实例约 1 秒延迟聚焦期间由用户主动切走时是否抢焦点。
- 双屏、负坐标、不同缩放、主副屏切换和显示器热插拔；本机本轮只有单屏。
- 睡眠/唤醒、锁屏/解锁、网络物理断开/恢复。
- 真实登录启动；本轮只验证 LaunchAgent 后端往返。
- Keychain 首次允许、拒绝、锁定、撤销和重新授权的可见错误路径。
- 耳机、USB、HDMI、蓝牙、AirPlay 和暂时无默认输出设备的 CoreAudio 矩阵。
- 真实 Intel Mac；Rosetta 结果不能替代 Intel 硬件。
- 签名/notarized staging DMG 安装与真实跨版本自动更新。
- GitHub Draft PR 的 Windows regression 与 macOS universal 两个远程 job。
