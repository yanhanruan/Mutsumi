<p align="center">
  <img src="app-icon-cucumber-puppy-transparent-compress.png" alt="Mutsumi Logo" width="120" />
  <!-- 建议这里放一张角色的透明底头像 -->
</p>

<h1 align="center">Mutsumi (睦) — Desktop Companion</h1>

<p align="center">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2.0-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-Backend-000000?logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://vuejs.org/"><img src="https://img.shields.io/badge/Vue_3-Frontend-4FC08D?logo=vuedotjs&logoColor=white" alt="Vue 3"></a>
  <a href="#download"><img src="https://img.shields.io/badge/Platform-Windows-0078D6?logo=windows&logoColor=white" alt="Windows Only"></a>
</p>

<p align="center">
  <a href="#mutsumi--桌面陪伴宠物-中文"><b>中文</b></a> | <a href="#mutsumi--desktop-companion-english"><b>English</b></a>
</p>

<table align="center">
  <tr>
    <td align="center"><img src="docs/images/idle.avif"    width="200" alt="Mutsumi idle" /><br/><sub>🌿 静静陪伴 · just hanging out</sub></td>
    <td align="center"><img src="docs/images/headpat.avif" width="200" alt="Head pat" /><br/><sub>✋ 摸摸头 · head pat</sub></td>
    <td align="center"><img src="docs/images/tarot.avif"   width="200" alt="Tarot reading" /><br/><sub>🔮 塔罗占卜 · tarot reading</sub></td>
  </tr>
</table>

---


<h2 id="mutsumi--桌面陪伴宠物-中文">🌟 Mutsumi — 桌面陪伴宠物</h2>

<p align="center"><i>她不告诉你该怎么做。她只是安静地在那里等你。</i></p>

Mutsumi 是一个静静住在你屏幕角落的小伙伴。
她不会打扰你工作，只是自顾自地待在那里。当你播放音乐时，她会默默戴上耳机感受节奏；当你感到疲惫时，可以戳戳她、喂杯茶。得益于 **Tauri 2 + Rust** 的底层驱动，她非常轻量（几十 MB），几乎不占用系统资源。

### ✨ 核心体验

👻 **无感陪伴，穿透点击**：始终悬浮在屏幕最前方，但透明区域完全穿透，绝不遮挡你点击底部的代码或网页。你可以随时把她拖拽到屏幕的任意角落。

🎧 **全局音频感知（无需配置）**：当你开始播放音乐或看视频时，她会立刻戴上耳机跟着节奏晃动；声音停止，她会摘下耳机恢复平静。

🍅 **极客番茄钟 & 天气**：内置轻量级专注/休息计时器，并在角落安静地展示实时天气。

🎮 **丰富的互动菜单**：右键点击她，即可呼出互动面板：
* ✋ **摸摸头**：召唤哈基米摸摸头
* 🍨 **喂抹茶芭菲**：抹茶芭菲最喜欢了！
*  **让她睡觉 / 隐藏**：让她躲起来
* 🔮 **每日塔罗占卜**：每天抽取3张专属塔罗牌，带来每天不一样的未知惊喜与指引。

<h3 align="center">🎧 睦子米喜欢跟着节奏摇摆</h3>

<p align="center">
  <img src="docs/images/music1.avif" width="176" alt="music reaction 1" />
  <img src="docs/images/music2.avif" width="176" alt="music reaction 2" />
  <img src="docs/images/music3.avif" width="176" alt="music reaction 3" />
  <img src="docs/images/music4.avif" width="176" alt="music reaction 4" />
  <img src="docs/images/music5.avif" width="176" alt="music reaction 5" />
</p>


<h3 align="center">🔮 塔罗牌样例</h3>

<table align="center">
  <tr>
    <td align="center"><img src="public/assets/tarot/Cups03.avif"   width="120" alt="Three of Cips" /></td>
    <td align="center"><img src="public/assets/tarot/15-TheDevil.avif"  width="120" alt="The Devil" /></td>
    <td align="center"><img src="public/assets/tarot/19-TheSun.avif"    width="120" alt="The Sun" /></td>
    <td align="center"><img src="public/assets/tarot/Wands04.avif"   width="120" alt="Four of Wands" /></td>
    <td align="center"><img src="public/assets/tarot/21-TheWorld.avif"  width="120" alt="The World" /></td>
  </tr>
  <tr>
    <td align="center"><img src="public/assets/tarot/Cups06.avif"       width="120" alt="Six of Cups" /></td>
    <td align="center"><img src="public/assets/tarot/Cups13.avif"       width="120" alt="Queen of Cups" /></td>
    <td align="center"><img src="public/assets/tarot/Wands05.avif"      width="120" alt="Five of Wands" /></td>
    <td align="center"><img src="public/assets/tarot/Wands13.avif"      width="120" alt="Queen of Wands" /></td>
    <td align="center"><img src="public/assets/tarot/Pentacles05.avif"  width="120" alt="Five of Pentacles" /></td>
  </tr>
</table>
<p align="center"><sub><i>共 78 张卡片——每天都有不同的惊喜！（卡牌仅供娱乐）</i></sub></p>



下个版本即将推出🚀：休闲小音游、屏保飞行模式.....



### ⚙️ 偏好设置

右键点击系统托盘（右下角）的 Mutsumi 图标，可以自定义：`番茄钟时长` / `角色体型 (小/中/大)` / `天气开关` / `界面语言`

<p align="center"><img src="docs/images/setting.avif" width="440" alt="Settings window" /></p>

---

### 🚀 快速获取

**直接安装（推荐）：**
前往 [Releases 页面](../../releases) 获取最新的 `.exe` 可执行文件，双击即可召唤小黄瓜。（目前仅支持 Windows）

**面向开发者的源码构建：**
如果你想研究 Tauri 的异形窗口或系统音频捕获逻辑，欢迎克隆代码！
环境要求：`Node.js (18+)` + `Rust 环境`

```bash
git clone <你的仓库地址>
cd Mutsumi
npm install
npm run tauri dev
```

*(首次运行会编译 Rust 核心依赖，大约需要喝杯茶的时间 🍵，后续启动秒开)*

### 🛠️ 技术架构

Tauri 2（Rust 后端）+ Vue 3 前端，打包为单文件原生 Windows 应用。

```
Mutsumi/
├── src/                        Vue 前端
│   ├── components/             UI 组件（PetWindow、SettingsWindow 等）
│   ├── composables/            逻辑钩子（动画、音频感知、天气、番茄钟…）
│   ├── i18n/locales/           多语言支持（中文 / 英文 / 日文）
│   └── data/                   台词与对话数据
├── src-tauri/src/              Rust 后端
│   ├── audio.rs                全局系统音频监听（WASAPI）
│   ├── weather.rs              天气数据获取与缓存
│   ├── pomodoro.rs             专注 / 休息状态机
│   ├── state.rs                宠物状态（精力、好感度、心情）
│   ├── persistence.rs          本地 JSON 持久化
│   ├── tray.rs                 系统托盘菜单
│   ├── cursor.rs               光标距离检测
│   ├── late_night.rs           深夜模式行为
│   └── idle.rs                 待机行为逻辑
└── public/assets/              动画帧资源（WebP 序列）
```

### 🤝 参与贡献

欢迎任何形式的贡献！如果你有想法、发现了 Bug，或者想为动画、台词、多语言翻译出一份力：

1. Fork 本仓库
2. 新建分支：`git checkout -b feat/你的想法`
3. 提交改动并发起 Pull Request

如果只是想聊聊设计思路或报告问题，直接开一个 [Issue](../../issues) 就好。

如果喜欢这个项目，感谢你的star⭐——这对我们帮助很大！

---

<h2 id="mutsumi--desktop-companion-english">🌟 Mutsumi — Desktop Companion</h2>

<p align="center"><i>She doesn't tell you what to do. She's just quietly there, waiting.</i></p>

Mutsumi is a quiet little companion living in the corner of your screen. She minds her own business, puts on her headphones when you play some tunes, and reacts when you interact with her. Powered by **Tauri 2 + Rust**, she is exceptionally lightweight (only a few dozen megabytes) and consumes virtually no system resources.

### ✨ Core Features

👻 **Always There, Never in the Way**
 Stays on top of all your windows, while fully transparent areas let your clicks pass straight through. Your code, browser, and apps remain completely accessible. Drag her anywhere on your screen at any time.

🎧 **Automatic Audio Awareness**
 Start playing music or a video, and she'll instantly put on her headphones and groove along to the beat. When the audio stops, she'll take them off and quietly return to idle.

🍅 **Built-in Pomodoro Timer & Weather**
 Stay focused with a lightweight Pomodoro timer and keep an eye on the current weather, conveniently displayed in the corner of your screen.

🎮 **Fun Interactive Menu**
 Right-click her to open a menu full of interactions:

- ✋ **Pat Her Head** — Give her a gentle head pat.
- 🍨 **Feed Her a Matcha Parfait** — Her favorite treat!
- 😴 **Put Her to Sleep / Hide Her** — Let her take a break and disappear from view.
- 🔮 **Daily Tarot Reading** — Draw three tarot cards each day for a fresh dose of mystery, surprises, and inspiration.

<h3 align="center">🎧 Mutsumi Loves Bopping Along to Your Music</h3>

<p align="center">
  <img src="docs/images/music1.avif" width="105" alt="music reaction 1" />
  <img src="docs/images/music2.avif" width="105" alt="music reaction 2" />
  <img src="docs/images/music3.avif" width="105" alt="music reaction 3" />
  <img src="docs/images/music4.avif" width="105" alt="music reaction 4" />
  <img src="docs/images/music5.avif" width="105" alt="music reaction 5" />
</p>

### 🚀 Getting Started

**Option 1 — Download (Easiest):**
Grab the latest `.msi` installer from the [Releases page](../../releases) and double-click to install. *(Windows only for now)*

**Option 2 — Build from Source (For Geeks):**
Want to see how we handle transparent windows or global audio capture in Tauri? Clone away!
Requirements: `Node.js (18+)` + `Rust`

```bash
git clone <repo-url>
cd Mutsumi
npm install
npm run tauri dev
```

*(The first Rust compilation might take a few minutes — perfect time to grab a cup of tea 🍵. Subsequent builds are blazing fast)*

### ⚙️ Settings

Right-click her icon in the system tray (bottom-right corner) to tweak: `Pomodoro Durations` / `Character Size (S/M/L)` / `Weather Toggle` / `Language`

<p align="center"><img src="docs/images/setting.avif" width="440" alt="Settings window" /></p>

🚀 Coming soon: Casual Rhythm Game, Flying Screensaver Mode...

### 🛠️ Architecture

Tauri 2 (Rust backend) + Vue 3 frontend, bundled as a single native Windows app.

```
Mutsumi/
├── src/                        Vue frontend
│   ├── components/             UI components (PetWindow, SettingsWindow, etc.)
│   ├── composables/            Logic hooks (animation, audio, weather, pomodoro…)
│   ├── i18n/locales/           Translations (Chinese / English / Japanese)
│   └── data/                   Dialogue and quote data
├── src-tauri/src/              Rust backend
│   ├── audio.rs                Global system audio monitoring (WASAPI)
│   ├── weather.rs              Weather fetching and caching
│   ├── pomodoro.rs             Focus / break state machine
│   ├── state.rs                Pet state (energy, affection, mood)
│   ├── persistence.rs          Local JSON persistence
│   ├── tray.rs                 System tray menu
│   ├── cursor.rs               Cursor proximity detection
│   ├── late_night.rs           Late-night mode behaviour
│   └── idle.rs                 Idle behaviour logic
└── public/assets/              Animation frames (WebP sequences)
```

### 🤝 Contributing

Contributions of any kind are welcome! Whether you have an idea for a new feature, found a bug, or would like to help with animations, dialogue, or translations, we'd love to hear from you.

1. Fork the repo
2. Create a branch: `git checkout -b feat/your-idea`
3. Commit your changes and open a Pull Request

Just want to share a thought or report something? Feel free to open an [Issue](../../issues) 

---

If you like this project, consider giving it a ⭐ — it helps a lot!
