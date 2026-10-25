/**
 * What to tell the user about their DLSS 5 model and GPU. The judgement
 * itself (`fit`) is made in Rust (`shim_core::model_fit`); this only picks
 * the words and decides where a note is worth showing.
 */
import type { ModelStatus } from "@/lib/generated/ModelStatus";
import type { Route } from "@/lib/generated/Route";
import type { Tone } from "@/lib/status";
import { t } from "@/i18n";

export type NotePlace = "components" | "game";

export interface ModelNote {
  text: string;
  tone: Tone;
}

/** Routes that copy the user's model next to the game. */
export function routeUsesModel(route: Route): boolean {
  return route !== "optiscaler" && route !== "reshade_vulkan";
}

/** Routes where the RenoDX add-on runs the model (ReShade hosts it). */
export function routeUsesRenoDx(route: Route): boolean {
  return route === "reshade_renodx" || route === "reshade_feeder";
}

/**
 * A newer NVIDIA build than the add-ons were made for (310.9.16 vs 310.8).
 * The OptiScaler fork runs it; RenoDX add-on v4.7 crashed with it. On a game
 * page it is only worth saying where the route runs RenoDX.
 */
export function newerBuildNote(status: ModelStatus | null, place: NotePlace, route?: Route): ModelNote | null {
  if (status?.build !== "nvidia_newer") return null;
  if (place === "components") return { text: t("model.newer.components"), tone: "warning" };
  return route && routeUsesRenoDx(route) ? { text: t("model.newer.game"), tone: "warning" } : null;
}

export function modelNote(status: ModelStatus | null, place: NotePlace): ModelNote | null {
  if (!status?.fit) return null;
  const gpu = status.gpu?.name ?? t("model.yourGpu");
  switch (status.fit) {
    case "fits":
      // Good news belongs on the Components screen, not on every game page.
      return place === "components" ? { text: t("model.fit.fits", { gpu }), tone: "success" } : null;
    case "needs_compat_build":
      return { text: t("model.fit.needsCompat", { gpu }), tone: "warning" };
    case "unknown_build":
      return place === "components" ? { text: t("model.fit.unknownBuild", { gpu }), tone: "neutral" } : null;
    case "no_nvidia_gpu":
      return { text: t("model.fit.noNvidia", { gpu }), tone: "danger" };
  }
}
