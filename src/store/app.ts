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
import type { Game } from "@/lib/generated/Game";
import type { DiscoveryOutcome } from "@/lib/generated/DiscoveryOutcome";
import type { ErrorDto } from "@/lib/generated/ErrorDto";
import type { ComponentRow } from "@/lib/generated/ComponentRow";
import type { ComponentProgress } from "@/lib/generated/ComponentProgress";
import type { InstallProgress } from "@/lib/generated/InstallProgress";
import type { UpdateInfo } from "@/lib/generated/UpdateInfo";

export type Screen =
  | { kind: "library" }
  | { kind: "game"; id: string }
  | { kind: "components" }
  | { kind: "settings" };

/** Outcome of the last install/remove on a game, shown inline on its page. */
export type InstallOutcome =
  | { kind: "installed"; files: number }
  | { kind: "removed" }
  | { kind: "failed"; error: ErrorDto };

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

  components: ComponentRow[] | null;
  componentProgress: Record<string, ComponentProgress>;
  componentErrors: Record<string, ErrorDto>;

  /** Game id with an install/remove in flight. */
  installing: string | null;
  installProgress: InstallProgress | null;
  /** Keyed by game id; cleared when the user leaves the page. */
  installOutcome: Record<string, InstallOutcome>;

  /** A newer release, until dismissed. */
  updateInfo: UpdateInfo | null;

  go: (screen: Screen) => void;
  boot: () => Promise<void>;
  scan: () => Promise<void>;
  rescanGame: (gameId: string) => Promise<void>;
  openFolder: (gameId: string) => Promise<ErrorDto | null>;
  setHidden: (gameId: string, hidden: boolean) => Promise<ErrorDto | null>;
  dismissUpdate: () => void;
  relaunchElevated: () => Promise<ErrorDto | null>;
  updateSettings: (patch: Partial<Settings>) => Promise<ErrorDto | null>;
  loadComponents: () => Promise<void>;
  fetchComponent: (id: string) => Promise<void>;
  install: (gameId: string, confirmAntiCheat: boolean) => Promise<void>;
  remove: (gameId: string) => Promise<void>;
  /** Remove, then install again with the current component pins. */
  update: (gameId: string) => Promise<void>;
  clearOutcome: (gameId: string) => void;
}

const emptyLibrary: Library = { schema: 1, scanned_at: null, games: [] };

function replaceGame(library: Library, game: Game): Library {
  return { ...library, games: library.games.map((g) => (g.id === game.id ? game : g)) };
}

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
  components: null,
  componentProgress: {},
  componentErrors: {},
  installing: null,
  installProgress: null,
  installOutcome: {},
  updateInfo: null,

  go: (screen) => set({ screen }),

  boot: async () => {
    try {
      const [info, settings, library] = await Promise.all([
        commands.appInfo(),
        commands.getSettings(),
        commands.getLibrary(),
      ]);
      set({ info, settings, library, bootError: null });
      if (settings.check_updates && info.can_scan) {
        commands
          .checkUpdate()
          .then((updateInfo) => set({ updateInfo }))
          .catch(() => {});
      }
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
      // Covers arrive after the scan, quietly; the library already shows.
      commands
        .fetchCovers()
        .then((library) => set({ library }))
        .catch(() => {});
    } catch (e) {
      set({ scanError: toErrorDto(e) });
    } finally {
      unlisten?.();
      set({ scanning: false, scanProgress: null });
    }
  },

  rescanGame: async (gameId) => {
    try {
      const game = await commands.rescanGame(gameId);
      set((s) => ({ library: replaceGame(s.library, game) }));
    } catch (e) {
      set({ scanError: toErrorDto(e) });
    }
  },

  openFolder: async (gameId) => {
    try {
      await commands.openFolder(gameId);
      return null;
    } catch (e) {
      return toErrorDto(e);
    }
  },

  setHidden: async (gameId, hidden) => {
    const current = get().settings;
    if (!current) return null;
    const without = current.hidden_games.filter((id) => id !== gameId);
    const hidden_games = hidden ? [...without, gameId] : without;
    const error = await get().updateSettings({ hidden_games });
    if (!error) {
      set((s) => ({
        library: {
          ...s.library,
          games: s.library.games.map((g) => (g.id === gameId ? { ...g, hidden } : g)),
        },
      }));
    }
    return error;
  },

  dismissUpdate: () => set({ updateInfo: null }),

  relaunchElevated: async () => {
    try {
      await commands.relaunchElevated();
      return null;
    } catch (e) {
      return toErrorDto(e);
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

  loadComponents: async () => {
    try {
      set({ components: await commands.getComponents() });
    } catch {
      set({ components: [] });
    }
  },

  fetchComponent: async (id) => {
    const { componentErrors } = get();
    const { [id]: _dropped, ...otherErrors } = componentErrors;
    set({ componentErrors: otherErrors });
    const unlisten = await events
      .onComponentProgress((p) => {
        if (p.id === id) set((s) => ({ componentProgress: { ...s.componentProgress, [id]: p } }));
      })
      .catch(() => null);
    try {
      const row = await commands.fetchComponent(id);
      set((s) => ({
        components: (s.components ?? []).map((r) => (r.component.id === id ? row : r)),
      }));
    } catch (e) {
      set((s) => ({ componentErrors: { ...s.componentErrors, [id]: toErrorDto(e) } }));
    } finally {
      unlisten?.();
      set((s) => {
        const { [id]: _done, ...rest } = s.componentProgress;
        return { componentProgress: rest };
      });
    }
  },

  install: async (gameId, confirmAntiCheat) => {
    if (get().installing) return;
    set({ installing: gameId, installProgress: null });
    const unlisten = await events
      .onInstallProgress((p) => {
        if (p.game_id === gameId) set({ installProgress: p });
      })
      .catch(() => null);
    try {
      const game = await commands.installGame(gameId, confirmAntiCheat);
      const manifest = await commands.getInstall(gameId).catch(() => null);
      set((s) => ({
        library: replaceGame(s.library, game),
        installOutcome: {
          ...s.installOutcome,
          [gameId]: { kind: "installed", files: manifest?.files.length ?? 0 },
        },
      }));
    } catch (e) {
      set((s) => ({
        installOutcome: { ...s.installOutcome, [gameId]: { kind: "failed", error: toErrorDto(e) } },
      }));
    } finally {
      unlisten?.();
      set({ installing: null, installProgress: null });
    }
  },

  remove: async (gameId) => {
    if (get().installing) return;
    set({ installing: gameId, installProgress: null });
    try {
      const game = await commands.removeGame(gameId);
      set((s) => ({
        library: replaceGame(s.library, game),
        installOutcome: { ...s.installOutcome, [gameId]: { kind: "removed" } },
      }));
    } catch (e) {
      set((s) => ({
        installOutcome: { ...s.installOutcome, [gameId]: { kind: "failed", error: toErrorDto(e) } },
      }));
    } finally {
      set({ installing: null, installProgress: null });
    }
  },

  update: async (gameId) => {
    if (get().installing) return;
    const previous = await commands.getInstall(gameId).catch(() => null);
    await get().remove(gameId);
    if (get().installOutcome[gameId]?.kind === "failed") return;
    await get().install(gameId, previous?.anti_cheat_override ?? false);
  },

  clearOutcome: (gameId) =>
    set((s) => {
      const { [gameId]: _dropped, ...rest } = s.installOutcome;
      return { installOutcome: rest };
    }),
}));
