import type { GameStatus } from "@/lib/generated/GameStatus";
import type { Route } from "@/lib/generated/Route";
import type { AntiCheat } from "@/lib/generated/AntiCheat";
import type { GraphicsApi } from "@/lib/generated/GraphicsApi";
import type { Bitness } from "@/lib/generated/Bitness";
import type { Engine } from "@/lib/generated/Engine";
import { t } from "@/i18n";

export const apiLabel: Record<GraphicsApi, string> = {
  dx9: "DX9",
  dx10: "DX10",
  dx11: "DX11",
  dx12: "DX12",
  vulkan: "Vulkan",
  open_gl: "OpenGL",
};

export const bitnessLabel: Record<Bitness, string> = {
  x86: "32-bit",
  x64: "64-bit",
};

export const engineLabel: Record<Engine, string | null> = {
  unreal: "Unreal",
  unity: "Unity",
  red_engine: "RED Engine",
  other: null,
};

export type Tone = "neutral" | "success" | "warning" | "danger";

export const routeLabel: Record<Route, string> = {
  optiscaler: "OptiScaler",
  reshade_renodx: "ReShade + RenoDX",
  reshade_vulkan: "ReShade (Vulkan)",
};

export const antiCheatLabel: Record<AntiCheat, string> = {
  easy_anti_cheat: "Easy Anti-Cheat",
  battl_eye: "BattlEye",
  vanguard: "Vanguard",
  ricochet: "Ricochet",
  game_guard: "GameGuard",
  xigncode: "XIGNCODE",
  javelin: "EA Javelin",
  ace: "Tencent ACE",
  mhyprot: "mhyprot",
  vac: "VAC",
  other: "anti-cheat",
};

/** The one line under a card title. */
export function statusLine(s: GameStatus): { text: string; tone: Tone } {
  switch (s.kind) {
    case "ready":
      return { text: t("status.ready"), tone: "neutral" };
    case "installed":
      return { text: `${t("status.installed")} · ${routeLabel[s.route]}`, tone: "success" };
    case "update_available":
      return { text: t("status.update"), tone: "warning" };
    case "anti_cheat":
      return { text: `${t("status.antiCheat")}: ${antiCheatLabel[s.which]}`, tone: "danger" };
    case "unsupported":
      return { text: `${t("status.unsupported")}: ${s.reason}`, tone: "neutral" };
    case "pending":
      return { text: t("status.pending"), tone: "neutral" };
  }
}

export type Filter = "all" | "installed" | "update" | "anti_cheat" | "unsupported";

export function matchesFilter(s: GameStatus, f: Filter): boolean {
  switch (f) {
    case "all":
      return true;
    case "installed":
      return s.kind === "installed" || s.kind === "update_available";
    case "update":
      return s.kind === "update_available";
    case "anti_cheat":
      return s.kind === "anti_cheat";
    case "unsupported":
      return s.kind === "unsupported";
  }
}
