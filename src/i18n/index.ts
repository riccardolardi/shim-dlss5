/**
 * Minimal string table. English only for now; the table is keyed so other
 * languages can be dropped in later without touching components.
 */
import en from "./en.json";

export type Key = keyof typeof en;
type Vars = Record<string, string | number>;

const tables: Record<string, Record<string, string>> = { en };
let current = "en";

export function setLanguage(lang: string) {
  current = lang in tables ? lang : "en";
}

export function t(key: Key, vars?: Vars): string {
  const raw = tables[current]?.[key] ?? en[key] ?? key;
  if (!vars) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, name: string) =>
    name in vars ? String(vars[name]) : `{${name}}`,
  );
}
