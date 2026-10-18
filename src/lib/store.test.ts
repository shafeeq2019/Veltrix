import { dl } from "../test/fixtures";
import { useStore } from "./store";

describe("store", () => {
  beforeEach(() => useStore.setState({ downloads: {}, selected: [], anchor: null, totalSpeed: 0 }));

  it("applies engine events", () => {
    const a = dl({ status: "connecting" });
    const s = useStore.getState();
    s.applyEvent({ type: "added", download: a });
    expect(useStore.getState().downloads[a.id]).toBeDefined();
    s.applyEvent({
      type: "progress",
      total_speed: 500,
      items: [{ id: a.id, downloaded: 300, total_size: 1000, speed: 500, avg_speed: 400, eta_secs: 1, connections_active: 4, status: "downloading" }],
    });
    const d = useStore.getState().downloads[a.id];
    expect(d.downloaded).toBe(300);
    expect(d.status).toBe("downloading");
    expect(d.connections_active).toBe(4);
    expect(useStore.getState().totalSpeed).toBe(500);
    s.applyEvent({ type: "updated", download: { ...d, status: "completed", speed: 0 } });
    expect(useStore.getState().totalSpeed).toBe(0);
    s.applyEvent({ type: "removed", id: a.id });
    expect(useStore.getState().downloads[a.id]).toBeUndefined();
  });

  it("supports single, toggle and range selection", () => {
    const ids = ["a", "b", "c", "d"];
    const { select } = useStore.getState();
    select("b", "single", ids);
    select("d", "range", ids);
    expect(useStore.getState().selected).toEqual(["b", "c", "d"]);
    select("c", "toggle", ids);
    expect(useStore.getState().selected).toEqual(["b", "d"]);
  });
});
