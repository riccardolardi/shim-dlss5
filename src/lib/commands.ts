/**
 * Typed wrappers around every Tauri command and event. The only file that
 * imports `invoke`/`listen`, so the command surface is visible in one place.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppInfo } from "@/lib/generated/AppInfo";
import type { Settings } from "@/lib/generated/Settings";
import type { Library } from "@/lib/generated/Library";
import type { Game } from "@/lib/generated/Game";
import type { InstallMode } from "@/lib/generated/InstallMode";
import type { NeuralOptions } from "@/lib/generated/NeuralOptions";
import type { LastRun } from "@/lib/generated/LastRun";
import type { Outcome } from "@/lib/generated/Outcome";
import type { Report } from "@/lib/generated/Report";
import type { ScanReport } from "@/lib/generated/ScanReport";
import type { ScanProgress } from "@/lib/generated/ScanProgress";
import type { Preview } from "@/lib/generated/Preview";
import type { InstallManifest } from "@/lib/generated/InstallManifest";
import type { ComponentRow } from "@/lib/generated/ComponentRow";
import type { ComponentProgress } from "@/lib/generated/ComponentProgress";
import type { InstallProgress } from "@/lib/generated/InstallProgress";
import type { CoverReady } from "@/lib/generated/CoverReady";
import type { FileInfo } from "@/lib/generated/FileInfo";
import type { UpdateInfo } from "@/lib/generated/UpdateInfo";
import type { ErrorDto } from "@/lib/generated/ErrorDto";

export const commands = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  getLibrary: () => invoke<Library>("get_library"),
  scanLibrary: () => invoke<ScanReport>("scan_library"),
  rescanGame: (gameId: string) => invoke<Game>("rescan_game", { gameId }),
  setGameMode: (gameId: string, mode: InstallMode | null) =>
    invoke<Game>("set_game_mode", { gameId, mode }),
  setNeuralOptions: (gameId: string, options: NeuralOptions | null) =>
    invoke<Game>("set_neural_options", { gameId, options }),
  lastRun: (gameId: string) => invoke<LastRun | null>("last_run", { gameId }),
  reportPreview: (gameId: string, outcome: Outcome | null) =>
    invoke<Report>("report_preview", { gameId, outcome }),
  submitReport: (gameId: string, outcome: Outcome) =>
    invoke<void>("submit_report", { gameId, outcome }),
  openFolder: (gameId: string) => invoke<void>("open_folder", { gameId }),
  fetchCovers: () => invoke<Library>("fetch_covers"),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  relaunchElevated: () => invoke<void>("relaunch_elevated"),
  planPreview: (gameId: string) => invoke<Preview>("plan_preview", { gameId }),
  getInstall: (gameId: string) => invoke<InstallManifest | null>("get_install", { gameId }),
  getComponents: () => invoke<ComponentRow[]>("get_components"),
  fetchComponent: (id: string) => invoke<ComponentRow>("fetch_component", { id }),
  inspectFile: (path: string) => invoke<FileInfo>("inspect_file", { path }),
  installGame: (gameId: string, confirmAntiCheat: boolean) =>
    invoke<Game>("install_game", { gameId, confirmAntiCheat }),
  removeGame: (gameId: string) => invoke<Game>("remove_game", { gameId }),
};

export const events = {
  /** Fires while `scan_library` runs. Returns the unsubscribe function. */
  onScanProgress: (handler: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan://progress", (e) => handler(e.payload)),
  onComponentProgress: (handler: (p: ComponentProgress) => void): Promise<UnlistenFn> =>
    listen<ComponentProgress>("component://progress", (e) => handler(e.payload)),
  onInstallProgress: (handler: (p: InstallProgress) => void): Promise<UnlistenFn> =>
    listen<InstallProgress>("install://progress", (e) => handler(e.payload)),
  onCoverReady: (handler: (p: CoverReady) => void): Promise<UnlistenFn> =>
    listen<CoverReady>("cover://ready", (e) => handler(e.payload)),
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
