// Builds the extension for Chromium (MV3 service worker) and Firefox (MV3
// background script) into dist/chrome and dist/firefox.
import { build } from "esbuild";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

// Public key that pins the unpacked Chromium extension id to
// pjmfnhocabgijmcbdcbkfeaogjnhffea (see docs/BROWSER_INTEGRATION.md).
const CHROME_KEY =
  "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAz7rNSjneAatBjiaclAyhJsDPrHmj92FXs3F8UcjvHh8AsHxeQuoOQrsU6OB5p7hvJJmSRuaBMQA+iCOeYZjZ4v39TTGw25gy2T9tOtSxXEhcbFhnM30yt/shrPmM/OfFUl8F4V5X1kjWP+jPZ1Kc+ZXBq9wILbV7ZPMNGeVWWzrec5y252Twze2B1xu+1oE8EztVVpMq0zSaXfI2Sn02sKh1WrbSaWTwgpi5p/wKD3onJF8RtIpPKP2deoBKTwBn3EdrF8mq1EnsiLPXoOumOXSj/UXhOPK05hpBnEnUEupSIuC+xyykbubNHtRuLo6Fb0z2LncDRwGnCgOyyGWt/QIDAQAB";

const base = {
  manifest_version: 3,
  // Texts come from static/_locales/<lang>/messages.json (browser language).
  name: "__MSG_extName__",
  version: pkg.version,
  description: "__MSG_extDescription__",
  default_locale: "en",
  icons: { 32: "icons/icon-32.png", 64: "icons/icon-64.png", 128: "icons/icon-128.png" },
  action: { default_popup: "popup.html", default_icon: { 32: "icons/icon-32.png", 64: "icons/icon-64.png" } },
  permissions: ["downloads", "nativeMessaging", "contextMenus", "storage"],
  optional_permissions: ["cookies"],
  optional_host_permissions: ["<all_urls>"],
};

const targets = {
  chrome: { ...base, key: CHROME_KEY, background: { service_worker: "background.js" }, minimum_chrome_version: "110" },
  firefox: {
    ...base,
    background: { scripts: ["background.js"] },
    browser_specific_settings: { gecko: { id: "veltrix@veltrix.local", strict_min_version: "128.0" } },
  },
};

for (const [name, manifest] of Object.entries(targets)) {
  const out = join(root, "dist", name);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  await build({
    entryPoints: { background: join(root, "src/background.ts"), popup: join(root, "src/popup.ts") },
    bundle: true,
    format: "iife",
    target: name === "chrome" ? "chrome110" : "firefox128",
    outdir: out,
    logLevel: "warning",
  });
  cpSync(join(root, "static"), out, { recursive: true });
  writeFileSync(join(out, "manifest.json"), JSON.stringify(manifest, null, 2));
  console.log(`built ${name} → ${out}`);
}
