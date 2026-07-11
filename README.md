# 🦀 Usage Widget for Claude

A tiny, cute Windows 11 desktop widget that shows your **Claude plan usage limits** — the same numbers you see in Claude's *Plan usage limits* popup or Claude Code's `/usage` command. Featuring **Kani**, a little orange crab who gets progressively more nervous as you burn through your quota.

> **Unofficial.** This project is not affiliated with or endorsed by Anthropic. It talks to an undocumented endpoint that may change or stop working at any time. The mascot and icon are original artwork.

![theme](https://img.shields.io/badge/theme-orange%20%C3%97%20dark-d97757) ![platform](https://img.shields.io/badge/platform-Windows%2011-blue) ![stack](https://img.shields.io/badge/built%20with-Tauri%20v2-orange)

## ⬇️ Download

**[Download the installer (v0.1.3, ~1.3 MB)](installer/Usage%20Widget%20for%20Claude_0.1.3_x64-setup.exe)** — Windows 11 x64, no admin rights needed.

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

- **Reads** `%USERPROFILE%\.claude\.credentials.json` (Claude Code's own login file), **read-only**, to obtain your OAuth access token.
- **Sends** exactly one kind of request: `GET https://api.anthropic.com/api/oauth/usage` with that token. **No other network calls. No telemetry. No analytics. No auto-update.**
- The token never leaves the Rust backend process — the UI layer only ever receives percentages and timestamps.
- Config/cache lives at `%LOCALAPPDATA%\com.varintha.usagewidget\` (window position + the embedded WebView2 browser's own cache) — never secrets.
- If your Claude Code token has expired, the widget will *not* try to refresh it (that could break your Claude Code session). Just open Claude Code once and the widget picks up the fresh token.
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

วิดเจ็ตเล็กๆ บน Windows 11 แสดงเปอร์เซ็นต์การใช้งาน Claude ตามแพลนของคุณ (ตัวเลขเดียวกับหน้า *Plan usage limits* หรือคำสั่ง `/usage` ใน Claude Code)

**ถ้าใช้ Claude Code อยู่แล้ว:** ติดตั้งแล้วเปิดได้เลย ไม่ต้องตั้งค่าอะไร — วิดเจ็ตหา login ของ Claude Code ในเครื่องให้อัตโนมัติ

**ถ้าไม่ได้ใช้ Claude Code:** เปิดครั้งแรกจะมีหน้าให้วาง OAuth token (`sk-ant-oat…`) หนึ่งครั้ง โดย token ถูกเก็บใน Windows Credential Manager อย่างปลอดภัย

**ความปลอดภัย:** แอปอ่านไฟล์ login ของ Claude Code แบบอ่านอย่างเดียว ยิง API ไปที่ `api.anthropic.com` โดเมนเดียวเท่านั้น ไม่มี telemetry ไม่ส่งข้อมูลไปที่อื่นใดทั้งสิ้น และ token ไม่เคยหลุดออกจากตัวโปรแกรมฝั่ง Rust

**Uninstall:** ลบแคช WebView2 และ token ที่ paste ไว้ (ถ้ามี) ออกให้อัตโนมัติ บางเครื่องอาจเหลือไฟล์โปรแกรม 2 ไฟล์เล็กๆ ค้างไว้สักพัก (ไม่มีข้อมูลผู้ใช้ใดๆ) ถ้าโปรแกรมป้องกันไวรัสกำลังสแกนพอดีตอนลบ — ลบเองได้เลยถ้าเจอ

**หมายเหตุเรื่อง SmartScreen:** ตอนติดตั้งครั้งแรก Windows SmartScreen จะเตือนเพราะ installer ไม่ได้ code-sign (ใบรับรองมีค่าใช้จ่ายและต้องยืนยันตัวตน) — เรื่องนี้**แก้ด้วยโค้ดไม่ได้** เป็นเรื่องความน่าเชื่อถือที่ต้องสร้างสะสม ไม่ใช่บั๊ก มีแค่ 2 ทางเลือกจริงๆ: (1) ซื้อใบรับรอง code-signing ซึ่งมีค่าใช้จ่ายและต้องยืนยันตัวตน หรือ (2) กด *More info → Run anyway* (ปลอดภัย เพราะ source code เปิดให้ตรวจสอบได้ทั้งหมด) หรือ build จาก source เอง
