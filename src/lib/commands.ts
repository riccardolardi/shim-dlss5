/**
 * Typed wrappers around every Tauri command. The only file that imports
 * `invoke`, so the command surface is visible in one place.
 */
import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "@/lib/generated/AppInfo";
import type { Settings } from "@/lib/generated/Settings";
import type { Library } from "@/lib/generated/Library";
import type { ScanReport } from "@/lib/generated/ScanReport";
import type { ErrorDto } from "@/lib/generated/ErrorDto";

export const commands = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  getLibrary: () => invoke<Library>("get_library"),
  scanLibrary: () => invoke<ScanReport>("scan_library"),
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
