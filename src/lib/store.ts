import { create } from "zustand";
import type { CategoryKey, Sort } from "./filter";
import type { DownloadView, EngineEvent, OfferView, Settings } from "./types";

export interface Toast {
  id: number;
  kind: "info" | "error" | "success";
  text: string;
}

interface AppStore {
  downloads: Record<string, DownloadView>;
  totalSpeed: number;
  queueRunning: boolean;
  settings: Settings | null;
  selected: string[];
  anchor: string | null;
  category: CategoryKey;
  search: string;
  sort: Sort;
  offers: OfferView[];
  toasts: Toast[];

  setAll: (list: DownloadView[]) => void;
  applyEvent: (e: EngineEvent) => void;
  setSettings: (s: Settings) => void;
  setQueueRunning: (r: boolean) => void;
  setCategory: (c: CategoryKey) => void;
  setSearch: (q: string) => void;
  setSort: (s: Sort) => void;
  select: (id: string, mode: "single" | "toggle" | "range", visible: string[]) => void;
  setSelected: (ids: string[]) => void;
  pushOffer: (o: OfferView) => void;
  dropOffer: (id: string) => void;
  toast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
}

let toastSeq = 1;

export const useStore = create<AppStore>((set, get) => ({
  downloads: {},
  totalSpeed: 0,
  queueRunning: false,
  settings: null,
  selected: [],
  anchor: null,
  category: "all",
  search: "",
  sort: { key: "added", dir: "desc" },
  offers: [],
  toasts: [],

  setAll: (list) => set({ downloads: Object.fromEntries(list.map((d) => [d.id, d])) }),

  applyEvent: (e) => {
    switch (e.type) {
      case "added":
      case "updated":
        set((s) => ({ downloads: { ...s.downloads, [e.download.id]: e.download } }));
        break;
      case "removed":
        set((s) => {
          const next = { ...s.downloads };
          delete next[e.id];
          return { downloads: next, selected: s.selected.filter((x) => x !== e.id) };
        });
        break;
      case "progress":
        set((s) => {
          const next = { ...s.downloads };
          const live = new Set(e.items.map((i) => i.id));
          for (const it of e.items) {
            const d = next[it.id];
            if (!d) continue;
            next[it.id] = {
              ...d,
              downloaded: it.downloaded,
              total_size: it.total_size ?? d.total_size,
              speed: it.speed,
              eta_secs: it.eta_secs,
              connections_active: it.connections_active,
              status: it.status,
              avg_speed: it.avg_speed || d.avg_speed,
            };
          }
          // Downloads not reported any more have no live speed.
          for (const id of Object.keys(next)) {
            if (!live.has(id) && next[id].speed !== 0 && !["downloading", "connecting"].includes(next[id].status)) {
              next[id] = { ...next[id], speed: 0, eta_secs: null, connections_active: 0 };
            }
          }
          return { downloads: next, totalSpeed: e.total_speed };
        });
        break;
      case "queue_state":
        set({ queueRunning: e.running });
        break;
      case "completed":
      case "failed":
        break;
    }
    // Idle: no progress events arrive any more, so reset the total.
    const anyActive = Object.values(get().downloads).some((d) => d.status === "downloading" || d.status === "connecting");
    if (!anyActive && get().totalSpeed !== 0) set({ totalSpeed: 0 });
  },

  setSettings: (settings) => set({ settings }),
  setQueueRunning: (queueRunning) => set({ queueRunning }),
  setCategory: (category) => set({ category, selected: [], anchor: null }),
  setSearch: (search) => set({ search }),
  setSort: (sort) => set({ sort }),

  select: (id, mode, visible) =>
    set((s) => {
      if (mode === "single") return { selected: [id], anchor: id };
      if (mode === "toggle") {
        const has = s.selected.includes(id);
        return { selected: has ? s.selected.filter((x) => x !== id) : [...s.selected, id], anchor: id };
      }
      const a = s.anchor ? visible.indexOf(s.anchor) : -1;
      const b = visible.indexOf(id);
      if (a < 0 || b < 0) return { selected: [id], anchor: id };
      const [from, to] = a < b ? [a, b] : [b, a];
      return { selected: visible.slice(from, to + 1) };
    }),
  setSelected: (selected) => set({ selected }),

  pushOffer: (o) => set((s) => ({ offers: s.offers.some((x) => x.offer_id === o.offer_id) ? s.offers : [...s.offers, o] })),
  dropOffer: (id) => set((s) => ({ offers: s.offers.filter((o) => o.offer_id !== id) })),

  toast: (kind, text) => {
    const id = toastSeq++;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, kind, text }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 8000 : 4000);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
