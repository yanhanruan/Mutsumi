🚀 Mutsumi v1.5.0

本次更新带来三大新功能：气球飞行屏保、三语联网搜索、应用内自动更新。
This release brings three big features: a balloon flying screensaver, trilingual web search, and in-app auto-update.

**🎈 飞行屏保模式 (Flying Screensaver / Balloon Mode)**

- 电脑闲置一段时间后（等待时长可在设置中调整，6–30 分钟），睦头会带上黄瓜小气球轻轻飞起，缓缓地在整个屏幕上飘荡，顺便保护你的屏幕。也可以随时按 Ctrl+Alt+F 手动让她起飞或降落。
  After your PC has been idle for a while (wait time adjustable in Settings, 6–30 minutes), Mutsumi lifts off with a little cucumber balloon and drifts gently across your whole screen — a screensaver that also protects your display. You can also send her flying, or land her, anytime with Ctrl+Alt+F.
  
- 飞行时移动的是整个窗口而不是画面里的小人，所以她能贴着屏幕缓缓平移、遇到边缘轻轻反弹、并随飞行方向自动转向；起飞与降落都有专门的过渡动画。
  In flight it's the whole window that glides — not a sprite trapped inside a tiny box — so she drifts smoothly from edge to edge, bounces softly off the screen borders, and turns to face the way she's heading. Take-off and landing have their own morph animations, and she lands exactly where she stops.
  
- 可在设置中开关飞行屏保，并调整触发前的闲置等待时长。
  Toggle the flying screensaver on or off, and tune the idle wait time, in Settings.

**🌏 联网搜索：现在也说英文和日文 (Web Search now speaks English and Japanese)**

新增 / New

- 🌏 搜索支持中 / 英 / 日三语：以前只有用中文提问才会触发联网搜索、也只有中文结果能被正确挑选。现在用 English 或 日本語 问「東京の天気」「USD to JPY exchange rate」「ゼルダ新作の発売日」，睦头一样听得懂、查得到。是否该联网、怎么筛结果、怎么认出那个事实（温度 / 汇率 / 日期）——三个环节都做了三语适配。
  🌏 Search works in English, Chinese, and Japanese. Previously only a Chinese question would trigger a live search or get its results filtered correctly. Now "東京の天気", "USD to JPY exchange rate", or "GTA 6 release date" work just as well. All three stages — deciding whether to search, filtering results, and recognizing the actual fact (temperature / rate / date) — now handle every language.
- 🔎「再深入一层」：当搜索摘要只提到话题却没写清楚那个数字时，睦头会点进排名第一的页面，直接从正文里取出温度 / 汇率 / 日期。实测里，日语和英文的天气提问都能拿到当天实际气温（如 Tokyo 35℃、東京 26℃）。
  🔎 "Go deeper." When a search summary names the topic but doesn't state the number, Mutsumi opens the top result and pulls the real temperature, rate, or date straight from the page. In testing, both Japanese and English weather questions returned the day's actual temperature (Tokyo 35℃, 東京 26℃).

改进与修复 / Improved & Fixed

- 收紧了结果筛选：真正带着答案的摘要不会再被误当成营销废话删掉；只和你共享「今天」「如何」这类功能词的无关结果会被正确剔除。
  Tighter result filtering: a snippet that actually carries the answer is no longer mistaken for marketing filler and dropped, and junk that only shares filler words like "today" / "今天" with your question is now correctly excluded.
- 补齐了繁体与日文新字体的归一（気→气、発→发、価→价……），日语提问与繁体页面都能正确匹配。
  Added Japanese-shinjitai and traditional-Chinese folding (気→气, 発→发, 価→价 …), so Japanese questions and traditional-script pages match properly.

覆盖 Bing 国内 / Bing / Google / 百度 / DuckDuckGo 五个搜索引擎实测通过。
Verified across all five engines: Bing CN, Bing, Google, Baidu, and DuckDuckGo.

**🔄 应用内自动更新 (In-App Auto-Update)**

- 新增应用内自动更新：自动检查，一键升级到新版本。
  In-app auto-update: auto-checks, upgrade with one click.
- 更新提醒可推迟 1–30 天；也可随时在"关于"页面手动检查。
  Snooze reminders for 1-30 days, or check manually anytime from About.
- 所有更新均经过数字签名验证，确保安装包真实可信。
  Every update is cryptographically signed and verified before install.

说明：本次是自动更新功能上线的第一个版本。如果你现在用的是手动下载的 1.4.0 或更早版本，需要手动下载一次这次的 1.5.0；从 1.5.0 起，之后的版本都会自动提醒并更新，不用再手动下载。
Note: this is the first release with auto-update built in. If you're on a manually-downloaded 1.4.0 or earlier, please download 1.5.0 once by hand — from 1.5.0 onward, future versions will notify and update themselves.
