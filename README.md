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

<p align="center"><i>就算是黄瓜,也需要时间才能长大，好事不能着急 🌱</i></p>

Mutsumi 是一个静静住在你屏幕角落的小伙伴。
她不会打扰你工作，只是自顾自地待在那里。当你播放音乐时，她会默默戴上耳机感受节奏；当你感到疲惫时，可以戳戳她、喂杯茶。得益于 **Tauri 2 + Rust** 的底层驱动，她非常轻量（几十 MB），几乎不占用系统资源。

### ✨ 核心体验

👻 **无感陪伴，穿透点击**：始终悬浮在屏幕最前方，但透明区域完全穿透，绝不遮挡你点击底部的代码或网页。你可以随时把她拖拽到屏幕的任意角落。

🎧 **全局音频感知（无需配置）**：当你开始播放音乐或看视频时，她会立刻戴上耳机跟着节奏晃动；声音停止，她会摘下耳机恢复平静。

🎵 **迷你音乐控制器**：右下角常驻一个会随音乐律动的音响小图标。悬停即可展开控制面板——播放 / 暂停、上一首 / 下一首、快进 / 快退 10 秒、重播、系统音量与静音，并显示当前曲目、歌手和播放进度，点击或拖动进度条即可跳转到任意位置。当多个应用同时在播放时，还能切换音源并自动跟随最新的播放会话。基于 Windows 系统媒体控件（SMTC），能控制任何正在播放的应用：Spotify、网易云音乐、浏览器等。可在设置中随时开关。

<p align="center"><img src="docs/images/music-controller.avif" width="100%" alt="music controller" /></p>

<p><sub><i>经测试部分软件（网易云、QQ音乐等）没有应用底层进度条控制接口，因此仅支持播放 / 暂停、上一首 / 下一首、系统音量与静音，potplayer、spotify、chrome、edge等音频源均支持完整控制功能</i></sub></p>

🍅 **极客番茄钟 & 天气**：内置轻量级专注/休息计时器，并在角落安静地展示实时天气。

🎮 **丰富的互动菜单**：右键点击她，即可呼出互动面板：
* ✋ **摸摸头**：召唤哈基米摸摸头
* 🍨 **喂抹茶芭菲**：抹茶芭菲最喜欢了！
*  **让她睡觉 / 隐藏**：让她躲起来
* 🔮 **每日塔罗占卜**：每天抽取3张专属塔罗牌，带来每天不一样的未知惊喜与指引。

💬 **和睦聊天**：无论是太阳天还是雨天，睦头都会安静地陪在你身边：
* 🥒 **小黄瓜**：为了尽可能还原睦头，我们结合剧情内容、角色设定、荣格八维与九型人格分析，对角色进行了长期拆解与建模。通过 Prompt Engineering 与行为约束设计，实现睦头安静而真诚的性格，而不是千篇一律的 AI 回复。
* 🧠 **长期记忆**：她会记得你们聊过的事情：家里的猫、最近的工作、偶尔提起的小烦恼，甚至是你们之间那些不起眼的小约定。底层基于 RAG 记忆系统实现，本地化轻量存储。
* 🗂️ **连续记录 & 历史检索**：所有对话像微信 / iMessage 一样连成一条时间线并保存在本地；可按关键词或日期翻查，向上滚动自动加载更早的消息，并按时间分组显示。
* 🔍 **搜索增强**：睦头也会主动了解外面的世界。内置轻量级搜索系统，可以获取最新网络热点、Ave Mujica 相关资讯、天气等实时信息。相比依赖模型原生搜索 Agent，响应更快、成本更低，结果也更聚焦。
* 🖼️ **图片识别**：睦子米可以看得懂你的照片！她会笨拙但真心地用自己的方式来表达（你做的菜、你的猫、窗外的雨……），陪你度过每一次值得纪念的时刻。
* 💙 **Emoji 选择器**：内置可搜索的表情面板，支持中 / 英 / 日关键词。
* 🎙️ **语音输入**：支持语音输入转文本。

<!-- 建议这里放一张聊天窗口的截图 -->
<p><sub><i>聊天为云端 AI 能力，需要配置通义千问（DashScope）的 API Key 后使用；语音朗读（让睦开口说话）仍在开发中。</i></sub></p>

<h3 align="center">🎧 睦子米喜欢跟着节奏摇摆</h3>

<p align="center">
  <img src="docs/images/music1.avif" width="105" alt="music reaction 1" />
  <img src="docs/images/music2.avif" width="105" alt="music reaction 2" />
  <img src="docs/images/music3.avif" width="105" alt="music reaction 3" />
  <img src="docs/images/music4.avif" width="105" alt="music reaction 4" />
  <img src="docs/images/music5.avif" width="105" alt="music reaction 5" />
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



下个版本即将推出🚀：GPT-SoVITS小睦AI语音、休闲小音游、屏保飞行模式.....



### ⚙️ 偏好设置

右键点击系统托盘（右下角）的 Mutsumi 图标，可以自定义：`番茄钟时长` / `角色体型 (小/中/大)` / `天气开关` / `音乐控制器开关` / `界面语言`

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
│   ├── components/             UI 组件（PetWindow、SettingsWindow、ChatPanel 等）
│   ├── composables/            逻辑钩子（动画、音频感知、天气、番茄钟…）
│   ├── i18n/locales/           多语言支持（中文 / 英文 / 日文）
│   └── data/                   台词与对话数据
├── src-tauri/src/              Rust 后端
│   ├── chat/                   角色扮演聊天（RAG 记忆 · 提取 · 反思）
│   ├── db/                     SQLite（长期记忆 / 聊天记录）
│   ├── services/qwen.rs        通义千问 LLM（聊天 / 视觉 / 向量）
│   ├── audio.rs                全局系统音频监听（WASAPI）
│   ├── media.rs                媒体播放控制（SMTC）
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
### 🎁 致谢与参考

特别感谢以下伙伴及项目的贡献，让小睦能够顺利来到大家的桌面：

*   **社区贡献**：感谢 *-睦头人おれ.**, **可爱睦子米-_**, **CyanKirin99** 的支持与贡献，感谢 **xxx** 提供的 bug 反馈，以及全体用户的热情参与和反馈。
*   **性格模板 & 搜索功能**：参考自开源项目 [BANDORI-PET-REV](https://github.com/HELPMEEADICE/BANDORI-PET-REV)。
*   **Agent 状态系统**：设计灵感与架构参考论文 [《Generative Agents》](https://arxiv.org/abs/2304.03442)。
*   **人物背景剧情**：考据参考自 [萌娘百科 - 若叶睦](https://zh.moegirl.org.cn/%E8%8B%A5%E5%8F%B6%E7%9D%A6)。

### 🤝 参与贡献

欢迎任何形式的贡献！如果你有想法、发现了 Bug，或者想为动画、台词、多语言翻译出一份力：

1. Fork 本仓库
2. 新建分支：`git checkout -b feat/你的想法`
3. 提交改动并发起 Pull Request

如果只是想聊聊设计思路或报告问题，直接开一个 [Issue](../../issues) 就好。

如果喜欢这个项目，感谢你的star⭐——这对我们帮助很大！

---

<h2 id="mutsumi--desktop-companion-english">🌟 Mutsumi — Desktop Companion</h2>

<p align="center"><i>If even a cucumber needs time to grow, there's no need to rush. Good things take time 🌱</i></p>

Mutsumi is a quiet little companion living in the corner of your screen. She minds her own business, puts on her headphones when you play some tunes, and reacts when you interact with her. Powered by **Tauri 2 + Rust**, she is exceptionally lightweight (only a few dozen megabytes) and consumes virtually no system resources.

### ✨ Core Features

👻 **Always There, Never in the Way**
 Stays on top of all your windows, while fully transparent areas let your clicks pass straight through. Your code, browser, and apps remain completely accessible. Drag her anywhere on your screen at any time.

🎧 **Automatic Audio Awareness**
 Start playing music or a video, and she'll instantly put on her headphones and groove along to the beat. When the audio stops, she'll take them off and quietly return to idle.

🎵 **Mini Music Controller**
 A little speaker icon sits in the bottom-right corner and pulses along with your audio. Hover to expand a control panel — play/pause, previous/next, skip ±10s, replay, plus system volume and mute — alongside the current track, artist, and a progress bar you can click or drag to seek. When multiple apps are playing at once, switch between sources or let it auto-follow whichever one is most active. Built on Windows System Media Transport Controls (SMTC), so it drives anything that's playing: Spotify, NetEase Cloud Music, browser media, and more. Toggle it anytime from Settings.

<p align="center"><img src="docs/images/music-controller.avif" width="100%" alt="music controller" /></p>

<p><sub><i>Testing has shown that some applications (such as NetEase Cloud Music and QQ Music) do not expose low-level playback progress controls, so only Play/Pause, Previous/Next Track, system volume, and mute are supported. Audio sources such as PotPlayer, Spotify, Chrome, and Edge support the full set of media controls.</i></sub></p>

🍅 **Built-in Pomodoro Timer & Weather**
 Stay focused with a lightweight Pomodoro timer and keep an eye on the current weather, conveniently displayed in the corner of your screen.

🎮 **Fun Interactive Menu**
 Right-click her to open a menu full of interactions:

- ✋ **Pat Her Head** — Give her a gentle head pat.
- 🍨 **Feed Her a Matcha Parfait** — Her favorite treat!
- 😴 **Put Her to Sleep / Hide Her** — Let her take a break and disappear from view.
- 🔮 **Daily Tarot Reading** — Draw three tarot cards each day for a fresh dose of mystery, surprises, and inspiration.

💬 **Chat with Mutsumi**: Whether it's a sunny day or a rainy one, Mutsumi will always be quietly by your side.

* 🥒 **Project Cucumber**
  To recreate Mutsumi as faithfully as possible, we conducted extensive character analysis based on her story, official characterization, Jungian cognitive functions, and the Enneagram. Through prompt engineering and behavioral constraints, Mutsumi is designed to respond with the quiet sincerity and reserved personality that define her, rather than sounding like a generic AI assistant.

* 🧠 **Long-Term Memory**
  She remembers the things you've talked about: your cat, your work, small worries mentioned in passing, and even the little promises you make together. Powered by a RAG-based memory system with lightweight local storage, your conversations can continue naturally over time.

* 🗂️ **Persistent Chat History & Search**
  All conversations are stored locally and organized into a continuous timeline, similar to WeChat, iMessage, or other modern messaging apps. Browse past chats by keyword or date, automatically load older messages as you scroll, and view conversations grouped by time.

* 🔍 **Enhanced Search**
  Mutsumi can keep up with the world outside, too. A built-in lightweight search system allows her to access real-time information such as trending topics, Ave Mujica news, weather updates, and more. Compared with relying solely on an LLM's built-in search agent, this approach is faster, more cost-efficient, and more focused.

* 🖼️ **Image Understanding**
  Mutsumi can understand the photos you share. Whether it's a meal you cooked, your cat, or the rain outside your window, she'll respond in her own sincere and sometimes awkward way, helping you preserve the moments that matter.

* 💙 **Emoji Picker**
  Includes a searchable emoji panel with support for Chinese, English, and Japanese keywords.

* 🎙️ **Voice Input**
  Speak naturally and have your voice automatically converted into text for a more effortless chatting experience.


<!-- A screenshot of the chat window could go here -->
<p><sub><i>Chat is a cloud AI capability and requires your own Qwen (DashScope) API key; spoken replies (Mutsumi talking back) are still under construction.</i></sub></p>

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

Right-click her icon in the system tray (bottom-right corner) to tweak: `Pomodoro Durations` / `Character Size (S/M/L)` / `Weather Toggle` / `Music Controller Toggle` / `Language`

<p align="center"><img src="docs/images/setting.avif" width="440" alt="Settings window" /></p>

🚀 Coming soon: Spoken replies (Mutsumi's own voice), Casual Rhythm Game, Flying Screensaver Mode...

### 🛠️ Architecture

Tauri 2 (Rust backend) + Vue 3 frontend, bundled as a single native Windows app.

```
Mutsumi/
├── src/                        Vue frontend
│   ├── components/             UI components (PetWindow, SettingsWindow, ChatPanel, etc.)
│   ├── composables/            Logic hooks (animation, audio, weather, pomodoro…)
│   ├── i18n/locales/           Translations (Chinese / English / Japanese)
│   └── data/                   Dialogue and quote data
├── src-tauri/src/              Rust backend
│   ├── chat/                   Role-play chat (RAG memory · extraction · reflection)
│   ├── db/                     SQLite (long-term memory / chat history)
│   ├── services/qwen.rs        Qwen LLM (chat / vision / embeddings)
│   ├── audio.rs                Global system audio monitoring (WASAPI)
│   ├── media.rs                Media playback control (SMTC)
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
### 🎁 Acknowledgements & References

Special thanks to the following contributors and projects whose support made it possible to bring Mutsumi to everyone's desktop:

* **Community Contributions:** Many thanks to **-睦头人おれ.**, **可爱睦子米-_**, and **CyanKirin99** for their support and contributions; to **xxx** for valuable bug reports; and to all users for their enthusiasm, feedback, and continued support.
* **Character Prompt & Search Functionality:** Inspired by and adapted from the open-source project BANDORI-PET-REV.
* **Agent State System:** The design and architecture were inspired by the paper *Generative Agents: Interactive Simulacra of Human Behavior*.
* **Character Background & Lore Research:** Reference materials were gathered from the Moegirl Wiki article on Wakaba Mutsumi.

### 🤝 Contributing

Contributions of any kind are welcome! Whether you have an idea for a new feature, found a bug, or would like to help with animations, dialogue, or translations, we'd love to hear from you.

1. Fork the repo
2. Create a branch: `git checkout -b feat/your-idea`
3. Commit your changes and open a Pull Request

Just want to share a thought or report something? Feel free to open an [Issue](../../issues) 

---

If you like this project, consider giving it a ⭐ — it helps a lot!
