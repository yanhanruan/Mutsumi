<p align="center">
  <img src="app-icon-cucumber-puppy-transparent-compress.png" alt="Mutsumi Logo" width="120" />
  <!-- 建议这里放一张角色的透明底头像 -->
</p>

<h1 align="center">Mutsumi (睦) — Desktop Companion</h1>

<p align="center">
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2.0-24C8D8?logo=tauri&logoColor=white" alt="Tauri 2"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-Backend-000000?logo=rust&logoColor=white" alt="Rust"></a>
  <a href="https://vuejs.org/"><img src="https://img.shields.io/badge/Vue_3-Frontend-4FC08D?logo=vuedotjs&logoColor=white" alt="Vue 3"></a>
  <a href="../../releases"><img src="https://img.shields.io/badge/Windows-released-0078D6?logo=windows&logoColor=white" alt="Windows released"></a>
  <a href="docs/MACOS-ADAPTATION-PLAN.md"><img src="https://img.shields.io/badge/macOS-in%20development-000000?logo=apple&logoColor=white" alt="macOS in development"></a>
</p>

> **Platform status / 平台状态：** Windows 版已发布。macOS 13+ 适配正在开发与真机验收中，
> 尚未提供签名、notarized 的公开 DMG。The Windows build is released; macOS 13+ is under
> active development and does not yet have a signed, notarized public DMG.

<p align="center">
  <a href="#mutsumi--桌面陪伴宠物-中文"><b>中文</b></a> | <a href="#mutsumi--desktop-companion-english"><b>English</b></a>
</p>
<table align="center">
  <tr>
    <td align="center"><img src="docs/images/idle.avif"    width="200" alt="Mutsumi idle" /><br/><sub>🌿 静静陪伴 · just hanging out</sub></td>
    <td align="center"><img src="docs/images/headpat.avif" width="200" alt="Head pat" /><br/><sub>✋ 摸摸头 · head pat</sub></td>
    <td align="center"><img src="docs/images/tarot.avif"   width="200" alt="Tarot reading" /><br/><sub>🔮 塔罗占卜 · tarot reading</sub></td>
    <td align="center"><img src="docs/images/sleeping.avif"   width="200" alt="Nap time" /><br/><sub>💤 小惬中 · Nap time</sub></td>
  </tr>
</table>


---


<h2 id="mutsumi--桌面陪伴宠物-中文">🌟 Mutsumi — 桌面陪伴宠物</h2>

<p align="center"><i>就算是黄瓜,也需要时间才能长大，好事不能着急 🌱</i></p>

Mutsumi 是一个静静住在你屏幕角落的小伙伴。
她不会打扰你工作，只是自顾自地待在那里。当你播放音乐时，她会默默戴上耳机感受节奏；当你感到疲惫时，可以戳戳她、喂杯茶。得益于 **Tauri 2 + Rust** 的底层驱动，她非常轻量，几乎不占用系统资源。

### ✨ 核心体验

👻 **无感陪伴，穿透点击**：始终悬浮在屏幕最前方，但透明区域完全穿透，绝不遮挡你点击底部的代码或网页。你可以随时把她拖拽到屏幕的任意角落。

🎈 **飞行屏保模式**：当电脑闲置一段时间后（等待时长可在设置中调整，6–30 分钟），睦头会带上黄瓜小气球轻轻飞起，缓缓地在整个屏幕上飘荡（顺便保护屏幕）。支持在设置中开关。

<p align="center"><img src="docs/images/flying.avif" width="100%" alt="flying mode" /></p>

🎧 **全局音频感知（无需配置）**：当你开始播放音乐或看视频时，她会戴上耳机；声音停止后恢复平静。Windows 使用 WASAPI 振幅，macOS 首版使用公开 CoreAudio 输出设备 I/O 状态估算活动，因此静音但未关闭的音频流可能仍被视为活动。

🎵 **迷你音乐控制器（当前仅 Windows）**：右下角的音响小图标可展开播放 / 暂停、上一首 / 下一首、进度、系统音量与静音等控制。该功能依赖 Windows SMTC；macOS 没有对等的公开跨应用接口，因此 macOS 版会保留音频活动徽章，但隐藏不可用的曲目信息和控制面板。

<p align="center"><img src="docs/images/music-controller.avif" width="100%" alt="music controller" /></p>

<p><sub><i>经测试部分软件（网易云、QQ音乐等）没有应用底层进度条控制接口，因此仅支持播放 / 暂停、上一首 / 下一首、系统音量与静音，potplayer、spotify、chrome、edge等音频源均支持完整控制功能</i></sub></p>

🍅 **极客番茄钟 & 天气**：内置轻量级专注/休息计时器，并在角落安静地展示实时天气。

🖥️ **系统状态一览**：右键菜单中的「系统状态」面板，实时显示 CPU / 内存 / 网络 / 开机时长 / 电量，并附带一页硬件规格速览（CPU、内存、显卡显存、磁盘分区）。

🎮 **丰富的互动菜单**：右键点击她，即可呼出互动面板：
* ✋ **摸摸头**：召唤哈基米摸摸头
* 🍨 **喂抹茶芭菲**：抹茶芭菲最喜欢了！
*  **让她睡觉 / 隐藏**：让她躲起来
* 🔮 **每日塔罗占卜**：每天抽取3张专属塔罗牌，带来每天不一样的未知惊喜与指引。

💬 **和睦聊天**：无论是太阳天还是雨天，睦头都会安静地陪在你身边：
* 🥒 **小黄瓜**：为了尽可能还原睦头，我们结合剧情内容、角色设定、荣格八维与九型人格分析，对角色进行了长期拆解与建模。通过 Prompt Engineering 与行为约束设计，实现睦头安静而真诚的性格，而不是千篇一律的 AI 回复。
* 💭 **长期记忆**：她会记得你们聊过的事情：家里的猫、最近的工作、偶尔提起的小烦恼，甚至是你们之间那些不起眼的小约定。底层基于 RAG 记忆系统实现，本地化轻量存储。
* 🗂️ **连续记录 & 历史检索**：所有对话像微信 / iMessage 一样连成一条时间线并保存在本地；可按关键词或日期翻查，向上滚动自动加载更早的消息，并按时间分组显示。

> 🐍如果用毒蛇的毒毒毒蛇，毒蛇会不会被毒蛇蛇毒的毒毒死？😆众所周知，作为最猛没有之一的大爬虫，各大浏览器引擎厂商采用了及其严苛的底层加密（浏览器高级指纹识别、自动化框架特征检测、引擎混淆与反调试、TLS/JA3 指纹、HTTP/2 帧指纹等等）以实现 anti-scraping & bot-detection。原始的搜索增强方案采用 Rust reqwest 与引擎 fallback，但无法稳定跨过这些保护。现在改用操作系统自带的真实 WebView（Windows 为 WebView2 / Chromium，macOS 为 WKWebView / WebKit），无需携带额外的浏览器运行时。这样每个客户端仍然独立、安全，不需要 IP 连接池或指纹伪造。

* 🔍 **联网搜索**：睦头也会好奇外面的世界。想知道最近的热点、Ave Mujica 的新消息、或者东京今天的天气？跟她说一声就好——她会读网页，再把真正有用的那几条挑出来告诉你。核心技术：

  - **反反爬原理：** 大多数桌面应用用 HTTP 客户端假装浏览器，却无法解决 Cloudflare、JS、SPA 等难点。睦头直接把隐藏 WebView（Windows WebView2 / Chromium 或 macOS WKWebView / WebKit）导航到搜索页——请求带着真实浏览器的网络指纹、warm cookie jar 和 JS 引擎。因为文档来源就是搜索引擎本身，连 CORS 边界都不存在。一个长期复用的隐藏单例窗口保持 cookie；遇到人机验证时，验证窗口会显示出来让用户手动完成，随后继续原请求。
  - **渲染后的 HTML 传递问题：** Tauri v2 的安全模型下，一个远程页面不允许调用应用命令。解法：给这个 `serp-fetcher` 窗口单独授一份能力（`capabilities/webview-serp.json`），注入的 init-script 通过核心 event 插件（`plugin:event|emit`）把渲染后的 `outerHTML` 发回 Rust。每次导航在 URL 的 **query**（`&__serpid=…`——故意用 query 而非 fragment，这样即便被 `continue=` 重定向包裹也能存活）里带一个请求 id，脚本在 document-start 读到并回传以对上号；当某引擎在重定向里丢了这个参数时，再用"当前唯一在途导航"兜底路由——因为所有导航都串行，在途永远只有一个。
  - **多引擎 · 双解析 · 链接还原：** Bing（国内与国际）、Google、百度、DuckDuckGo：主解析用正则盯结构性 landmark（如 `<li class="b_algo">`，比 CSS 类名更抗改版），返回空再退回 CSS 选择器兜底；每家的跳转包装各自还原成真实地址（Bing `/ck/a?u=a1<base64url>`、Google `/url?q=`、DDG `l/?uddg=`、百度 302 直跳）。解析全是纯同步函数，选择器失配只返回空、安全fallback。五类引擎均有实测。
  - **两段式架构：** SERP 摘要其实是答案的**劣质载体**：引擎常把数字截断、或换成网站的 SEO 标语（"XX天气网为您提供…"）。所以先做质量筛选——要求命中查询主题、且摘要里真的带一个具体事实（温度 / 汇率 / 日期）；话题相关的链接会保留，但它的废话摘要会被丢掉、而不是当答案喂给模型；并做了繁体与日文新字体归一（気→气、発→发、英偉達→英伟达）。当排名第一的摘要**只提到话题却没写那个数字**时，触发"再深入一层"：把同一个隐藏窗口导航进正文页，剥掉非内容子树、按文档顺序取出真正陈述该事实的段落（有硬上限）。深入只会增加信号——抽取不到就退回原摘要，安全revert。
  - **可复现测试：** 任何环节失败都安静降级为"没有联网信息"，搜索绝不打断聊天；触发、筛选、抽取等纯逻辑都有单元测试。而依赖真实 WebView 的活路径（反爬通过率、结果质量）没法进 `cargo test`，于是做了**应用内、环境变量开关**的基准：`MUTSUMI_SERP_BENCH` 跑机器人识别通过率、`MUTSUMI_SERP_QUALITY` 跑全流程质量报告，逐引擎 × 主题输出可复查的表格。
* 🖼️ **图片识别**：睦子米可以看得懂你的照片！她会笨拙但真心地用自己的方式来表达（你做的菜、你的猫、窗外的雨……），陪你度过每一次值得纪念的时刻。
* 💙 **Emoji 选择器**：内置可搜索的表情面板，支持中 / 英 / 日关键词。
* 🎙️ **语音输入**：支持语音输入转文本。

<p align="center">
    <img src="docs/images/chat.avif" height="450" alt="Settings window" />
    <img src="docs/images/chat-setting.avif" height="450" alt="Settings window" />
</p>

<p><sub><i>聊天为云端 AI 能力，需要配置通义千问（DashScope）的 API Key 后使用；GPT-SoVITS小睦AI语音仍在开发中。</i></sub></p>

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



下个版本即将推出🚀：屏保飞行模式（于1.5.0版本推出✔）、GPT-SoVITS小睦AI语音、休闲小音游.....



### ⚙️ 偏好设置

右键点击系统托盘（右下角）的 Mutsumi 图标，可以自定义：`番茄钟时长` / `角色体型 (小/中/大)` / `天气开关` / `音乐控制器开关` / `飞行屏保开关与等待时长` / `界面语言`

<p align="center">
    <img src="docs/images/setting.avif" width="440" alt="Settings window" />
</p>

---

### 🚀 快速获取

**Windows 直接安装（推荐）：**
前往 [Releases 页面](../../releases) 获取最新的 Windows 安装包。macOS 签名 / notarized DMG 尚未公开发布，目前仅建议开发和测试人员从源码运行。

**面向开发者的源码构建：**
如果你想研究 Tauri 的异形窗口或系统音频捕获逻辑，欢迎克隆代码！
环境要求：`Node.js (18+)` + `Rust stable`；macOS 还需安装 Xcode Command Line Tools。

```bash
git clone <你的仓库地址>
cd Mutsumi
npm install
npm run tauri dev
```

*(首次运行会编译 Rust 核心依赖，大约需要喝杯茶的时间 🍵，后续启动秒开)*

### 🛠️ 技术架构

Tauri 2（Rust 后端）+ Vue 3 前端，Windows 为当前发布平台，macOS 13+ 适配正在进行中。

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

*   **社区贡献**：感谢 **-睦头人おれ.**, **可爱睦子米-_**, **CyanKirin99** 的支持与贡献，感谢 **♿网友睦子米♿🍥**、**若叶睦** 提供的 bug 反馈，以及各位伙伴的热情参与和反馈。
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

Mutsumi is a quiet little companion living in the corner of your screen. She minds her own business, puts on her headphones when you play some tunes, and reacts when you interact with her. Powered by **Tauri 2 + Rust**, she is exceptionally lightweight and consumes virtually no system resources.

### ✨ Core Features

👻 **Always There, Never in the Way**
 Stays on top of all your windows, while fully transparent areas let your clicks pass straight through. Your code, browser, and apps remain completely accessible. Drag her anywhere on your screen at any time.

🎈 **Flying Screensaver**
After your computer has been idle for a configurable period (6–30 minutes), Muto will take off with a tiny cucumber balloon and gently float around your screen. It’s a fun little screensaver that also helps protect your display. You can enable or disable it anytime in Settings.

<p align="center"><img src="docs/images/flying.avif" width="100%" alt="flying mode" /></p>

🎧 **Automatic Audio Awareness**
 Start playing music or a video and she'll put on her headphones; when audio stops, she returns to idle. Windows uses WASAPI amplitude. The first macOS build estimates activity from public CoreAudio output-device I/O, so a silent stream that remains open may still appear active.

🎵 **Mini Music Controller (currently Windows only)**
 The speaker badge expands into playback, track, progress, system-volume, and mute controls backed by Windows SMTC. macOS has no equivalent public cross-application API, so the macOS build keeps the audio-activity badge while hiding unavailable metadata and transport controls.

<p align="center"><img src="docs/images/music-controller.avif" width="100%" alt="music controller" /></p>

<p><sub><i>Testing has shown that some applications (such as NetEase Cloud Music and QQ Music) do not expose low-level playback progress controls, so only Play/Pause, Previous/Next Track, system volume, and mute are supported. Audio sources such as PotPlayer, Spotify, Chrome, and Edge support the full set of media controls.</i></sub></p>

🍅 **Built-in Pomodoro Timer & Weather**
 Stay focused with a lightweight Pomodoro timer and keep an eye on the current weather, conveniently displayed in the corner of your screen.

🖥️ **System State at a Glance**
 The "System Status" panel (right-click menu) shows live CPU / memory / network / uptime / battery, plus a one-page hardware spec sheet (CPU, RAM, GPU VRAM, disk partitions).

🎮 **Fun Interactive Menu**
 Right-click her to open a menu full of interactions:

- ✋ **Pat Her Head** — Give her a gentle head pat.
- 🍨 **Feed Her a Matcha Parfait** — Her favorite treat!
- 😴 **Put Her to Sleep / Hide Her** — Let her take a break and disappear from view.
- 🔮 **Daily Tarot Reading** — Draw three tarot cards each day for a fresh dose of mystery, surprises, and inspiration.

💬 **Chat with Mutsumi**: Whether it's a sunny day or a rainy one, Mutsumi will always be quietly by your side.

* 🥒 **Project Cucumber**
  To recreate Mutsumi as faithfully as possible, we conducted extensive character analysis based on her story, official characterization, Jungian cognitive functions, and the Enneagram. Through prompt engineering and behavioral constraints, Mutsumi is designed to respond with the quiet sincerity and reserved personality that define her, rather than sounding like a generic AI assistant.
* 💭 **Long-Term Memory**
  She remembers the things you've talked about: your cat, your work, small worries mentioned in passing, and even the little promises you make together. Powered by a RAG-based memory system with lightweight local storage, your conversations can continue naturally over time.
* 🗂️ **Persistent Chat History & Search**
  All conversations are stored locally and organized into a continuous timeline, similar to WeChat, iMessage, or other modern messaging apps. Browse past chats by keyword or date, automatically load older messages as you scroll, and view conversations grouped by time.

> 🐍 What happens when you use a snake’s own venom against it? Uno reverse card? 😆 Browser engines have deep anti-automation defenses that a static HTTP client cannot reliably reproduce. Mutsumi therefore uses the operating system's real WebView—WebView2 / Chromium on Windows and WKWebView / WebKit on macOS—without bundling another browser runtime. Each client remains independent and does not need proxy pools or fingerprint spoofing.

* 🔍 **Web Search** Mutsumi gets curious about the outside world too. Want to know the latest trending topics, new updates about Ave Mujica, or today’s weather in Tokyo? Just ask her—she will read the web pages and pick out the few pieces of information that are genuinely useful. Core technologies:
  * **Anti-anti-scraping principle:** Most desktop applications use HTTP clients to impersonate browsers, but they cannot properly handle challenges such as Cloudflare, JavaScript, and SPAs. Mutsumi instead navigates a hidden WebView—Windows WebView2 / Chromium or macOS WKWebView / WebKit—directly to the search page. Requests therefore carry a real browser engine, warm cookie jar, and JavaScript runtime. A long-lived singleton keeps cookies warm; if a human-verification challenge appears, the window surfaces for the user to complete it before the original request continues.
  * **Passing rendered HTML back to the application:** Under Tauri v2’s security model, a remote page is not allowed to invoke application commands. The solution is to grant the `serp-fetcher` window its own dedicated capability (`capabilities/webview-serp.json`). An injected init script then sends the rendered `outerHTML` back to Rust through the core event plugin (`plugin:event|emit`). Each navigation carries a request ID in the URL’s **query string** (`&__serpid=…`—deliberately placed in the query rather than the fragment so that it survives even when wrapped inside a `continue=` redirect). The script reads the ID at document start and sends it back so the response can be matched to the correct request. When an engine drops this parameter during a redirect, routing falls back to the “only currently in-flight navigation.” Because all navigations are serialized, there can only ever be one in flight.
  * **Multiple engines · dual parsing · link restoration:** Bing (domestic and international), Google, Baidu, and DuckDuckGo are supported. The primary parser uses regular expressions to target structural landmarks such as `<li class="b_algo">`, which are more resistant to redesigns than ordinary CSS class names. If that returns nothing, it falls back to CSS selectors. Each engine’s redirect wrapper is independently resolved back to the real destination URL: Bing’s `/ck/a?u=a1<base64url>`, Google’s `/url?q=`, DDG’s `l/?uddg=`, and Baidu’s direct 302 redirects. All parsing is implemented as pure synchronous functions. A selector mismatch simply returns an empty result and triggers a safe fallback. All five engine variants have been tested in practice.
  * **Two-stage architecture:** SERP snippets are actually a **poor carrier for answers**. Search engines often truncate numbers or replace useful content with a website’s SEO tagline, such as “XX Weather provides you with…”. Results therefore undergo quality filtering first: a snippet must match the query topic and actually contain a concrete fact, such as a temperature, exchange rate, or date. Topically relevant links are retained, but their useless snippets are discarded rather than fed to the model as answers. Traditional Chinese and Japanese shinjitai variants are also normalized (`気→气`, `発→发`, `英偉達→英伟达`). When the top-ranked snippet **mentions the topic but omits the actual number**, the system triggers a “go one level deeper” step: it navigates the same hidden window to the source page, strips out non-content subtrees, and extracts—in document order—the paragraphs that genuinely state the relevant fact, subject to a hard limit. Going deeper can only add signal: if extraction fails, the system safely reverts to the original snippet.
  * **Reproducible testing:** A failure at any stage quietly degrades to “no online information available,” so search never interrupts the conversation. Pure logic for triggering, filtering, extraction, and related steps is covered by unit tests. The live path that depends on a real WebView—such as anti-bot pass rates and result quality—cannot be included in `cargo test`, so the application includes **in-app benchmarks controlled by environment variables**. `MUTSUMI_SERP_BENCH` measures bot-detection pass rates, while `MUTSUMI_SERP_QUALITY` runs a full-pipeline quality report. Both produce reviewable tables broken down by engine × topic.
* 🖼️ **Image Understanding**
  Mutsumi can understand the photos you share. Whether it's a meal you cooked, your cat, or the rain outside your window, she'll respond in her own sincere and sometimes awkward way, helping you preserve the moments that matter.
* 💙 **Emoji Picker**
  Includes a searchable emoji panel with support for Chinese, English, and Japanese keywords.
* 🎙️ **Voice Input**
  Speak naturally and have your voice automatically converted into text for a more effortless chatting experience.

<p align="center">
    <img src="docs/images/chat.avif" height="450" alt="Settings window" />
    <img src="docs/images/chat-setting.avif" height="450" alt="Settings window" />
</p>

<p><sub><i>Chat is a cloud AI capability and requires your own Qwen (DashScope) API key; Mutsumi AI tts is still under construction.</i></sub></p>

<h3 align="center">🎧 Mutsumi Loves Bopping Along to Your Music</h3>

<p align="center">
  <img src="docs/images/music1.avif" width="105" alt="music reaction 1" />
  <img src="docs/images/music2.avif" width="105" alt="music reaction 2" />
  <img src="docs/images/music3.avif" width="105" alt="music reaction 3" />
  <img src="docs/images/music4.avif" width="105" alt="music reaction 4" />
  <img src="docs/images/music5.avif" width="105" alt="music reaction 5" />
</p>

### 🚀 Getting Started

**Option 1 — Windows download (Easiest):**
Grab the latest Windows installer from the [Releases page](../../releases). A signed, notarized macOS DMG has not been published yet; source builds are currently intended for development and testing only.

**Option 2 — Build from Source (For Geeks):**
Want to see how we handle transparent windows or global audio capture in Tauri? Clone away!
Requirements: `Node.js (18+)` + `Rust stable`; macOS also requires Xcode Command Line Tools.

```bash
git clone <repo-url>
cd Mutsumi
npm install
npm run tauri dev
```

*(The first Rust compilation might take a few minutes — perfect time to grab a cup of tea 🍵. Subsequent builds are blazing fast)*

### ⚙️ Settings

Right-click her Windows tray icon or macOS menu-bar icon to tweak: `Pomodoro Durations` / `Character Size (S/M/L)` / `Weather Toggle` / `Audio Badge or Music Controller` / `Flying Screensaver Toggle & Wait Time` / `Language`

<p align="center"><img src="docs/images/setting.avif" width="440" alt="Settings window" /></p>

🚀 Coming soon: Flying Screensaver Mode( Released in version 1.5.0 ✔), Spoken replies (Mutsumi's own voice), Casual Rhythm Game...

### 🛠️ Architecture

Tauri 2 (Rust backend) + Vue 3 frontend. Windows is the current release platform; macOS 13+ adaptation is in progress.

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
