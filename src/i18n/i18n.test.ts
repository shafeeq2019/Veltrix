import { de } from "./de";
import { en } from "./en";
import { errorText, localeFor, MESSAGES, resolveLanguage, translate } from ".";

function shape(messages: Record<string, unknown>) {
  return Object.entries(messages)
    .map(([k, v]) => `${k}:${typeof v === "string" ? "s" : Object.keys(v as object).sort().join("/")}`)
    .sort();
}

function placeholders(messages: Record<string, unknown>) {
  return Object.fromEntries(
    Object.entries(messages).map(([k, v]) => [k, (JSON.stringify(v).match(/\{\w+\}/g) ?? []).sort().filter((p, i, a) => a.indexOf(p) === i)]),
  );
}

describe("i18n", () => {
  it("has the same keys, plural forms and placeholders in every language", () => {
    for (const messages of Object.values(MESSAGES)) {
      expect(shape(messages)).toEqual(shape(de));
      expect(placeholders(messages)).toEqual(placeholders(de));
    }
    expect(Object.values(en).every((v) => (typeof v === "string" ? v : v.other).trim().length > 0)).toBe(true);
  });

  it("interpolates and picks plural forms", () => {
    expect(translate("de", "table.remaining", { time: "5 s" })).toBe("noch 5 s");
    expect(translate("en", "table.remaining", { time: "5 s" })).toBe("5 s left");
    expect(translate("de", "app.remove.title", { count: 1 })).toBe("Download entfernen?");
    expect(translate("de", "app.remove.title", { count: 3 })).toBe("3 Downloads entfernen?");
    expect(translate("en", "app.remove.title", { count: 1 })).toBe("Remove download?");
    expect(translate("en", "app.remove.title", { count: 2 })).toBe("Remove 2 downloads?");
    expect(translate("en", "add.addedCount", { ok: 2, count: 3 })).toBe("Added 2 of 3 downloads");
  });

  it("resolves the language setting", () => {
    expect(resolveLanguage("de", "en-US")).toBe("de");
    expect(resolveLanguage("en", "de-DE")).toBe("en");
    expect(resolveLanguage("system", "de-AT")).toBe("de");
    expect(resolveLanguage("system", "en-GB")).toBe("en");
    expect(resolveLanguage("system", "fr-FR")).toBe("en");
    expect(resolveLanguage(undefined, undefined)).toBe("en");
    expect(localeFor("de", "en-US")).toBe("de-DE");
    expect(localeFor("en", "en-GB")).toBe("en-GB");
  });

  it("translates command errors by code", () => {
    const t = (key: Parameters<typeof translate>[1]) => translate("de", key);
    expect(errorText({ code: "disk_full", message: "No space left on device" }, t)).toBe("Datenträger voll");
    expect(errorText({ code: "something_new", message: "raw message" }, t)).toBe("raw message");
    expect(errorText("plain", t)).toBe("plain");
  });
});
