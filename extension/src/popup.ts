import { ext } from "./native";

interface Status {
  paused: boolean;
  forwardCookies: boolean;
  hostStatus: "unknown" | "ok" | "app_not_running" | "host_missing";
  lastResult: string;
  appVersion: string | null;
}

const statusEl = document.getElementById("status")!;
const enabledEl = document.getElementById("enabled") as HTMLInputElement;
const cookiesEl = document.getElementById("cookies") as HTMLInputElement;
const lastEl = document.getElementById("last")!;

async function patch(p: Partial<Status>) {
  const stored = (await ext.storage.local.get("state")) as { state?: Record<string, unknown> };
  await ext.storage.local.set({ state: { ...(stored.state ?? {}), ...p } });
}

function render(s: Status) {
  enabledEl.checked = !s.paused;
  cookiesEl.checked = s.forwardCookies;
  statusEl.className = "status";
  if (s.hostStatus === "ok") {
    statusEl.textContent = `Verbunden mit OpenDownload Manager ${s.appVersion ?? ""}`;
    statusEl.classList.add("ok");
  } else if (s.hostStatus === "app_not_running") {
    statusEl.textContent = "Die App läuft nicht. Sie wird beim nächsten erkannten Download automatisch gestartet.";
  } else if (s.hostStatus === "host_missing") {
    statusEl.textContent = "OpenDownload Manager ist nicht installiert oder die Browser-Integration ist nicht registriert.";
    statusEl.classList.add("err");
  }
  lastEl.textContent = s.lastResult ? `Letzte Übergabe: ${s.lastResult}` : "";
}

enabledEl.addEventListener("change", () => void patch({ paused: !enabledEl.checked }));

cookiesEl.addEventListener("change", async () => {
  if (cookiesEl.checked) {
    // Must run inside the click handler (user gesture).
    const granted = await ext.permissions.request({ permissions: ["cookies"], origins: ["<all_urls>"] });
    if (!granted) {
      cookiesEl.checked = false;
      return;
    }
    await patch({ forwardCookies: true });
  } else {
    await patch({ forwardCookies: false });
    await ext.permissions.remove({ permissions: ["cookies"], origins: ["<all_urls>"] }).catch(() => false);
  }
});

ext.runtime.sendMessage({ type: "status" }).then((s: Status) => s && render(s));
