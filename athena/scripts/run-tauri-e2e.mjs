import os from "os";
import path from "path";
import { spawn } from "child_process";

const platform = process.platform;

// Tauri v2 webdriver support: Windows + Linux only.
if (platform === "darwin") {
  console.log(
    [
      "[athena audit:e2e] Skipping Tauri WebDriver E2E on macOS.",
      "Tauri v2 does not support WebDriver automation on macOS (no WKWebView driver).",
      "",
      "Options:",
      "- Run `npm run audit:e2e` on a Windows or Linux machine.",
      "- Or run in CI (recommended).",
    ].join("\n"),
  );
  process.exit(0);
}

const tauriDriverPath = path.resolve(os.homedir(), ".cargo", "bin", platform === "win32" ? "tauri-driver.exe" : "tauri-driver");

// Start WebdriverIO runner (it will spawn tauri-driver itself via wdio.conf.mjs).
const child = spawn("npx", ["wdio", "run", "e2e-tests/wdio.conf.mjs"], {
  stdio: "inherit",
  shell: true,
  env: {
    ...process.env,
    ATHENA_TAURI_DRIVER: tauriDriverPath,
  },
});

child.on("exit", (code) => process.exit(code ?? 1));

