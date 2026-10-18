import { dl } from "../test/fixtures";
import { categoryCounts, matchesCategory, visibleDownloads } from "./filter";

describe("filter", () => {
  const a = dl({ file_name: "Movie.mp4", category: "video", status: "downloading", total_size: 100, downloaded: 50 });
  const b = dl({ file_name: "setup.exe", category: "programs", status: "completed", total_size: 10 });
  const c = dl({ file_name: "doc.pdf", category: "documents", status: "failed", in_queue: true, domain: "files.org" });
  const d = dl({ file_name: "x.torrent", category: "c1", status: "waiting", in_queue: true });
  const all = [a, b, c, d];

  it("matches status and type categories", () => {
    expect(matchesCategory(a, "active")).toBe(true);
    expect(matchesCategory(b, "completed")).toBe(true);
    expect(matchesCategory(c, "failed")).toBe(true);
    expect(matchesCategory(c, "queue")).toBe(true);
    expect(matchesCategory(a, "video")).toBe(true);
    expect(matchesCategory(d, "custom:c1")).toBe(true);
  });

  it("searches name and domain", () => {
    expect(visibleDownloads(all, "all", "files.org", { key: "name", dir: "asc" })).toEqual([c]);
    expect(visibleDownloads(all, "all", "SETUP", { key: "name", dir: "asc" })).toEqual([b]);
  });

  it("sorts", () => {
    const byName = visibleDownloads(all, "all", "", { key: "name", dir: "asc" }).map((x) => x.file_name);
    expect(byName).toEqual(["doc.pdf", "Movie.mp4", "setup.exe", "x.torrent"]);
    const byProgress = visibleDownloads(all, "all", "", { key: "progress", dir: "desc" }).map((x) => x.file_name);
    expect(byProgress[0]).toBe("setup.exe");
  });

  it("counts categories", () => {
    const counts = categoryCounts(all, [{ id: "c1", name: "Torrents", extensions: ["torrent"] }]);
    expect(counts.all).toBe(4);
    expect(counts.queue).toBe(2);
    expect(counts["custom:c1"]).toBe(1);
  });
});
