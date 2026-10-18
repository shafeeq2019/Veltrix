// Typed wrappers around the Tauri commands defined in src-tauri/src/commands.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AddDownloadRequest,
  AppInfo,
  CmdError,
  DownloadView,
  EngineEvent,
  IntegrationStatus,
  OfferResolution,
  OfferView,
  Priority,
  ProbeInfo,
  Settings,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as CmdError).message);
  return String(e);
}

export function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) return String((e as CmdError).code);
  return undefined;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  list: () => invoke<DownloadView[]>("list_downloads"),
  add: (request: AddDownloadRequest) => invoke<DownloadView>("add_download", { request }),
  probe: (url: string, referrer?: string | null, username?: string | null, password?: string | null) =>
    invoke<ProbeInfo>("probe_url", { url, referrer, username, password }),
  pause: (ids: string[]) => invoke<void>("pause_downloads", { ids }),
  resume: (ids: string[]) => invoke<void>("resume_downloads", { ids }),
  cancel: (ids: string[]) => invoke<void>("cancel_downloads", { ids }),
  retry: (ids: string[]) => invoke<void>("retry_downloads", { ids }),
  remove: (ids: string[], deleteFiles: boolean) => invoke<void>("remove_downloads", { ids, deleteFiles }),
  setPriority: (ids: string[], priority: Priority) => invoke<void>("set_priority", { ids, priority }),
  setInQueue: (ids: string[], inQueue: boolean) => invoke<void>("set_in_queue", { ids, inQueue }),
  moveInQueue: (id: string, delta: number) => invoke<void>("move_in_queue", { id, delta }),
  setCredentials: (id: string, username: string, password: string | null) =>
    invoke<void>("set_credentials", { id, username, password }),
  startQueue: () => invoke<void>("start_queue"),
  stopQueue: () => invoke<void>("stop_queue"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  openFile: (id: string, confirmed: boolean) => invoke<void>("open_file", { id, confirmed }),
  showInFolder: (id: string) => invoke<void>("show_in_folder", { id }),
  isExecutable: (fileName: string) => invoke<boolean>("is_executable", { fileName }),
  resolveOffer: (offerId: string, resolution: OfferResolution) =>
    invoke<void>("resolve_offer", { offerId, resolution }),
  pendingOffers: () => invoke<OfferView[]>("pending_offers"),
  integrationStatus: () => invoke<IntegrationStatus>("integration_status"),
  registerIntegration: () => invoke<IntegrationStatus>("register_browser_integration"),
  unregisterIntegration: () => invoke<IntegrationStatus>("unregister_browser_integration"),
};

export function onEngineEvent(cb: (e: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("engine-event", (ev) => cb(ev.payload));
}

export function onResync(cb: () => void): Promise<UnlistenFn> {
  return listen("engine-resync", () => cb());
}

export function onCaptureOffer(cb: (o: OfferView) => void): Promise<UnlistenFn> {
  return listen<OfferView>("capture-offer", (ev) => cb(ev.payload));
}

export function onOfferExpired(cb: (id: string) => void): Promise<UnlistenFn> {
  return listen<string>("capture-offer-expired", (ev) => cb(ev.payload));
}
