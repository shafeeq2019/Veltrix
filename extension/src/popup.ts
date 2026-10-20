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
const DECISIONS = ["accepted", "queued", "ignored", "duplicate"];
const msg = (key: string, ...subs: string[]) => ext.i18n.getMessage(key, subs);

// Static texts from _locales (follows the browser language).
document.documentElement.lang = ext.i18n.getUILanguage();
document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => (el.textContent = msg(el.dataset.i18n!)));

async function patch(p: Partial<Status>) {
  const stored = (await ext.storage.local.get("state")) as { state?: Record<string, unknown> };
  await ext.storage.local.set({ state: { ...(stored.state ?? {}), ...p } });
}

function render(s: Status) {
  enabledEl.checked = !s.paused;
  cookiesEl.checked = s.forwardCookies;
  statusEl.className = "status";
  if (s.hostStatus === "ok") {
    statusEl.textContent = msg("statusConnected", s.appVersion ?? "").trim();
    statusEl.classList.add("ok");
  } else if (s.hostStatus === "app_not_running") {
    statusEl.textContent = msg("statusAppNotRunning");
  } else if (s.hostStatus === "host_missing") {
    statusEl.textContent = msg("statusHostMissing");
    statusEl.classList.add("err");
  }
  // Decisions are stored as codes ("accepted", …), errors as text.
  const last = DECISIONS.includes(s.lastResult) ? msg(`decision_${s.lastResult}`) : s.lastResult;
  lastEl.textContent = last ? msg("lastResult", last) : "";
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
