# macOS 桌面真机测试记录

> 测试窗口：2026-08-23 至 2026-08-25
>
> 记录更新：2026-08-25（本机自动化复验与产品 pending 决策）
>
> 分支：`feat/macos-adaptation`
>
> 当前证据锚点：`002c7a8 fix(macOS): respect user focus changes`
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
| Rust 测试 | 通过 | 347 项通过，36 项按设计 ignored |
| 发布、bundle、生命周期纯契约 | 通过 | 57/57；包含本轮新增请求 slice 执行预检 |
| universal app | 通过 | Mach-O 恰好包含 `arm64` 与 `x86_64` |
| 最低系统版本 | 通过 | `LSMinimumSystemVersion=13.0` |
| Dock 前台资格 | 通过 | `APPL`，非 `LSUIElement`，非 `LSBackgroundOnly` |
| unsigned DMG | 通过 | `mutsumi_1.5.3_universal.dmg` 生成，`hdiutil verify` 通过 |
| arm64 生命周期 | 通过 | 前台启动、第二实例复用/激活、标准 Quit、socket 清理 |
| x86_64 Rosetta 生命周期 | 通过 | 确认 `LSArchitecture=x86_64`，其余生命周期契约同样通过 |

2026-08-25 在 `4ed1d0d` 上重新执行本机可复现的 CI 主体：Vue/Vitest
387/387、发布/bundle/生命周期纯契约 57/57、前端生产构建、Rust 347 passed /
36 ignored（含 doctest）均通过；同一功能源码的现有 universal app/DMG 再次通过
unsigned bundle contract。当前受控 shell 的默认 `PATH` 未暴露 Cargo 工具链，
最终复验显式指定 rustup stable 的 `rustc` / `rustdoc` 后以零退出码完成；这不是
代码或远程 `desktop-ci` 的失败。

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

### 4.2 延迟重聚焦尊重用户主动切换

人工桌面验收已确认 Dock 图标常驻、透明宠物窗口、菜单栏入口、隐藏后点击 Dock
恢复、`设置 > 关于 > 主窗口` 的恢复优先级，以及标准第二实例复用/激活。更新窗口
没有可用更新载荷，本轮未验证其最高优先级。

主动切走边界暴露了原实现的缺口：第二实例回调即时聚焦后，无条件的一秒延迟重试
无法区分 LaunchServices 回弹和用户选择其他应用。本轮将重试改为：

- 继续保留即时聚焦，并在没有后续用户操作时补偿 LaunchServices 时序竞态。
- 使用公开 CoreGraphics 读取鼠标按下、键盘和滚动事件的空闲时间；检测到调度后
  超出 50 ms 容差的焦点相关输入时取消延迟重试，且忽略不会表达切换意图的被动
  鼠标移动。
- 使用公开 `mach_continuous_time` 计算包含系统睡眠的经过时间；超过两秒时视为
  陈旧任务并放弃，不在运行时停顿或睡眠后突然抢焦点。
- CoreGraphics 采样失败时保留原有补偿行为；纯决策覆盖有输入、无输入、采样失败、
  50 ms 容差边界、陈旧延迟、Mach tick 换算和连续时钟 live sample。

修复后的 universal app 已在 arm64 与 x86_64/Rosetta 下通过标准生命周期 smoke。
精确最终构建的首轮 arm64 smoke 曾出现一次前台激活超时，fallback cleanup 没有留下
进程、LaunchServices 注册或 socket；同一源码随后自动采样五类事件，确认程序化
Finder/第二实例期间各事件年龄均只随时间递增且最终前台为 Mutsumi，完整 arm64
重跑通过。该结果继续说明全局前台断言会受真实桌面操作影响，不把超时隐藏为通过。
自动化尝试用无副作用 F13 事件覆盖输入分支，但当前执行进程的
`CGPreflightPostEventAccess=false`，macOS 拒绝发布合成输入；因此最终物理输入端到端
确认仍保留为一项人工验收，不把未生效的合成事件记为通过。产品在 2026-08-25
决定将该项推迟到正式发布前处理，不作为当前 macOS 适配开发的阻塞门禁。

### 4.3 依赖审计提示

`npm ci` 在 2026-08-23 的 advisory 数据下报告 4 个 high 项，路径落在
Vite/PostCSS/nanoid/brace-expansion 的构建与测试依赖图。它们没有在本次 macOS
验证提交中自动升级，避免把依赖维护混入平台验收；后续应以独立安全维护提交更新
锁文件并同时跑 Windows/macOS 回归。没有执行 `npm audit fix`。

## 5. 尚未完成，不能标记通过

- 更新窗口存在可用更新载荷时的最高恢复优先级。
- 正式发布前：修复后单实例约 1 秒延迟期间由用户物理切走的一次最终端到端确认。
- 双屏、负坐标、不同缩放、主副屏切换和显示器热插拔；本机本轮只有单屏。
- 睡眠/唤醒、锁屏/解锁、网络物理断开/恢复。
- 真实登录启动；本轮只验证 LaunchAgent 后端往返。
- Keychain 首次允许、拒绝、锁定、撤销和重新授权的可见错误路径。
- 耳机、USB、HDMI、蓝牙、AirPlay 和暂时无默认输出设备的 CoreAudio 矩阵。
- 真实 Intel Mac；Rosetta 结果不能替代 Intel 硬件。
- 签名/notarized staging DMG 安装与真实跨版本自动更新。
- GitHub Draft PR 的 Windows regression 与 macOS universal 两个远程 job。
