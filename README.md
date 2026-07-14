# 🦀 Usage Widget for Claude

A tiny, cute Windows 10/11 desktop widget that shows your **Claude plan usage limits** — the same numbers you see in Claude's *Plan usage limits* popup or Claude Code's `/usage` command. Featuring **Kani**, a little orange crab who gets progressively more nervous as you burn through your quota.

> **Unofficial.** This project is not affiliated with or endorsed by Anthropic. It talks to an undocumented endpoint that may change or stop working at any time. The mascot and icon are original artwork.

![theme](https://img.shields.io/badge/theme-orange%20%C3%97%20dark-d97757) ![platform](https://img.shields.io/badge/platform-Windows%2010%20%2F%2011-blue) ![stack](https://img.shields.io/badge/built%20with-Tauri%20v2-orange)

## ⬇️ Download

**[Download the installer (v0.2.8, ~1.4 MB)](installer/Usage%20Widget%20for%20Claude_0.2.8_x64-setup.exe)** — Windows 10 / 11 x64, no admin rights needed. Requires the [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/), which almost every Windows 10/11 machine already has (it ships with Windows 11 and with Edge on Windows 10) — if yours somehow doesn't, grab it from that link first.

Windows SmartScreen will warn you on first run because the installer isn't code-signed — click **More info → Run anyway**, or [build it yourself from source](#building-from-source).

## What it shows

- **Current session** % used + reset countdown
- **Weekly · All models** % used + reset countdown
- **Weekly · per-model** buckets (e.g. Fable/Opus) as reported by the API
- Your plan (Pro/Max), last-updated time, and a mascot mood ring:
  😊 under 50% · 😟 50–80% · 😱 over 80% · 😴 error/expired token

## Getting started

### If you use Claude Code (zero config)

1. Install the widget (or `npm run tauri build` your own — see below).
2. Launch it. That's it. The widget finds your existing Claude Code login automatically.

### If you don't use Claude Code (one step)

Being logged into **claude.ai in a browser is not enough** — the usage endpoint this widget calls is Claude Code's own login, a separate system. To get a token:

1. Install [Claude Code](https://claude.com/claude-code) (CLI or desktop) and log in once — even if you never use it for coding, this populates the login file the widget reads. Then just launch the widget; it picks it up automatically.
2. Or, launch the widget and paste a Claude OAuth access token (`sk-ant-oat…`) on the one-page setup screen. Find it at `%USERPROFILE%\.claude\.credentials.json` → `claudeAiOauth.accessToken` on any machine where you've logged into Claude Code. It's stored in **Windows Credential Manager**, never in a plain file.

## Transparency: what this app reads and where data goes

- **Reads** `%USERPROFILE%\.claude\.credentials.json` (Claude Code's own login file) to obtain your OAuth access token.
- **Sends** exactly two kinds of requests, both to Anthropic and nothing else:
  1. `GET https://api.anthropic.com/api/oauth/usage` — fetches your usage percentages.
  2. `POST https://console.anthropic.com/v1/oauth/token` — **only when the token has expired**, renews it using the same OAuth refresh flow (and the same public client id) Claude Code itself uses.
  **No other network calls. No telemetry. No analytics. No auto-update.**
- **Writes** to that credentials file in exactly one case: storing the renewed token pair after a refresh, in the same format Claude Code writes, so Claude Code and the widget stay signed in together. Nothing else in the file is touched (atomic replace, all other fields preserved).
- The token never leaves the Rust backend process — the UI layer only ever receives percentages and timestamps.
- Config/cache lives at `%LOCALAPPDATA%\com.varintha.usagewidget\` (window position + the embedded WebView2 browser's own cache) — never secrets.
- Auto-refresh only works while the stored *refresh token* is valid (about a month, rolling). If the widget ever says it can't renew the login, open Claude Code once and sign in.
- Polls every **5 minutes by default** (configurable 30s–10min in Settings), and only while the window is actually visible — hidden in the tray, it stops polling entirely rather than fetching data nobody's looking at. Launching the widget while a copy is already running just brings the existing window forward instead of starting a second poller, so you never end up with two copies quietly hammering the same rate limit.
- **Uninstalling** removes that cache folder and any manually-pasted token from Windows Credential Manager automatically. On some systems, two small leftover program files (`usage-widget-for-claude.exe`, `uninstall.exe`, no user data) can remain in the install folder for a few seconds afterward if antivirus is still scanning them at the exact moment of removal — a known quirk of self-deleting Windows installers in general. Safe to delete manually if you ever see it; contains nothing but the app binary itself.

## Building from source

Prerequisites: [Rust (MSVC)](https://rustup.rs/), [Node.js](https://nodejs.org/), and the Visual Studio C++ Build Tools.

```powershell
npm install
npm run dev      # run in dev mode
npm run build    # produce the NSIS installer in src-tauri/target/release/bundle/nsis/
```

## A note on SmartScreen

Release builds are **not code-signed** (certificates cost money and require identity verification). Windows SmartScreen will warn you the first time you run the installer — this can't be fixed in code; it's fundamentally about earned trust:

- SmartScreen's "unrecognized app" warning is about the *publisher having no reputation*, not about detected malware. There's no submission process to clear it, unlike a malware false-positive.
- The only real fixes are (a) a paid code-signing certificate — an EV certificate gets instant trust but costs more and requires notarized identity verification; a standard OV certificate is cheaper but still warns initially, building reputation over weeks of downloads — or (b) living with it, which is normal for small unsigned open-source tools.
- Your options: click **More info → Run anyway** (safe, since the source here is public and auditable), or [build it yourself from source](#building-from-source) — the entire codebase is small enough to read over coffee.

## Tray menu

Left-click the tray crab to show/hide the widget. Right-click for: Refresh now · Always on top · Start with Windows · Quit. Closing the window just hides it to the tray.

## License

[MIT](LICENSE). Mascot "Kani" 🦀 is original artwork, released under the same license.

---

# 🦀 วิธีใช้ (ภาษาไทย)

วิดเจ็ตเล็กๆ บน Windows 10/11 แสดงเปอร์เซ็นต์การใช้งาน Claude ตามแพลนของคุณ (ตัวเลขเดียวกับหน้า *Plan usage limits* หรือคำสั่ง `/usage` ใน Claude Code)

**ถ้าใช้ Claude Code อยู่แล้ว:** ติดตั้งแล้วเปิดได้เลย ไม่ต้องตั้งค่าอะไร — วิดเจ็ตหา login ของ Claude Code ในเครื่องให้อัตโนมัติ

**ถ้าไม่ได้ใช้ Claude Code:** การ login เข้า claude.ai ผ่านเบราว์เซอร์**ไม่นับ** เพราะ endpoint ที่วิดเจ็ตนี้เรียกใช้ผูกกับ login ของ Claude Code โดยเฉพาะ (คนละระบบกัน) มีสองทางเลือก:
1. ติดตั้ง [Claude Code](https://claude.com/claude-code) (CLI หรือ desktop) แล้ว login สักครั้ง — ต่อให้ไม่ได้ใช้เขียนโค้ดเลยก็ได้ แค่ login ครั้งเดียววิดเจ็ตก็หาเจอเองอัตโนมัติ
2. หรือเปิดวิดเจ็ตแล้ววาง OAuth access token (`sk-ant-oat…`) บนหน้าตั้งค่าหน้าแรกโดยตรง หา token ได้จาก `%USERPROFILE%\.claude\.credentials.json` → `claudeAiOauth.accessToken` บนเครื่องไหนก็ได้ที่เคย login Claude Code ไว้ โดย token ถูกเก็บใน Windows Credential Manager อย่างปลอดภัย ไม่เคยเก็บเป็นไฟล์ธรรมดา

**ความปลอดภัย:** แอปคุยกับ Anthropic เท่านั้น (ดึง usage จาก `api.anthropic.com` และต่ออายุ token ผ่าน `console.anthropic.com` เมื่อหมดอายุ ด้วยวิธีเดียวกับที่ Claude Code ทำเอง) ไม่มี telemetry ไม่ส่งข้อมูลไปที่อื่นใดทั้งสิ้น และ token ไม่เคยหลุดออกจากตัวโปรแกรมฝั่ง Rust — การเขียนไฟล์ login ของ Claude Code เกิดขึ้นกรณีเดียวคือบันทึก token ใหม่หลังต่ออายุ (รูปแบบเดียวกับที่ Claude Code เขียน ทำให้ทั้งสองแอป login ค้างไว้ด้วยกัน) การต่ออายุอัตโนมัติจะใช้ได้ตราบใดที่ refresh token ยังไม่หมดอายุ (ประมาณ 1 เดือนแบบต่ออายุตัวเองเรื่อยๆ) ถ้าวิดเจ็ตแจ้งว่าต่ออายุไม่ได้ ให้เปิด Claude Code แล้ว login ใหม่หนึ่งครั้ง

**การดึงข้อมูล:** ดึงทุก 5 นาทีเป็นค่าเริ่มต้น (ปรับได้ 30 วิ–10 นาทีใน Settings) และดึงเฉพาะตอนหน้าต่างเปิดอยู่เท่านั้น — ถ้าซ่อนอยู่ใน tray จะหยุดดึงไปเลย ไม่เปลืองโควตาเปล่าๆ กับข้อมูลที่ไม่มีใครดู และถ้าเปิดวิดเจ็ตซ้ำตอนที่มีตัวหนึ่งรันอยู่แล้ว จะแค่ดึงหน้าต่างเดิมขึ้นมาแทนที่จะเปิดตัวใหม่ซ้อนกัน ป้องกันไม่ให้มีสอง instance แอบดึงข้อมูลพร้อมกันจนโดน rate limit บ่อยขึ้น

**รองรับ:** Windows 10 และ 11 — ต้องมี [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) ซึ่งเครื่อง Windows 10/11 เกือบทั้งหมดมีอยู่แล้ว (Windows 11 มีมาให้ในตัว, Windows 10 มีมาพร้อม Edge) ถ้าเครื่องไหนไม่มีจริงๆ โหลดจากลิงก์นี้ก่อนได้เลย

**Uninstall:** ลบแคช WebView2 และ token ที่ paste ไว้ (ถ้ามี) ออกให้อัตโนมัติ บางเครื่องอาจเหลือไฟล์โปรแกรม 2 ไฟล์เล็กๆ ค้างไว้สักพัก (ไม่มีข้อมูลผู้ใช้ใดๆ) ถ้าโปรแกรมป้องกันไวรัสกำลังสแกนพอดีตอนลบ — ลบเองได้เลยถ้าเจอ

**หมายเหตุเรื่อง SmartScreen:** ตอนติดตั้งครั้งแรก Windows SmartScreen จะเตือนเพราะ installer ไม่ได้ code-sign (ใบรับรองมีค่าใช้จ่ายและต้องยืนยันตัวตน) — เรื่องนี้**แก้ด้วยโค้ดไม่ได้** เป็นเรื่องความน่าเชื่อถือที่ต้องสร้างสะสม ไม่ใช่บั๊ก มีแค่ 2 ทางเลือกจริงๆ: (1) ซื้อใบรับรอง code-signing ซึ่งมีค่าใช้จ่ายและต้องยืนยันตัวตน หรือ (2) กด *More info → Run anyway* (ปลอดภัย เพราะ source code เปิดให้ตรวจสอบได้ทั้งหมด) หรือ build จาก source เอง
