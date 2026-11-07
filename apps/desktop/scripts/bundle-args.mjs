// Prints the extra `tauri build` arguments for this platform and version.
//
// Usage (from apps/desktop): node scripts/bundle-args.mjs [base-config.json]
//
// A pre-release version such as 1.11.0-rc.1 needs two changes:
// - Windows Installer only takes major.minor.patch.build, so the .msi gets
//   a numeric version (1.11.0-rc.1 becomes 1.11.0.1).
// - rpm refuses a "-" in a version, and both rpm and dpkg would sort
//   1.11.0-rc.1 above 1.11.0, blocking the upgrade to the final release.
//   So Linux builds the .AppImage only (the one the in-app updater uses).
// A stable version passes the base config through unchanged.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const tauriDir = path.join(here, "..", "src-tauri");
const { version } = JSON.parse(fs.readFileSync(path.join(tauriDir, "tauri.conf.json"), "utf8"));
const base = process.argv[2];
const config = base ? JSON.parse(fs.readFileSync(base, "utf8")) : {};
const args = [];

const [core, pre] = version.split("-");
if (pre) {
  if (process.platform === "win32") {
    const build = (pre.match(/(\d+)$/) || [, "0"])[1];
    config.bundle = config.bundle || {};
    config.bundle.windows = config.bundle.windows || {};
    config.bundle.windows.wix = { ...config.bundle.windows.wix, version: `${core}.${build}` };
  }
  if (process.platform === "linux") args.push("--bundles", "appimage");
}

if (Object.keys(config).length > 0) {
  // A file rather than inline JSON, which PowerShell would strip the quotes from.
  const out = path.join(tauriDir, "tauri.build.conf.json");
  fs.writeFileSync(out, JSON.stringify(config, null, 2));
  args.push("--config", "src-tauri/tauri.build.conf.json");
}
console.log(args.join(" "));
