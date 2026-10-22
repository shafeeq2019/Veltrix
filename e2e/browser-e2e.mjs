// End-to-end test of the complete hand-over chain:
//   Chromium + extension → native messaging host → desktop app (IPC) → engine → file on disk
//
// Linux only (needs a display; run under xvfb-run). Requirements:
//   cargo build -p veltrix -p veltrix-native-host
//   (cd extension && npm run build)
//   a Chromium build that allows --load-extension (CHROMIUM env var, default /opt/pw-browsers/chromium)
// Usage: xvfb-run -a node e2e/browser-e2e.mjs
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import http from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = mkdtempSync(join(tmpdir(), "odm-e2e-"));
const dataDir = join(work, "data");
const downloads = join(work, "downloads");
const profile = join(work, "chrome-profile");
mkdirSync(dataDir, { recursive: true });
mkdirSync(downloads, { recursive: true });
mkdirSync(join(profile, "NativeMessagingHosts"), { recursive: true });

const SIZE = 24 * 1024 * 1024 + 321;
const content = Buffer.alloc(SIZE);
for (let i = 0; i < SIZE; i++) content[i] = (i * 31 + 7) % 251;
const sha = (b) => createHash("sha256").update(b).digest("hex");

// 1. Local web server: a page with a download link; the file supports ranges.
let rangeRequests = 0;
const server = http.createServer((req, res) => {
  if (req.url === "/") {
    res.writeHead(200, { "content-type": "text/html" });
    res.end('<a id="dl" href="/files/get?id=42">Download</a><script>setTimeout(() => document.getElementById("dl").click(), 4000)</script>');
    return;
  }
  if (req.url?.startsWith("/files/get")) {
    const headers = {
      "content-type": "application/zip",
      "content-disposition": 'attachment; filename="E2E Testarchiv.zip"',
      "accept-ranges": "bytes",
      etag: '"e2e-v1"',
    };
    const m = /bytes=(\d+)-(\d*)/.exec(req.headers.range ?? "");
    if (m) {
      rangeRequests++;
      const start = Number(m[1]);
      const end = m[2] ? Math.min(Number(m[2]), SIZE - 1) : SIZE - 1;
      res.writeHead(206, { ...headers, "content-range": `bytes ${start}-${end}/${SIZE}`, "content-length": end - start + 1 });
      res.end(content.subarray(start, end + 1));
    } else {
      // Full responses (the browser's own download) are throttled so the
      // extension has time to pause it, like on a real network.
      res.writeHead(200, { ...headers, "content-length": SIZE });
      let pos = 0;
      const timer = setInterval(() => {
        if (pos >= SIZE || res.destroyed) {
          clearInterval(timer);
          res.end();
          return;
        }
        res.write(content.subarray(pos, pos + 256 * 1024));
        pos += 256 * 1024;
      }, 100);
      res.on("close", () => clearInterval(timer));
    }
    return;
  }
  res.writeHead(404).end();
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

// 2. App settings: download folder + automatic take-over (no dialog needed).
execFileSync("python3", [
  "-c",
  `import sqlite3,json,sys
c=sqlite3.connect(sys.argv[1])
c.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)")
c.execute("INSERT OR REPLACE INTO settings VALUES ('app', ?)", (json.dumps({"download_dir": sys.argv[2], "capture": {"auto_start": True}}),))
c.commit()`,
  join(dataDir, "odm.sqlite"),
  downloads,
]);

// 3. Native messaging manifest inside the Chromium profile.
const host = join(root, "target/debug/veltrix-native-host");
writeFileSync(
  join(profile, "NativeMessagingHosts", "app.veltrix.json"),
  JSON.stringify({
    name: "app.veltrix",
    description: "e2e",
    path: host,
    type: "stdio",
    allowed_origins: ["chrome-extension://pjmfnhocabgijmcbdcbkfeaogjnhffea/"],
  }),
);

// 4. Desktop app in background mode.
// Software rendering keeps WebKitGTK happy on headless CI machines.
const env = { ...process.env, ODM_DATA_DIR: dataDir, ODM_LOG: "info", WEBKIT_DISABLE_COMPOSITING_MODE: "1", WEBKIT_DISABLE_DMABUF_RENDERER: "1", LIBGL_ALWAYS_SOFTWARE: "1" };
const app = spawn(join(root, "target/debug/veltrix"), ["--background"], { env, stdio: ["ignore", "inherit", "inherit"] });
app.on("exit", (code, signal) => console.error(`app exited: code=${code} signal=${signal}`));
const deadline = Date.now() + 60_000;
while (!existsSync(join(dataDir, "ipc-token")) || !existsSync(join(dataDir, "ipc.sock"))) {
  if (Date.now() > deadline) {
    try {
      console.error(readFileSync(join(dataDir, "logs", "odm.log"), "utf8"));
    } catch {
      console.error("no app log written");
    }
    console.error("data dir:", readdirSync(dataDir));
    app.kill("SIGKILL");
    server.close();
    throw new Error("app did not start");
  }
  await new Promise((r) => setTimeout(r, 200));
}

let failed = false;
let browser;
try {
  // 5. Chromium with the unpacked extension (the host inherits ODM_DATA_DIR).
  // Started directly instead of via Playwright, whose download handling
  // bypasses the browser's normal download flow.
  const ext = join(root, "extension/dist/chrome");
  browser = spawn(
    process.env.CHROMIUM ?? "/opt/pw-browsers/chromium",
    [`--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", "--disable-gpu", ...(process.getuid?.() === 0 ? ["--no-sandbox"] : []), `--disable-extensions-except=${ext}`, `--load-extension=${ext}`, base + "/"],
    { env, stdio: "ignore" },
  );
  // 6. The file must arrive in the app's download folder, byte-exact.
  const target = join(downloads, "E2E Testarchiv.zip");
  const until = Date.now() + 60_000;
  while (!(existsSync(target) && statSync(target).size === SIZE)) {
    if (Date.now() > until) throw new Error(`file not downloaded by the app; folder: ${readdirSync(downloads)}`);
    await new Promise((r) => setTimeout(r, 250));
  }
  if (sha(readFileSync(target)) !== sha(content)) throw new Error("content mismatch");
  console.log(`OK: app downloaded ${SIZE} bytes via ${rangeRequests} range requests`);
  if (rangeRequests < 2) throw new Error("expected a segmented download");
} catch (e) {
  failed = true;
  console.error("E2E FAILED:", e);
  try {
    console.error(readFileSync(join(dataDir, "logs", "odm.log"), "utf8").split("\n").slice(-30).join("\n"));
  } catch (err) {
    console.error("no app log", err);
  }
} finally {
  browser?.kill("SIGTERM");
  app.kill("SIGTERM");
  server.close();
}
process.exit(failed ? 1 : 0);
