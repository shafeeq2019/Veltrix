import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(__dirname, "..");
const locales = join(root, "static/_locales");
type Messages = Record<string, { message: string; placeholders?: Record<string, { content: string }> }>;
const read = (lang: string): Messages => JSON.parse(readFileSync(join(locales, lang, "messages.json"), "utf8"));

describe("extension locales", () => {
  const langs = readdirSync(locales);
  const en = read("en");

  it("has German and English", () => {
    expect(langs).toEqual(expect.arrayContaining(["de", "en"]));
  });

  it("uses the same keys and placeholders in every language", () => {
    for (const lang of langs) {
      const m = read(lang);
      expect(Object.keys(m).sort()).toEqual(Object.keys(en).sort());
      for (const [key, value] of Object.entries(m)) {
        expect(value.message.trim(), `${lang}/${key}`).not.toBe("");
        expect(Object.keys(value.placeholders ?? {}), `${lang}/${key}`).toEqual(Object.keys(en[key].placeholders ?? {}));
      }
    }
  });

  it("defines every message the code and manifest use", () => {
    const sources = [
      ...["background.ts", "popup.ts"].map((f) => readFileSync(join(root, "src", f), "utf8")),
      readFileSync(join(root, "static/popup.html"), "utf8"),
      readFileSync(join(root, "scripts/build.mjs"), "utf8"),
    ].join("\n");
    const used = [
      ...sources.matchAll(/getMessage\("(\w+)"/g),
      ...sources.matchAll(/msg\("(\w+)"/g),
      ...sources.matchAll(/data-i18n="(\w+)"/g),
      ...sources.matchAll(/__MSG_(\w+)__/g),
    ].map((m) => m[1]);
    expect(used.length).toBeGreaterThan(10);
    for (const key of used) expect(en, key).toHaveProperty(key);
    for (const d of ["accepted", "queued", "ignored", "duplicate"]) expect(en).toHaveProperty(`decision_${d}`);
  });
});
