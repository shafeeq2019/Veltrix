import { extractUrls, formatBytes, formatDuration, formatSpeed, fromBytesPerSecond, percent, toBytesPerSecond } from "./format";

describe("format", () => {
  it("formats bytes with binary units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1,5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5,0 MB");
    expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
    expect(formatBytes(null)).toBe("–");
  });

  it("formats speed and durations", () => {
    expect(formatSpeed(0)).toBe("–");
    expect(formatSpeed(2048)).toBe("2,0 KB/s");
    expect(formatDuration(42)).toBe("42 s");
    expect(formatDuration(125)).toBe("2 min 5 s");
    expect(formatDuration(3 * 3600 + 120)).toBe("3 h 2 min");
    expect(formatDuration(null)).toBe("–");
  });

  it("computes percent safely", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(10, null)).toBeNull();
    expect(percent(10, 0)).toBeNull();
    expect(percent(300, 200)).toBe(100);
  });

  it("converts KB/s and MB/s", () => {
    expect(toBytesPerSecond(1, "MB/s")).toBe(1048576);
    expect(toBytesPerSecond(512, "KB/s")).toBe(524288);
    expect(toBytesPerSecond(-1, "KB/s")).toBe(0);
    expect(fromBytesPerSecond(5 * 1048576)).toEqual({ value: 5, unit: "MB/s" });
    expect(fromBytesPerSecond(300 * 1024)).toEqual({ value: 300, unit: "KB/s" });
  });

  it("extracts only http(s) urls", () => {
    expect(extractUrls("see https://a.com/x.zip and http://b.org/y?z=1\njavascript:alert(1) ftp://c/d")).toEqual([
      "https://a.com/x.zip",
      "http://b.org/y?z=1",
    ]);
    expect(extractUrls("https://a.com/x https://a.com/x")).toHaveLength(1);
    expect(extractUrls("nothing here")).toEqual([]);
  });
});
