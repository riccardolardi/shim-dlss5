/**
 * Typed wrappers around every Tauri command and event. The only file that
 * imports `invoke`/`listen`, so the command surface is visible in one place.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo } from "@/lib/generated/AppInfo";
import type { Settings } from "@/lib/generated/Settings";
import type { Library } from "@/lib/generated/Library";
import type { ScanReport } from "@/lib/generated/ScanReport";
import type { ScanProgress } from "@/lib/generated/ScanProgress";
import type { PlannedChange } from "@/lib/generated/PlannedChange";
import type { ErrorDto } from "@/lib/generated/ErrorDto";

export const commands = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  getLibrary: () => invoke<Library>("get_library"),
  scanLibrary: () => invoke<ScanReport>("scan_library"),
  planPreview: (gameId: string) => invoke<PlannedChange[]>("plan_preview", { gameId }),
};

export const events = {
  /** Fires while `scan_library` runs. Returns the unsubscribe function. */
  onScanProgress: (handler: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan://progress", (e) => handler(e.payload)),
};

export function isErrorDto(e: unknown): e is ErrorDto {
  if (typeof e !== "object" || e === null) return false;
  const r = e as Record<string, unknown>;
  return (
    typeof r.code === "string" && typeof r.message === "string" && typeof r.detail === "string"
  );
}

/** Turn anything a command threw into something we can show. */
export function toErrorDto(e: unknown): ErrorDto {
  if (isErrorDto(e)) return e;
  const detail = e instanceof Error ? e.message : String(e);
  return { code: "unknown", message: "Something went wrong.", detail };
}
