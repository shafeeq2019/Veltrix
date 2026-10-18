import type { DownloadView } from "../lib/types";

let n = 0;
export function dl(over: Partial<DownloadView> = {}): DownloadView {
  n++;
  return {
    id: `id-${n}`,
    url: `https://example.com/file${n}.zip`,
    final_url: null,
    referrer: null,
    file_name: `file${n}.zip`,
    name_locked: false,
    save_dir: "/tmp",
    total_size: 1000,
    downloaded: 0,
    status: "paused",
    error: null,
    category: "archives",
    priority: "normal",
    in_queue: false,
    queue_position: n,
    resumable: true,
    etag: null,
    last_modified: null,
    mime: null,
    segments: [],
    connections: 0,
    expected_sha256: null,
    retry_count: 0,
    created_at: 1_700_000_000_000 + n,
    updated_at: 0,
    completed_at: null,
    avg_speed: 0,
    active_ms: 0,
    requires_auth: false,
    speed: 0,
    eta_secs: null,
    connections_active: 0,
    domain: "example.com",
    ...over,
  };
}
