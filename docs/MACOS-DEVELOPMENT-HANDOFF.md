# Mutsumi macOS 开发交接与续作指南

> 更新时间：2026-08-25
>
> 工作分支：`feat/macos-adaptation`
>
> 主分支基线：`main` / `d3b52a9`
>
> 续作起点：`d54d0c5 ci: upgrade official actions to Node 24`

这份文档用于在另一台 Mac 上恢复开发上下文。产品与技术决策仍以
[`MACOS-ADAPTATION-PLAN.md`](MACOS-ADAPTATION-PLAN.md) 为准，发布边界以
[`RELEASING.md`](RELEASING.md) 为准；最新本机证据见
[`MACOS-DESKTOP-TEST-RECORD.md`](MACOS-DESKTOP-TEST-RECORD.md)，本文只负责
“如何接上当前进度”。

## 1. 十分钟恢复工作区

新克隆：

```bash
# 已配置 GitHub SSH key：
git clone git@github.com:yanhanruan/Mutsumi.git

# 或使用 HTTPS：
# git clone https://github.com/yanhanruan/Mutsumi.git

cd Mutsumi
git fetch origin
git switch --track origin/feat/macos-adaptation
```

已有本地仓库时，先确认自己的修改，不要用 reset 覆盖未提交内容：

```bash
git status
git fetch origin
git switch feat/macos-adaptation
git pull --ff-only
```

确认分支和功能锚点：

```bash
git status --short --branch
git log --oneline --decorate -5
git log --oneline main..HEAD
```

## 2. 开发环境

CI 使用 Node.js 22 和 Rust stable。个人 Mac 还需要 Xcode Command Line
Tools、npm、Tauri 的两个 macOS Rust target；Apple Silicon 上测试 Intel slice
时还需要 Rosetta 2。

```bash
xcode-select -p
node --version
npm --version
rustc --version
cargo --version

rustup toolchain install stable
rustup override set stable
rustup target add --toolchain stable aarch64-apple-darwin x86_64-apple-darwin
rustc --version
npm ci
```

`rustup override set stable` 只作用于当前仓库目录，用于确保这里的 Cargo/Tauri
命令与 CI 一样使用 stable；它不会改变其他 Rust 项目的目录 override。

快速验证环境：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
npm run tauri dev
```

注意：通过 Settings 保存的 Qwen API Key 位于每台机器自己的系统 Keychain，
个人电脑首次使用时需要重新填写。开发和 live tests 还可能使用被 `.gitignore`
排除的 `src-tauri/.env`，例如 `DASHSCOPE_API_KEY`、Fish Audio token 和
reCAPTCHA；它同样不会随 Git 同步。如确有需要，应通过安全渠道在新机器重建，
不要提交或粘贴私钥、API Key、Developer ID 证书和 notarization 凭据到仓库。

## 3. 当前已经完成的范围

截至功能锚点 `002c7a8`：

- Phase 1：macOS 编译/启动基线、capability 模型和不可用能力的显式降级。
- Phase 2：透明桌宠窗口、全局光标、点击穿透、原子窗口几何、Dock 常驻契约，
  以及 LaunchServices 单实例激活/退出 smoke。
- Phase 3：Finder 显示、macOS 快捷键、自启动和 Keychain 后端、公开 API
  空闲检测、GPU/磁盘详情与默认网络类型。
- Phase 4：CoreAudio 音频活动降级方案和 WKWebView 正文深挖；不引入
  `MediaRemote` 等私有媒体 API。
- Phase 5：About capability 展示、三语平台差异文案，以及设置/关于/更新窗口
  的 macOS 自绘标题栏。
- Phase 6A–6C：Windows/macOS 桌面 CI、universal unsigned bundle contract、
  updater/release asset contract，以及尚未接线的 signed bundle gate。

最近一次完整 app/DMG 本机基线（`4ed1d0d`）：

- Rust：347 passed / 36 ignored。
- 前端：25 files / 387 passed。
- 发布、bundle 与生命周期纯契约：57/57 passed。
- universal app 同时包含 `arm64` 和 `x86_64`，最低系统版本为 macOS 13.0。
- unsigned DMG 已在正常交互式用户会话中生成并通过 `hdiutil verify`；受控文件
  沙箱仍不能代表完整的 `hdiutil`/Finder DMG 环境。
- Dock 生命周期 smoke 在 Apple Silicon 原生 arm64 和 Rosetta x86_64 下通过。
- Dock 常驻、透明窗口、菜单栏入口、隐藏恢复、设置/关于恢复优先级和标准第二实例
  激活已通过人工验收；延迟重聚焦会在检测到用户焦点相关输入或陈旧任务时取消。
  当前执行进程没有发布合成输入的辅助功能权限；产品已将修复后的物理切走端到端
  确认保留为正式发布前 pending，不阻塞当前适配开发。

2026-08-25 已在 `4ed1d0d` 重新执行本机可复现的 CI 主体与 unsigned bundle
contract，以上 Vue、纯契约、前端构建、Rust 和 universal app/DMG 结果保持通过。
当前 identifier 切片又通过前端 387/387、Rust 348 passed / 36 ignored 与纯契约
59/59，并用 app-only universal 构建确认新 identifier、macOS 13.0 和双架构；
本轮本机没有重新生成 DMG，必须由该切片推送后的 Draft PR `desktop-ci` 重建
app/DMG 后，才能把新 identifier 的完整 unsigned bundle 证据记为通过。

Draft PR [#18](https://github.com/yanhanruan/Mutsumi/pull/18) 已创建；最终证据锚点
`d54d0c5` 的
[`desktop-ci` run 32815861642](https://github.com/yanhanruan/Mutsumi/actions/runs/32815861642)
在 GitHub-hosted runner 上完整通过：Windows regression 2 分 41 秒，macOS
unsigned universal bundle 7 分 28 秒。后者重新构建 app/DMG、通过 bundle
contract 并上传七天保留的未签名 artifact。checkout/setup-node/upload-artifact
已升级到官方 v7，首轮 run 的 Node.js 20 Action 弃用提示在最终 run 中消失。
唯一剩余 annotation 是已知的 identifier 正式签名前门禁。

Rosetta 结果只证明 Intel slice 能在 Apple Silicon 转译环境中运行，不替代真实
Intel Mac 验收。

## 4. 日常验证命令

普通提交至少运行与改动相关的测试；跨模块或准备 PR 时运行完整集合：

```bash
npm test
npm run test:release-contract
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

构建与 CI 一致的未签名 universal app/DMG：

```bash
npx tauri build \
  --target universal-apple-darwin \
  --bundles app,dmg \
  --ci \
  --no-sign

node scripts/verify-macos-bundle.mjs \
  --bundle-dir src-tauri/target/universal-apple-darwin/release/bundle \
  --mode unsigned \
  --expected-minimum-system-version 13.0
```

若本机 DMG 工具链失败，可先用 `--bundles app` 验证应用本体，但这不能代替
CI 中的 DMG 生成和 `hdiutil verify`。

生命周期 smoke 会主动切换前台应用。运行前先正常退出所有 Mutsumi 实例，执行
期间不要点击其他窗口或切换 Space；脚本检测到同 bundle identifier 或相同
executable 的已有实例时会拒绝继续：

```bash
npm run test:macos-lifecycle-smoke -- \
  --app src-tauri/target/universal-apple-darwin/release/bundle/macos/mutsumi.app \
  --arch arm64

# 仅限已安装 Rosetta 的 Apple Silicon，或对应 Intel 环境；脚本会在启动前
# 直接探测并拒绝当前系统不能执行的目标 slice：
npm run test:macos-lifecycle-smoke -- \
  --app src-tauri/target/universal-apple-darwin/release/bundle/macos/mutsumi.app \
  --arch x86_64
```

Keychain 与 LaunchAgent live tests 会短暂写入隔离的测试条目，默认 ignored，
需要明确选择后才运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  macos_system_integration_tests::keychain_backend_round_trip \
  -- --ignored --nocapture

cargo test --manifest-path src-tauri/Cargo.toml \
  macos_system_integration_tests::launch_agent_backend_round_trip \
  -- --ignored --nocapture
```

## 5. 接下来最值得继续的工作

Draft PR 与首次双平台远程 CI 已完成。后续优先级建议如下；人工/硬件项统一保留到
自动化开发结束后执行，pending 不代表发布通过：

1. 继续记录可用设备上的真实桌面矩阵；缺少显示器、权限状态或外设时保留 pending，
   不让非阻塞人工项中断可自动完成的开发，但仍不得据此宣称发布门禁通过。
2. 有真实 Intel Mac 时，下载同一 universal 构建验证启动、单实例与正常退出；
   没有真机就继续把它保留为待验收，不能用 Rosetta 冒充完成。
3. macOS identifier 已冻结为 `io.github.yanhanruan.mutsumi`；下一步接入
   Developer ID、notarization、stapling 和 macOS updater/release workflow，
   但没有凭据时不得伪造 signed 正例。

单纯 push `feat/macos-adaptation` 不会通过 workflow 的 `push` 入口触发
`desktop-ci`，因为该入口只监听 `main`；当前 Draft PR #18 会让后续相关路径变更
通过 `pull_request` 事件继续运行 Windows 和 macOS 两个 job。

## 6. 尚未完成的真机矩阵

- [ ] 从 Finder、Dock 和菜单栏分别启动/激活应用。
- [ ] 隐藏主窗口后点击 Dock，确认恢复和聚焦；检查设置、关于、更新窗口。
- [ ] 正式发布前启动第二实例，在约 1 秒重试窗口内用物理鼠标或键盘切到其他应用，
      确认 Mutsumi 不会抢回焦点并记录结果。
- [ ] 单屏、双屏、负坐标、不同缩放比例及主副屏切换。
- [ ] 外接显示器插拔后的窗口位置、点击穿透和动画恢复。
- [ ] 睡眠/唤醒、锁屏/解锁、网络断开/恢复。
- [ ] 实际登录启动，而不只是 LaunchAgent plist 往返测试。
- [ ] Keychain 首次允许、拒绝、锁定以及重新授权时的可见错误状态。
- [ ] CoreAudio 在扬声器、耳机、蓝牙设备切换和暂时无输出设备时的行为。
- [ ] 真实 Intel Mac 启动与生命周期 smoke。
- [ ] staging DMG 安装及真实跨版本自动更新；必须等签名发布链就绪。

## 7. 已知边界，不要误判为已完成

- macOS 产物目前是未签名 CI baseline，不可对用户分发。
- `release.yml` 和 `staging-release.yml` 目前仍是 Windows-only。
- 长期 macOS identifier 已冻结为 `io.github.yanhanruan.mutsumi`，仅写在
  macOS overlay；根配置的 `com.mutsumi.app` 保留给 Windows，避免迁移现有
  app-data 与凭据命名空间。后续不得再把二者误当成同一个跨平台 identifier。
- signed bundle contract 已写好，但没有真实 Developer ID/notarized 正例；启用
  workflow 时必须用刚签出的 Mutsumi 产物跑完整 gate。
- `macos-private-api` 例外只允许 Tauri/Wry 的透明 WKWebView，且只面向
  官网/GitHub 直分发；不得扩大到 `MediaRemote` 等私有系统能力。
- 当前 singleton refocus 在回调后约 1 秒有界重试。原生与 Rosetta smoke 已通过；
  公开 CoreGraphics 输入采样和 Mach 连续时钟已覆盖主动切走与睡眠后陈旧任务，
  纯决策测试已通过，最终 OS 级物理输入端到端确认保留到正式发布前。
- 仓库现有 Rust 格式并非全量 `cargo fmt --check` clean；不要为了一个功能提交
  机械格式化无关历史文件。只保持本次 diff 聚焦并运行 `git diff --check`。

## 8. Codex 协作与提交规则

项目 canonical 规则的唯一来源是 [`rules/AGENTS.md`](rules/AGENTS.md)。它要求：

- 每次 commit 前必须新建 5.6-sol/high reviewer，对完整 staged diff 按 PRD 做只读审查。
- 主 agent 要独立核实报告，修复确定问题；任何审查后改动都必须重新 review。
- 用户已在 2026-08-25 授权持续开发期间自主提交：reviewer 通过后可直接 commit，
  不再逐次请求确认；用户随后明确授权维护当前分支 push 与 Draft PR #18。该授权
  不包含 merge、转为 Ready for review、发布、签名或其他远程状态变更。

本次 macOS 分支另有一项明确的交接约定：按模块暂存和提交，不能夹带无关
文件；达到一次 commit 的标准时进入上述 review/commit 流程并自主继续。

当前工作电脑根目录的 `AGENTS.md` 是未跟踪的本地导入文件，不会随 Git 分支同步。
若个人电脑上的 Codex 没有自动加载项目规则，可在仓库根目录创建本地
`AGENTS.md`，内容保持为：

```md
# Codex rules for this project

@docs/rules/AGENTS.md
```

该本地导入文件不要顺手混入功能 commit；是否长期跟踪它应单独决定。

## 9. 结束一次开发前

```bash
git status --short
git diff --check
git log --oneline --decorate -5
git push origin feat/macos-adaptation
```

确认远程包含最新 commit 后，另一台电脑只需 `git fetch`、切换同一分支并
`git pull --ff-only`。如果有未提交工作，应先提交到独立 WIP 分支或生成 patch，
不要依赖构建目录、Keychain、环境变量或未跟踪文件自动同步。
