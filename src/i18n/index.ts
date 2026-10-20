// Tiny dependency-free i18n. Texts live in one file per language (de.ts is
// the source of truth); the active language comes from the settings in the
// store, "system" follows the language of the WebView.

import { createElement, Fragment, type ReactNode } from "react";
import { useStore } from "../lib/store";
import type { DownloadStatus, ErrorCode, LanguageSetting } from "../lib/types";
import { de } from "./de";
import { en } from "./en";
import type { Message } from "./types";

export type { Message };
export type MessageKey = keyof typeof de;
export type Messages = Record<MessageKey, Message>;
export type Lang = "de" | "en";
export type Params = Record<string, string | number>;

export const MESSAGES: Record<Lang, Messages> = { de, en };
export const LANGUAGES: { code: Lang; name: string }[] = [
  { code: "de", name: "Deutsch" },
  { code: "en", name: "English" },
];

/** Language for a setting value; "system" picks the browser language, falling back to English. */
export function resolveLanguage(setting: LanguageSetting | string | undefined, browser?: string): Lang {
  if (setting && setting !== "system" && setting in MESSAGES) return setting as Lang;
  const base = (browser ?? "").toLowerCase().split(/[-_]/)[0];
  return base in MESSAGES ? (base as Lang) : "en";
}

/** BCP 47 locale for number and date formatting. Uses the browser's regional
 * variant when it matches the language (en-GB, de-AT, …). */
export function localeFor(lang: Lang, browser?: string): string {
  if (browser && browser.toLowerCase().split(/[-_]/)[0] === lang && browser.includes("-")) return browser;
  return lang === "de" ? "de-DE" : "en-US";
}

const plurals: Partial<Record<Lang, Intl.PluralRules>> = {};

function pick(lang: Lang, msg: Message, params?: Params): string {
  if (typeof msg === "string") return msg;
  const n = Number(params?.count ?? 0);
  const rules = (plurals[lang] ??= new Intl.PluralRules(lang));
  return rules.select(n) === "one" ? msg.one : msg.other;
}

export function translate(lang: Lang, key: MessageKey, params?: Params): string {
  const text = pick(lang, MESSAGES[lang][key] ?? MESSAGES.de[key] ?? key, params);
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m));
}

/** Like `translate`, but placeholders may be React nodes (e.g. <strong>). */
export function translateNodes(lang: Lang, key: MessageKey, params: Record<string, ReactNode>): ReactNode {
  const parts = pick(lang, MESSAGES[lang][key], params as Params).split(/(\{\w+\})/);
  return parts.map((part, i) => {
    const m = /^\{(\w+)\}$/.exec(part);
    return createElement(Fragment, { key: i }, m && m[1] in params ? params[m[1]] : part);
  });
}

export interface I18n {
  lang: Lang;
  locale: string;
  t: (key: MessageKey, params?: Params) => string;
}

function browserLanguage(): string | undefined {
  return typeof navigator === "undefined" ? undefined : navigator.language;
}

function make(lang: Lang): I18n {
  return { lang, locale: localeFor(lang, browserLanguage()), t: (key, params) => translate(lang, key, params) };
}

const cache: Partial<Record<Lang, I18n>> = {};
function forLang(lang: Lang): I18n {
  return (cache[lang] ??= make(lang));
}

/** Current language outside of React components (actions, toasts). */
export function i18n(): I18n {
  return forLang(resolveLanguage(useStore.getState().settings?.language, browserLanguage()));
}

export function useI18n(): I18n {
  const setting = useStore((s) => s.settings?.language);
  return forLang(resolveLanguage(setting, browserLanguage()));
}

export function useT(): I18n["t"] {
  return useI18n().t;
}

export function statusKey(status: DownloadStatus): MessageKey {
  return `status.${status}`;
}

export function errorKey(code: ErrorCode): MessageKey {
  return `error.${code}`;
}

/** Translated text for a command error: by its code, else the backend message. */
export function errorText(e: unknown, t: I18n["t"] = i18n().t): string {
  if (e && typeof e === "object" && "code" in e) {
    const key = `error.${String((e as { code: unknown }).code)}`;
    if (key in de) return t(key as MessageKey);
  }
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}
