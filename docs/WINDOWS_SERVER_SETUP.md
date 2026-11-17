# Run ATHENA on a Windows server PC

ATHENA can run all the time on a Windows PC (for example one that also hosts Minecraft and Plex).
People then use it from any browser on the home network, each signed in as themselves.

## What server mode does

- **Starts hidden.** The window stays hidden; ATHENA keeps running with nobody at the keyboard.
- **Serves the full app to browsers** on port 8787. Every screen works from a browser, with the
  same sign-in and clearance rules as at the station.
- **Keeps each person separate.** Each browser has its own sign-in, and a chat reply goes only to
  the person who asked.
- **Unloads the AI when idle.** The model unloads after 2 idle minutes and only one is loaded at
  a time, so it uses memory only while someone is chatting. The next question after a pause
  takes a few extra seconds while the model loads.
- **Yields to other programs.** It runs at below-normal priority, so Minecraft and Plex get the
  CPU first.
- **Keeps watching.** Alerts, library updates and the fleet link run on their schedules,
  according to the Vault settings.

## Before you start

- **Memory.** Minecraft usually takes 4–8 GB, Plex more while it transcodes, and a 3B model
  about 3 GB while loaded (an 8B model about 5 GB). 16 GB is comfortable; 8 GB is tight.
- **Disk.** Encyclopedias and packs grow. Choose a data folder on a large drive.
- **A fixed address.** In your router, give the server a DHCP reservation so its address stays
  the same.

## 1. Build the Windows installer

Build it on the server or on any Windows PC. You need Node.js 20 or newer, Rust
(rustup.rs), and the Visual Studio C++ Build Tools. Windows 11 already includes WebView2.

```powershell
cd athena
npm ci
npm run tauri build
```

The installer is in `athena\src-tauri\target\release\bundle\nsis\`. Run it on the server.

**Building on a Mac instead.** Install `brew install nsis llvm lld`, `rustup target add
x86_64-pc-windows-msvc` and `cargo install --locked cargo-xwin` (it downloads Microsoft's Windows
build libraries, under Microsoft's licence), then:

```bash
export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc
```

This builds `src-tauri/target/x86_64-pc-windows-msvc/release/athena.exe`. Homebrew's `makensis`
3.13 can crash while making the installer; if it does, install by hand: copy `athena.exe`, the
Windows helper programs renamed without their suffix (`bin/pmtiles-x86_64-pc-windows-msvc.exe` →
`pmtiles.exe`, likewise `tesseract`, `pdftoppm`, `kiwix-serve`, and
`ollama-x86_64-pc-windows-msvc.exe` → `ollama.exe`), and the `resources` and `payload` folders into
`%LOCALAPPDATA%\ATHENA` on the server.

**Helper libraries.** The Windows helper programs do not start without their libraries (Windows
reports a missing DLL and the program exits at once). Run `scripts/fetch_windows_tools.sh`, which
downloads the official builds and checks them, then copy:

| From `src-tauri/bin/windows-tools/` | To `%LOCALAPPDATA%\ATHENA\` | Used for |
|---|---|---|
| `kiwix/*.dll` | the folder itself, next to `kiwix-serve.exe` | the offline library and chat search over ZIM archives |
| `tesseract/` | `tools\tesseract\` | reading text in scans and photos (OCR) |
| `pdftoppm/` | `tools\pdftoppm\` | reading scanned PDF pages |
| `ollama/lib/` | `lib\` (giving `lib\ollama\ggml-*.dll` next to `ollama.exe`) | fast AI answers: without these CPU libraries Ollama falls back to a generic path about 4× slower, and chat answers time out |

Tesseract and Poppler keep their own folders so their libraries never mix with ATHENA's.

Installing `lld` can upgrade Homebrew's LLVM and break Homebrew's Rust (`rustc` fails with a
missing LLVM symbol). `brew upgrade rust` fixes it.

## 2. The AI model

ATHENA brings its own Ollama and starts it. The setup script keeps models in the data folder
(`<DataDir>\user\models\ollama`). The default model is `llama3.2:1b` (about 1.3 GB), which suits a
server with 8 GB of memory shared with games and media; ATHENA downloads it on first use. It runs on
the CPU if no GPU is free, and the first answer after the model has unloaded can take half a minute.

## 3. Run the setup script

Open PowerShell with **Run as administrator**:

```powershell
cd athena\server\windows
powershell -ExecutionPolicy Bypass -File .\install-athena-server.ps1 -DataDir "D:\AthenaData"
```

To bring your existing library and accounts across, copy the station's data folder into the new
`-DataDir` first. On a Mac it is `~/Library/Application Support/com.athena.micro`.

The script finds ATHENA, sets the data folder, opens port 8787 on private networks, creates the
"ATHENA Server" startup task and starts it. It prints the address to open, such as
`http://192.168.1.20:8787`.

## 4. Keep a Windows session signed in (required)

ATHENA runs browser requests through a hidden Windows web view (WebView2), and Windows only starts
it inside a signed-in desktop session. Without one, pages load but every action fails with "failed
to receive message from webview". So the task starts ATHENA when your Windows account signs in,
and the server must stay signed in: turn on automatic sign-in for that account (for example with
Microsoft's Sysinternals Autologon tool) so ATHENA comes back after a power cut or update. You can
lock the screen; ATHENA keeps running.

## 5. Use it

Open the address on a phone, tablet or laptop on the same network and sign in with your name
and PIN. On a new data folder, ATHENA creates one admin, **Caesar** (rank 5), and prints its PIN
once to its console output; add your own records on the Personnel screen and change or retire
Caesar's PIN. Each person needs their own personnel record (rank 4 or higher to add people).

## Safety

- **Keep it on your home network.** Do not forward port 8787 on your router: sign-in is name and
  PIN over plain HTTP. For access away from home, use a VPN such as Tailscale, or the fleet
  relay for station-to-station messages.
- **Wrong PINs lock the name out.** After 5 wrong PINs for a name, browser sign-in for that name
  is locked for 5 minutes.
- **Windows must call the network Private.** The firewall rule applies only to private
  networks.

## Limits

- **Restarts sign everyone out.** Browser sign-ins live in memory, so after a restart everyone
  signs in again.
- **No folder picker in a browser.** The station's file dialogs cannot open in a browser. Where
  a screen asks for a folder, type the path on the server (for example `D:\Manuals`).
- **USB and export tools are station-only.** USB export and other host-only tools work only at
  the server itself.

## Update it

Stop ATHENA **and its helper programs** before replacing `athena.exe`. A helper left running
(Ollama, or a WebView2 process) can keep port 8787 held after ATHENA exits, and the new ATHENA
then starts without its web server:

```powershell
Stop-ScheduledTask -TaskName "ATHENA Server"
Get-Process athena,ollama,msedgewebview2,kiwix-serve -ErrorAction SilentlyContinue | Stop-Process -Force
# copy the new athena.exe into %LOCALAPPDATA%\ATHENA, then:
Start-ScheduledTask -TaskName "ATHENA Server"
```

Updating signs everyone out.

## Remove it

```powershell
powershell -ExecutionPolicy Bypass -File .\install-athena-server.ps1 -Uninstall
```

This removes the task, the firewall rule and the settings. The data folder is left in place.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `-Port` | 8787 | Web port. |
| `-ModelIdleMinutes` | 2 | Minutes an idle model stays loaded. |
| `-DataDir` | Windows app data | Where the library, database and packs live. |
