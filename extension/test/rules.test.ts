import { basename, DEFAULT_CONFIG, domainMatches, extensionOf, shouldCapture, type CaptureConfig } from "../src/rules";

const cfg = (over: Partial<CaptureConfig> = {}): CaptureConfig => ({ ...DEFAULT_CONFIG, ...over });

describe("shouldCapture", () => {
  it("captures listed file types by name, URL or MIME", () => {
    expect(shouldCapture({ url: "https://x.org/get?id=1", filename: "C:\\Users\\a\\Downloads\\setup.exe" }, cfg()).capture).toBe(true);
    expect(shouldCapture({ url: "https://x.org/files/ubuntu.iso" }, cfg()).capture).toBe(true);
    expect(shouldCapture({ url: "https://x.org/dl", mime: "application/zip; charset=binary" }, cfg()).capture).toBe(true);
  });

  it("ignores unlisted, html, streaming and non-http downloads", () => {
    expect(shouldCapture({ url: "https://x.org/a.png", filename: "a.png" }, cfg())).toEqual({ capture: false, reason: "type_not_listed" });
    expect(shouldCapture({ url: "https://x.org/page", mime: "text/html" }, cfg()).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/live.m3u8" }, cfg()).capture).toBe(false);
    expect(shouldCapture({ url: "blob:https://x.org/123", filename: "a.zip" }, cfg()).capture).toBe(false);
    expect(shouldCapture({ url: "data:application/zip;base64,AA", filename: "a.zip" }, cfg()).capture).toBe(false);
  });

  it("respects exclusions, size and private mode", () => {
    expect(shouldCapture({ url: "https://cdn.example.com/a.zip" }, cfg({ excluded_domains: ["example.com"] })).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/a.zip?nocapture=1" }, cfg({ excluded_url_patterns: ["nocapture"] })).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/a.zip" }, cfg({ excluded_file_types: ["zip"] })).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/a.zip", size: 100 }, cfg({ min_size_bytes: 1024 })).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/a.zip", size: -1 }, cfg({ min_size_bytes: 1024 })).capture).toBe(true);
    expect(shouldCapture({ url: "https://x.org/a.zip", incognito: true }, cfg()).capture).toBe(false);
    expect(shouldCapture({ url: "https://x.org/a.zip", incognito: true }, cfg({ capture_private: true })).capture).toBe(true);
    expect(shouldCapture({ url: "https://x.org/a.zip" }, cfg({ enabled: false })).capture).toBe(false);
  });

  it("treats large generic binaries as downloads", () => {
    expect(shouldCapture({ url: "https://x.org/blob", mime: "application/octet-stream", size: 5_000_000 }, cfg()).capture).toBe(true);
    expect(shouldCapture({ url: "https://x.org/blob", mime: "application/octet-stream", size: 5_000 }, cfg()).capture).toBe(false);
  });

  it("uses always_take_over types", () => {
    expect(shouldCapture({ url: "https://x.org/a.xyz" }, cfg({ always_take_over: ["xyz"] })).capture).toBe(true);
  });
});

describe("helpers", () => {
  it("parses names", () => {
    expect(basename("/home/a/b.zip")).toBe("b.zip");
    expect(basename("C:\\a\\b.zip")).toBe("b.zip");
    expect(extensionOf("archive.TAR.GZ")).toBe("gz");
    expect(extensionOf(".bashrc")).toBeNull();
    expect(extensionOf("noext")).toBeNull();
  });

  it("matches domains", () => {
    expect(domainMatches("dl.example.com", "example.com")).toBe(true);
    expect(domainMatches("badexample.com", "example.com")).toBe(false);
    expect(domainMatches("example.com", "*.example.com")).toBe(false);
  });
});
