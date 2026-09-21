import { en } from "./lang/en";
import { ru } from "./lang/ru";

export type Lang = "ru" | "en";

export const dicts: Record<Lang, Record<string, string>> = { ru, en };

export function detectSystemLang(): Lang {
  try {
    // `userLanguage` is a legacy IE-only field, hence the structural type.
    const legacy = (navigator as Navigator & { userLanguage?: unknown }).userLanguage;
    const nav = navigator.language || (typeof legacy === "string" ? legacy : "") || "en";
    return nav.toLowerCase().startsWith("ru") ? "ru" : "en";
  } catch {
    return "en";
  }
}

export function tr(lang: Lang, key: string, params?: Record<string, string | number>): string {
  const d = dicts[lang] ?? en;
  let s = d[key] ?? (en as Record<string, string>)[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replace(`{${k}}`, String(v));
    }
  }
  return s;
}
