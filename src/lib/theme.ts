import type { Theme } from "@/lib/generated/Theme";

const query = () => window.matchMedia?.("(prefers-color-scheme: dark)");

export function resolveTheme(theme: Theme, systemDark: boolean): "light" | "dark" {
  if (theme === "system") return systemDark ? "dark" : "light";
  return theme;
}

export function applyTheme(theme: Theme) {
  const resolved = resolveTheme(theme, query()?.matches ?? false);
  document.documentElement.dataset.theme = resolved;
}

/** Re-apply when the OS theme flips. Returns an unsubscribe function. */
export function watchSystemTheme(onChange: () => void): () => void {
  const mq = query();
  if (!mq) return () => {};
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}
