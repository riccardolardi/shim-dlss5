/**
 * App state. One store, small, all mutations go through actions that replace
 * state with new objects.
 */
import { create } from "zustand";
import { commands, events, toErrorDto } from "@/lib/commands";
import type { AppInfo } from "@/lib/generated/AppInfo";
import type { ScanProgress } from "@/lib/generated/ScanProgress";
import type { Settings } from "@/lib/generated/Settings";
import type { Library } from "@/lib/generated/Library";
import type { DiscoveryOutcome } from "@/lib/generated/DiscoveryOutcome";
import type { ErrorDto } from "@/lib/generated/ErrorDto";

export type Screen =
  | { kind: "library" }
  | { kind: "game"; id: string }
  | { kind: "components" }
  | { kind: "settings" };

interface AppState {
  screen: Screen;
  info: AppInfo | null;
  settings: Settings | null;
  library: Library;
  scanning: boolean;
  scanProgress: ScanProgress | null;
  lastScan: DiscoveryOutcome | null;
  scanError: ErrorDto | null;
  bootError: ErrorDto | null;

  go: (screen: Screen) => void;
  boot: () => Promise<void>;
  scan: () => Promise<void>;
  updateSettings: (patch: Partial<Settings>) => Promise<ErrorDto | null>;
}

const emptyLibrary: Library = { schema: 1, scanned_at: null, games: [] };

export const useApp = create<AppState>((set, get) => ({
  screen: { kind: "library" },
  info: null,
  settings: null,
  library: emptyLibrary,
  scanning: false,
  scanProgress: null,
  lastScan: null,
  scanError: null,
  bootError: null,

  go: (screen) => set({ screen }),

  boot: async () => {
    try {
      const [info, settings, library] = await Promise.all([
        commands.appInfo(),
        commands.getSettings(),
        commands.getLibrary(),
      ]);
      set({ info, settings, library, bootError: null });
    } catch (e) {
      set({ bootError: toErrorDto(e) });
    }
  },

  scan: async () => {
    if (get().scanning) return;
    set({ scanning: true, scanError: null, scanProgress: null });
    const unlisten = await events
      .onScanProgress((scanProgress) => set({ scanProgress }))
      .catch(() => null);
    try {
      const report = await commands.scanLibrary();
      set({ library: report.library, lastScan: report.outcome });
    } catch (e) {
      set({ scanError: toErrorDto(e) });
    } finally {
      unlisten?.();
      set({ scanning: false, scanProgress: null });
    }
  },

  updateSettings: async (patch) => {
    const current = get().settings;
    if (!current) return null;
    const next: Settings = { ...current, ...patch };
    set({ settings: next });
    try {
      const saved = await commands.saveSettings(next);
      // Only accept the server copy if no newer edit happened meanwhile.
      if (get().settings === next) set({ settings: saved });
      return null;
    } catch (e) {
      if (get().settings === next) set({ settings: current });
      return toErrorDto(e);
    }
  },
}));
