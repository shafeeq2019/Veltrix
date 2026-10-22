// Native messaging client. Each request uses its own port (connectNative),
// which also keeps a Chrome MV3 service worker alive while waiting for the
// user to answer the dialog in the desktop app.

export const HOST_NAME = "app.veltrix";
export const PROTOCOL_VERSION = 1;

type Api = typeof chrome;
// Firefox exposes `browser`, Chromium `chrome`; the subset we use is identical.
export const ext: Api = ((globalThis as unknown as { browser?: Api }).browser ?? chrome) as Api;

export type HostResponse =
  | { v: number; id: string; type: "pong"; app_version: string }
  | { v: number; id: string; type: "config"; config: import("./rules").CaptureConfig }
  | { v: number; id: string; type: "decision"; decision: "accepted" | "queued" | "ignored" | "duplicate" }
  | { v: number; id: string; type: "error"; code: string; message: string };

export class NativeError extends Error {
  constructor(
    message: string,
    readonly kind: "host_missing" | "app_not_running" | "timeout" | "protocol" | "other",
  ) {
    super(message);
  }
}

function newId(): string {
  return crypto.randomUUID().replace(/-/g, "");
}

export function classifyDisconnect(message: string): NativeError["kind"] {
  const m = message.toLowerCase();
  if (m.includes("not found") || m.includes("no such native application") || m.includes("forbidden")) return "host_missing";
  return "other";
}

export function request(body: Record<string, unknown>, timeoutMs: number): Promise<HostResponse> {
  return new Promise((resolve, reject) => {
    const id = newId();
    let done = false;
    let port: chrome.runtime.Port;
    try {
      port = ext.runtime.connectNative(HOST_NAME);
    } catch (e) {
      reject(new NativeError(String(e), "host_missing"));
      return;
    }
    const finish = (fn: () => void) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      try {
        port.disconnect();
      } catch {
        /* already gone */
      }
      fn();
    };
    const timer = setTimeout(() => finish(() => reject(new NativeError("Timeout waiting for Veltrix", "timeout"))), timeoutMs);
    port.onMessage.addListener((msg: HostResponse) => {
      if (!msg || msg.id !== id) return;
      if (msg.type === "error" && msg.code === "app_not_running") {
        finish(() => reject(new NativeError(msg.message, "app_not_running")));
      } else {
        finish(() => resolve(msg));
      }
    });
    port.onDisconnect.addListener((p) => {
      const err = (p as unknown as { error?: { message: string } }).error?.message ?? ext.runtime.lastError?.message ?? "Native host disconnected";
      finish(() => reject(new NativeError(err, classifyDisconnect(err))));
    });
    port.postMessage({ v: PROTOCOL_VERSION, id, ...body });
  });
}
