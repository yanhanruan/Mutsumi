# Mutsumi macOS 适配计划

> 状态：实施中；已锁定 Dock 与透明窗口 API 边界
> 工作分支：`feat/macos-adaptation`
> 基线：`main` / `d3b52a9`
> 更新时间：2026-08-21

## 1. 背景

Mutsumi 当前是以 Windows 为唯一发布平台设计的 Tauri 2 桌面应用。Vue 前端和大部分 Rust 业务逻辑可以跨平台复用，但音频、媒体控制、光标、空闲检测、窗口原子操作、WebView 脚本回读、安装包与发布流程中存在 Windows 专属实现。

本次适配不以“能在 macOS 上编译”为终点，而是分阶段交付一个行为清晰、可以安装和升级、不会静默丢失核心交互的 macOS 版本。

## 2. 目标与非目标

### 2.1 目标

1. 在 Apple Silicon 和 Intel Mac 上构建、安装并启动 Mutsumi。
2. 保留桌宠核心体验：透明置顶、拖拽、点击穿透、托盘入口、动画和多窗口交互。
3. 保证聊天、长期记忆、天气、番茄钟、塔罗、设置和三语界面正常工作。
4. 对系统能力建立统一的 capability 模型；不支持的功能应明确降级，不允许静默 no-op。
5. 建立签名、notarization、DMG、自动更新和 CI 发布链。
6. 保持 Windows 现有功能和发布流程不回退。

### 2.2 本阶段非目标

1. 不发布 Mac App Store 版本；首版按官网/GitHub Releases 分发设计。
2. 不为了媒体控制采用未公开的 Apple 私有框架。
3. 不在首个 MVP 中强求 Windows SMTC 的完整跨应用媒体控制对等。
4. 不同时扩展 Linux 支持，但新增的平台抽象不得继续把非 Windows 平台统一当成一个模糊 fallback。

## 3. 待确认的产品基线

以下是建议值，review 后再锁定：

| 决策 | 建议 | 说明 |
| --- | --- | --- |
| 最低系统版本 | macOS 13 Ventura | 降低系统 API 分支和测试矩阵复杂度 |
| CPU 架构 | 首发同时支持 `arm64` 与 `x86_64` | 优先产出 universal 包；若第三方依赖阻塞，再拆成双包 |
| 分发方式 | 签名并 notarize 的 DMG | 暂不进入 Mac App Store |
| Dock 行为 | 常驻 Dock，同时保留菜单栏/托盘入口 | 需在真机确认隐藏、聚焦和设置窗口行为 |
| 媒体能力 | 公开 API 优先，首版允许显式降级 | 不采用私有 `MediaRemote` 作为生产依赖 |
| 权限策略 | 默认最少权限，按功能请求 | 权限被拒绝时仍可使用桌宠核心功能 |

## 4. 当前代码审计

| 区域 | 当前状态 | macOS 影响 | 优先级 |
| --- | --- | --- | --- |
| 应用入口 | `audio`、`media` 仅在 `cfg(windows)` 下声明，但状态和命令被无条件使用 | 直接编译阻塞 | P0 |
| 音频感知 | `audio.rs` 使用 WASAPI session meter | macOS 无实现 | P1 |
| 媒体控制 | `media.rs` 使用 Windows SMTC 和 endpoint volume | macOS 无实现 | P1 |
| 全局光标 | `cursor.rs` 在非 Windows 上直接返回 | 点击穿透/悬停逻辑失效 | P0 |
| 空闲检测 | `idle.rs` 在非 Windows 上直接返回 | 自动飞行屏保失效 | P1 |
| 窗口原子定位 | `window_ops.rs` 非 Windows 返回成功但不执行 | 卡牌模式可能出现跳动或尺寸错误 | P0 |
| 联网搜索 | SERP 基础导航依赖 WebView；正文回读硬编码 WebView2 COM | macOS 编译或正文深挖能力受阻 | P1 |
| 文件导出 | 保存逻辑跨平台；`reveal_in_folder` 非 Windows 静默成功 | Finder 中显示功能失效 | P1 |
| 硬件/系统状态 | CPU、内存等有通用实现；GPU、物理盘等偏 Windows | 信息不完整，需要 capability 标记 | P2 |
| 凭据保存 | `keyring` 已启用 `apple-native` | 可复用 macOS Keychain，需实测 | P0 |
| 快捷键/自启动/托盘 | 使用 Tauri 官方插件 | 理论可跨平台，需验证快捷键习惯和 macOS 生命周期 | P0 |
| 安装包 | `tauri.conf.json` 仅配置 NSIS | 无 macOS bundle/DMG | P0 |
| CI/发布 | release 与 staging 均为 `windows-latest` | 无 macOS 构建、签名、更新产物 | P0 |

当前开发机为 Apple Silicon macOS，Rust stable 与 Tauri 构建工具链已安装；`cargo check`、`cargo test`、前端测试、前端构建和 `tauri dev` 已形成可复现基线。

当前实施进度（2026-08-21）：

- Phase 1 编译/启动基线已完成，未实现能力通过 capability 显式降级。
- Phase 2 已完成 Dock 生命周期、透明窗口启动契约、全局光标、点击穿透协调和原子窗口几何；多显示器、缩放与负坐标仍需真机矩阵验收。
- Phase 3 已完成 Finder 显示、macOS 快捷键符号、公开 API 空闲检测、GPU/物理磁盘硬件详情，以及默认网络的 Wi‑Fi/Ethernet/其他/离线分类。空闲检测同时读取 CoreGraphics 全局输入时间与 IOKit `PreventUserIdleDisplaySleep` 聚合断言；采样失败时自动飞行保持关闭。硬件详情使用结构化 `system_profiler` JSON 与 `diskutil` plist；网络类型使用公开 SystemConfiguration 动态存储和接口 API，均已通过本机只读冒烟。自启动与系统凭据存储的失败状态已对用户可见；LaunchAgent 配置文件的启用状态往返、Keychain 的读写删除和单实例前台激活均已通过真机测试。实际登录启动、正常 UI 退出后的 socket 清理和休眠/锁屏/切换用户/显示器热插拔仍待真机矩阵验收。
- Phase 4A 音频/媒体公开 API spike 已完成，详细决策与兼容性证据见
  [`MACOS-MEDIA-SPIKE.md`](MACOS-MEDIA-SPIKE.md)。macOS 首版使用公开
  CoreAudio 输出设备 I/O 状态驱动音频活动动画，并明确标为 `degraded`；
  跨应用元数据/控制保持 `unavailable`，不采用私有 `MediaRemote` 或需要
  系统音频捕获权限的振幅采样。
- Phase 4B 已通过 WKWebView 原生 `evaluateJavaScript` completion callback
  补齐正文深挖；脚本结果由 Rust 主动拉取，任意正文域没有加入 Tauri
  capability。Apple Silicon 真机 14 组 DuckDuckGo 中/日/英质量用例全部
  完成，5 组成功从正文提取补强结果，未出现脚本超时、回调丢失或执行错误。
- Phase 5A 已将 About 功能清单接入运行时 capability：macOS 仅展示
  已实现的输出设备 I/O 音频活动，不再宣称或展示不可用的
  SMTC 媒体控制。平台差异文案已补齐 en/zh/ja，README 与下载说明
  明确标记 Windows 已发布、macOS 13+ 适配中且尚无公开签名 DMG。

## 5. 总体技术方案

### 5.1 平台模块边界

保持前端调用的命令名和事件名稳定，把平台差异收敛到 Rust 内部：

```text
src-tauri/src/platform/
├── mod.rs
├── capabilities.rs
├── audio/
│   ├── mod.rs
│   ├── windows.rs
│   └── macos.rs
├── media/
│   ├── mod.rs
│   ├── windows.rs
│   └── macos.rs
├── cursor/
├── idle/
├── window/
└── webview/
```

实施时不要求一次性移动全部文件。优先建立接口和 macOS 编译路径，再按模块逐步迁移，以降低 Windows 回归风险。

### 5.2 Capability 模型

新增一个统一命令，例如 `get_platform_capabilities`，至少返回：

- `audioActivity`
- `mediaMetadata`
- `mediaTransport`
- `systemVolume`
- `idleDetection`
- `globalCursor`
- `deepWebSearch`
- `hardwareDetails`
- `revealInFolder`

每项应区分：

- `available`：当前平台和系统版本可用；
- `permissionRequired`：需要用户授权；
- `unavailable`：当前实现不支持；
- `degraded`：功能可用但能力不完整。

前端根据 capability 隐藏、禁用或解释相关入口。所有用户可见说明必须进入现有 `en` / `zh` / `ja` i18n 系统。

### 5.3 安全和隐私原则

1. 透明桌宠允许一个严格限定的例外：仅启用 Tauri/Wry 为透明
   `WKWebView` 使用的 `macos-private-api`，仅用于官网/GitHub 直分发版本；
   不扩大到其他系统集成，也不以此版本申请 Mac App Store 上架。
2. 不为了音乐控制引入 `MediaRemote` 等私有 Apple API。
3. 不扩大任意远程网页的 Tauri IPC 权限。
4. 麦克风、辅助功能、屏幕与系统音频等权限按需申请；拒绝授权不能阻止应用启动。
5. API Key 继续只写入系统凭据存储，macOS 使用 Keychain。
6. 所有原生调用都提供错误和降级状态，不使用“返回成功但什么也没做”的 fallback。

## 6. 分阶段实施计划

### Phase 0：构建基线与决策锁定

工作项：

1. 安装 Rust stable、Xcode Command Line Tools 和 Tauri macOS 构建依赖。
2. 记录 `npm test`、`npm run build`、`cargo test`、`cargo check` 的基线结果。
3. 锁定最低 macOS 版本、架构、分发方式和媒体功能边界。
4. 为 Windows 和 macOS 建立功能矩阵，确认 MVP 范围。

验收：

- 能稳定复现当前 macOS 编译错误并形成清单。
- review 本文第 3 节中的所有产品决策。
- Windows 现有测试基线有记录。

### Phase 1：打通 macOS 编译和启动

工作项：

1. 拆分 `audio`、`media` 的公共接口与平台实现。
2. 修复 `lib.rs` 中无条件创建 Windows-only 状态和注册命令的问题。
3. 引入 capability 命令；macOS 尚未实现的能力返回明确状态。
4. 处理 `search/webview.rs` 中的 WebView2 专属类型，使 macOS 可以编译。
5. 补齐最小 macOS bundle 配置与 `.icns` 图标。

验收：

- `cargo check`、`cargo test`、`npm test`、`npm run build` 在 macOS 通过。
- `npm run tauri dev` 可启动主窗口、设置窗口和关于窗口。
- 未实现的功能不会导致 panic，也不会伪装成成功。

### Phase 2：桌宠核心窗口体验

工作项：

1. 验证透明背景、无边框、置顶、阴影和窗口层级。
2. 实现 macOS 全局光标位置采集，恢复逐像素点击穿透。
3. 为窗口尺寸与位置更新实现 macOS 原子路径，修复卡牌模式切换。
4. 验证拖拽、屏幕边缘、多显示器、不同缩放比例和负坐标。
5. 验证菜单栏图标、显示/隐藏、退出和多窗口聚焦。
6. 保持 Dock 图标常驻，并验证 Dock 点击、应用激活和设置窗口聚焦策略。

验收：

- 透明区域不拦截下层应用点击，角色可交互区域保持可点击。
- 主窗口在单屏、双屏和不同缩放比例下不会跳动、越界或丢失。
- 托盘、隐藏、再次显示和完全退出均符合 macOS 习惯。
- Dock 图标始终可见，点击后能正确激活应用并恢复可用窗口。
- 卡牌、聊天、设置和关于窗口开关无闪烁或尺寸错位。

### Phase 3：系统集成

工作项：

1. 实现系统空闲时间采集和睡眠/唤醒恢复，接回飞行屏保状态机。
2. 验证并调整全局快捷键；macOS 显示形式改为 `⌃` / `⌥` / `⌘` 语义。
3. 验证 LaunchAgent 自启动、单实例和 Keychain。
4. 完善系统状态和硬件信息的 macOS 实现或降级展示。
5. 使用 Finder 实现 `reveal_in_folder`。
6. 检查休眠、锁屏、切换用户、外接显示器插拔后的状态恢复。

验收：

- 自动飞行只在满足空闲条件时触发，睡眠唤醒后不会误触发。
- 权限拒绝、快捷键冲突和自启动失败均有明确状态。
- API Key 不会以明文出现在应用数据或日志中。

Apple Silicon 真机单实例记录（2026-08-21）：

- 首实例启动后创建 `/tmp/com_mutsumi_app_si.sock`；第二实例在约 0.8 秒内以状态 0 退出，首实例持续运行。
- 在同一系统脚本中先将 Finder 置前，再启动第二实例，最终前台进程为 `Mutsumi`，证明通知回调会激活已有窗口。
- 强制终止留下的陈旧 socket 能在下一次启动时被插件识别并接管；本次测试产生的 socket 已清理。
- 标准 UI Quit 的 socket 清理仍需人工点按验证；自动发送 `Cmd+Q` 因当前终端未获 macOS 辅助功能“发送按键”权限而未执行，不据此判定应用失败。

### Phase 4：音频、媒体和联网搜索专项

#### 4A. 音频与媒体

先做技术 spike，再决定首发能力：

1. 评估公开 API 是否能可靠获得系统音频活动状态。
2. 评估公开 API 对跨应用 now-playing 元数据和播放控制的覆盖率。
3. 评估系统音量、静音和 seek 的可用性及权限成本。
4. 输出 Spotify、Apple Music、浏览器视频等实际兼容性报告。

决策顺序：

- 首选：公开 API，可靠且无需过度权限。
- 次选：保留音频活动动画，媒体元数据和控制显式降级。
- MVP fallback：隐藏音乐控制器相关入口，但不影响其他功能。
- 排除：依赖私有 `MediaRemote` 框架发布生产版本。

#### 4B. 联网搜索

1. 保留现有隐藏单例 WebView、cookie warm-up、串行导航和人工验证流程。
2. 为 WKWebView 实现正文页面脚本执行与结果回读。
3. 不把任意正文域加入可调用 Tauri IPC 的 capability 范围。
4. 保持超时、取消、休眠恢复和无网络情况下的安全降级。

Apple Silicon WKWebView 真机记录（2026-08-21）：

- 可重放命令为 `MUTSUMI_SERP_QUALITY=duckduckgo npm run tauri dev`；质量
  harness 使用 `src-tauri/src/search/bench.rs` 中固定的 14 组用例，
  macOS 报告写入 `~/Library/Logs/com.mutsumi.app/serp-quality-report.md`。
- DuckDuckGo 14 组中/日/英质量用例全部完成 SERP 渲染、解析和最多两条结果输出。
- 天气、股价和汇率等 5 组触发正文深挖并成功回读渲染后 HTML；其中一组首条正文无相关信息后继续使用第二条结果成功补强，验证了既有 fallback 顺序。
- 全程未出现 `evaluateJavaScript` 错误、回调 channel 丢失或脚本执行超时。
- 正文回读由 Rust 通过 WKWebView completion callback 拉取；`webview-serp` capability 仍只授权搜索引擎域，未加入任何正文结果域。
- macOS 13、Intel、断网/恢复、休眠/唤醒和真实 challenge 人工解锁仍进入发布前矩阵。

验收：

- 媒体 capability 与真实可用能力一致。
- 没有音乐权限或没有媒体会话时，桌宠仍正常运行。
- 支持的搜索引擎可返回渲染后 SERP；正文深挖成功或安全降级。
- 任意结果页不能调用应用命令或获得额外本地权限。

### Phase 5：跨平台 UI 与 i18n

工作项：

1. 前端按 capability 控制设置项、徽章、提示和控制按钮。
2. 为所有新增文案补齐 `Translations` 类型及 `en` / `zh` / `ja` 翻译。
3. 适配 macOS 字体、快捷键符号、滚动条和安全区域。
4. 更新 About、README、下载说明和功能差异说明。

当前记录（2026-08-21）：

- Settings 已根据 capability 隐藏不可用媒体面板、禁用不可用的
  闲置飞行，并对 macOS 降级音频活动给出三语说明。
- About 功能清单现在使用同一 capability contract；capability 未解析
  或 IPC 失败时保守地只展示平台无关功能，不会伪造媒体能力。
- README 已记录 Windows/macOS 功能差异、开发状态与当前下载边界。
- macOS 字体、滚动条、安全区和三种语言的视觉矩阵仍待真机验收。

验收：

- 不存在硬编码用户可见字符串。
- 三种语言下均无溢出、截断和错误占位。
- 不支持能力不会显示可点击但无反应的控件。

### Phase 6：构建、签名、更新与发布

工作项：

1. 配置 macOS app bundle、DMG、图标、标识符和最低系统版本。
2. 建立 arm64、x86_64 或 universal 构建流程。
3. 配置 Developer ID 签名、hardened runtime、entitlements 和 notarization。
4. 扩展 staging 与 release workflow，保留 Windows 构建并新增 macOS job。
5. 更新发布资产校验脚本，使 updater manifest 同时校验 Windows 和 macOS。
6. 完成从旧 macOS 版本到 staging 版本的真实升级测试。

验收：

- 新机器可从 DMG 安装，Gatekeeper 不显示未签名或损坏警告。
- `spctl`、签名校验和 notarization 检查通过。
- Apple Silicon 与 Intel 目标均可启动。
- 自动更新下载正确的平台产物，失败时不会破坏现有安装。
- Windows NSIS、签名和 updater 流程保持通过。

### Phase 7：回归与发布门禁

自动化门禁：

- Vue/TypeScript 构建与 Vitest。
- Rust 单元测试和 macOS/Windows `cargo check`。
- 平台抽象的 contract tests。
- release asset 与 updater manifest 校验。

真机冒烟矩阵：

- Apple Silicon：最低支持系统 + 当前系统。
- Intel：至少一台真机或可信的 CI/测试机。
- 单屏、外接屏、不同缩放比例。
- 启动、隐藏、显示、退出、登录自启和单实例。
- 睡眠/唤醒、锁屏/解锁、网络断开/恢复。
- 权限首次允许、拒绝以及在系统设置中撤销。
- 从 staging DMG 安装并完成一次真实自动更新。

## 7. 建议的提交/PR 拆分

1. `build(macOS): establish compile baseline and capabilities`
2. `feat(macOS): implement pet window and cursor interactions`
3. `feat(macOS): add idle, lifecycle, Finder and system integrations`
4. `feat(macOS): add media capability strategy and WKWebView search bridge`
5. `build(macOS): add signed universal bundle and release workflows`
6. `docs(macOS): publish support matrix and testing runbook`

每个 PR 都必须同时跑 Windows 回归；不得把全部平台改造积压到一个无法独立验证的大提交中。

## 8. 主要风险与缓解措施

| 风险 | 影响 | 缓解措施 |
| --- | --- | --- |
| macOS 缺少与 SMTC 对等的公开跨应用媒体 API | 音乐控制器无法功能对等 | 先 spike，按 capability 降级，不使用私有 API |
| 系统音频采集需要高权限或受系统版本限制 | 首次授权体验差，兼容范围缩小 | 音频功能与核心应用解耦，默认最少权限 |
| WKWebView 与 WebView2 行为差异 | 搜索正文回读、challenge 流程不稳定 | 建立平台 bridge 和真实引擎基准，保持超时降级 |
| 透明窗口和点击穿透存在系统差异 | 核心桌宠体验受损 | Phase 2 独立验收，多显示器真机测试 |
| 签名、notarization、updater 资产组合复杂 | 用户无法安装或升级 | staging 发布先行，发布脚本增加平台资产契约校验 |
| 平台抽象改动影响 Windows | 现有用户回归 | 保持命令/事件契约稳定，CI 双平台门禁 |

## 9. Review 清单

请重点确认：

- [ ] 最低系统版本是否采用 macOS 13。
- [ ] 是否要求首发同时支持 Intel，还是先 Apple Silicon。
- [ ] 是否接受首个 MVP 暂不提供跨应用媒体控制。
- [x] 除 Tauri/Wry 透明窗口所需的限定例外外，系统集成只使用公开 Apple API；媒体能力继续禁止私有框架。
- [x] Dock 图标常驻，同时保留菜单栏图标。
- [ ] 是否按 Phase 1 → 2 → 3 → 4 → 5 → 6 的顺序实施。
- [ ] 是否接受首版通过 GitHub Releases 分发签名/notarized DMG，不进入 Mac App Store。

## 10. 建议的首个实施切片

review 通过后，第一批只做以下内容：

1. 安装并记录 macOS 构建工具链。
2. 建立 capability 数据结构。
3. 将 `audio` / `media` 拆为公共接口、Windows 实现和 macOS unavailable stub。
4. 修复 WebView2 类型导致的 macOS 编译问题，但暂不实现完整 WKWebView 深挖。
5. 让应用在 macOS 上完成 `cargo check`、启动和基础窗口冒烟。

该切片不触碰媒体原生实现和正式发布配置，目标是尽快获得一个可持续迭代、不会破坏 Windows 的 macOS 基线。
