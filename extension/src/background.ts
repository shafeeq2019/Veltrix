// Background script / service worker.
//
// Uses only the official downloads, contextMenus, storage and nativeMessaging
// APIs. It sees nothing but downloads the user starts; page navigation,
// images, scripts and API calls never reach the downloads API. Nothing is
// sent anywhere except to the local native messaging host, and only for a
// download that matches the user's capture rules.

import { ext, NativeError, request, type HostResponse } from "./native";
import { basename, DEFAULT_CONFIG, shouldCapture, type CaptureConfig } from "./rules";

const CONFIG_TTL_MS = 5 * 60 * 1000;
const OFFER_TIMEOUT_MS = 150_000;
const MENU_ID = "odm-download-link";

interface LocalState {
  paused: boolean;
  forwardCookies: boolean;
  config: CaptureConfig | null;
  configAt: number;
  hostStatus: "unknown" | "ok" | "app_not_running" | "host_missing";
  lastResult: string;
}

const DEFAULT_STATE: LocalState = {
  paused: false,
  forwardCookies: false,
  config: null,
  configAt: 0,
  hostStatus: "unknown",
  lastResult: "",
};

async function getState(): Promise<LocalState> {
  const stored = (await ext.storage.local.get("state")) as { state?: Partial<LocalState> };
  return { ...DEFAULT_STATE, ...(stored.state ?? {}) };
}

async function patchState(patch: Partial<LocalState>): Promise<void> {
  const s = await getState();
  await ext.storage.local.set({ state: { ...s, ...patch } });
}

async function setBadge(kind: "ok" | "error" | "none") {
  const text = kind === "ok" ? "✓" : kind === "error" ? "!" : "";
  await ext.action.setBadgeText({ text });
  if (kind !== "none") await ext.action.setBadgeBackgroundColor({ color: kind === "ok" ? "#15803d" : "#c2283b" });
  if (kind === "ok") setTimeout(() => void ext.action.setBadgeText({ text: "" }), 4000);
}

/** Capture config from the app (single source of truth), cached. */
async function getConfig(force = false): Promise<CaptureConfig> {
  const s = await getState();
  if (!force && s.config && Date.now() - s.configAt < CONFIG_TTL_MS) return s.config;
  try {
    const resp = await request({ type: "get_config" }, 10_000);
    if (resp.type === "config") {
      await patchState({ config: resp.config, configAt: Date.now(), hostStatus: "ok" });
      return resp.config;
    }
  } catch (e) {
    const kind = e instanceof NativeError ? e.kind : "other";
    await patchState({ hostStatus: kind === "host_missing" ? "host_missing" : "app_not_running" });
  }
  return s.config ?? DEFAULT_CONFIG;
}

async function cookieHeader(url: string): Promise<string | undefined> {
  const s = await getState();
  if (!s.forwardCookies || !ext.cookies) return undefined;
  try {
    const has = await ext.permissions.contains({ permissions: ["cookies"], origins: [new URL(url).origin + "/*"] });
    if (!has) return undefined;
    const cookies = await ext.cookies.getAll({ url });
    if (cookies.length === 0) return undefined;
    return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch {
    return undefined;
  }
}

async function offer(body: Record<string, unknown>): Promise<HostResponse> {
  return request({ type: "offer", ...body }, OFFER_TIMEOUT_MS);
}

const inFlight = new Set<number>();

async function handleDownload(item: chrome.downloads.DownloadItem): Promise<void> {
  if (inFlight.has(item.id) || item.byExtensionId === ext.runtime.id || item.state !== "in_progress") return;
  const s = await getState();
  if (s.paused || s.hostStatus === "host_missing") {
    // Re-check the host occasionally so installing the app later works.
    if (s.hostStatus === "host_missing" && Date.now() - s.configAt > CONFIG_TTL_MS) void getConfig(true);
    return;
  }
  const url = item.finalUrl || item.url;
  // blob:, data: and file: downloads only exist inside the browser.
  if (!/^https?:/i.test(item.url) || !/^https?:/i.test(url)) return;
  const config = await getConfig();
  const size = item.totalBytes > 0 ? item.totalBytes : item.fileSize > 0 ? item.fileSize : undefined;
  const verdict = shouldCapture(
    { url: item.url, finalUrl: item.finalUrl, filename: item.filename ? basename(item.filename) : undefined, mime: item.mime, size, incognito: item.incognito },
    config,
  );
  if (!verdict.capture) return;

  inFlight.add(item.id);
  try {
    // Hold the browser download while the user decides; nothing is lost if
    // the app declines or is unavailable — the browser simply continues.
    try {
      await ext.downloads.pause(item.id);
    } catch {
      return; // already finished or not pausable
    }
    let resp: HostResponse;
    try {
      resp = await offer({
        url,
        final_url: item.finalUrl && item.finalUrl !== item.url ? item.finalUrl : undefined,
        filename: item.filename ? basename(item.filename) : undefined,
        mime: item.mime || undefined,
        size,
        referrer: config.send_referrer && /^https?:/i.test(item.referrer ?? "") ? item.referrer : undefined,
        cookies: await cookieHeader(url),
        source: "browser_download",
        private: item.incognito,
      });
    } catch (e) {
      const kind = e instanceof NativeError ? e.kind : "other";
      await patchState({ hostStatus: kind === "host_missing" ? "host_missing" : "app_not_running", lastResult: String((e as Error).message) });
      await setBadge("error");
      await ext.downloads.resume(item.id).catch(() => {});
      return;
    }
    if (resp.type === "decision" && resp.decision !== "ignored") {
      await ext.downloads.cancel(item.id).catch(() => {});
      await ext.downloads.erase({ id: item.id }).catch(() => {});
      await patchState({ hostStatus: "ok", lastResult: resp.decision });
      await setBadge("ok");
    } else {
      await ext.downloads.resume(item.id).catch(() => {});
      await patchState({ hostStatus: "ok", lastResult: resp.type === "error" ? resp.message : "ignored" });
      if (resp.type === "error") await setBadge("error");
    }
  } finally {
    inFlight.delete(item.id);
  }
}

// Chromium: onDeterminingFilename knows the final name (Content-Disposition)
// and MIME type. We never change the name; suggest() is called right away
// so the browser is not blocked. Firefox has no such event: use onCreated.
const downloadsApi = ext.downloads as typeof chrome.downloads;
if (downloadsApi.onDeterminingFilename) {
  downloadsApi.onDeterminingFilename.addListener((item, suggest) => {
    suggest();
    void handleDownload(item);
  });
} else {
  downloadsApi.onCreated.addListener((item) => void handleDownload(item));
}

function createMenus() {
  ext.contextMenus.removeAll(() => {
    ext.contextMenus.create({ id: MENU_ID, title: "Mit OpenDownload Manager herunterladen", contexts: ["link", "video", "audio"] });
  });
}

ext.runtime.onInstalled.addListener(() => {
  createMenus();
  void getConfig(true);
});
ext.runtime.onStartup.addListener(() => void getConfig(true));

ext.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== MENU_ID) return;
  const url = info.linkUrl || info.srcUrl;
  if (!url || !/^https?:/i.test(url)) {
    await patchState({ lastResult: "Dieser Link kann nicht übernommen werden (kein http/https)." });
    await setBadge("error");
    return;
  }
  const config = await getConfig();
  const page = info.pageUrl ?? tab?.url;
  try {
    await offer({
      url,
      referrer: config.send_referrer && page && /^https?:/i.test(page) ? page : undefined,
      cookies: await cookieHeader(url),
      source: "context_menu",
      private: tab?.incognito ?? false,
    });
    await setBadge("ok");
  } catch (e) {
    await patchState({ lastResult: String((e as Error).message) });
    await setBadge("error");
  }
});

// Messages from the popup.
ext.runtime.onMessage.addListener((msg: { type: string }, _sender, sendResponse) => {
  if (msg?.type === "status") {
    (async () => {
      let app: string | null = null;
      let status: LocalState["hostStatus"] = "unknown";
      try {
        const r = await request({ type: "ping" }, 5000);
        if (r.type === "pong") {
          app = r.app_version;
          status = "ok";
        }
      } catch (e) {
        status = e instanceof NativeError && e.kind === "host_missing" ? "host_missing" : "app_not_running";
      }
      await patchState({ hostStatus: status });
      if (status === "ok") await getConfig(true);
      sendResponse({ ...(await getState()), appVersion: app });
    })();
    return true;
  }
  return false;
});
