import { modelNote, routeUsesModel } from "./modelNote";
import type { ModelStatus } from "@/lib/generated/ModelStatus";

const status = (fit: ModelStatus["fit"], name = "NVIDIA GeForce RTX 4070"): ModelStatus => ({
  gpu: { name, vram_mb: 12288, driver: "616.56" },
  family: "rtx40",
  build: "nvidia_signed",
  fit,
});

describe("modelNote", () => {
  it("says nothing without a verdict", () => {
    expect(modelNote(null, "components")).toBeNull();
    expect(modelNote(status(null), "game")).toBeNull();
  });

  it("warns RTX 20/30/40 users on both screens and names their card", () => {
    for (const place of ["components", "game"] as const) {
      const note = modelNote(status("needs_compat_build"), place);
      expect(note?.tone).toBe("warning");
      expect(note?.text).toContain("RTX 4070");
      expect(note?.text).toContain("RTX 50");
      // States the fact about the user's own file; never points at a modified build.
      expect(note?.text).not.toMatch(/E67DEE20|compatib|community/i);
    }
  });

  it("confirms a good pair only on the Components screen", () => {
    expect(modelNote(status("fits", "NVIDIA GeForce RTX 5070"), "components")?.tone).toBe("success");
    expect(modelNote(status("fits"), "game")).toBeNull();
  });

  it("keeps an unknown build neutral and off the game page", () => {
    expect(modelNote(status("unknown_build"), "components")?.tone).toBe("neutral");
    expect(modelNote(status("unknown_build"), "game")).toBeNull();
  });

  it("flags a non-NVIDIA GPU everywhere", () => {
    const note = modelNote(status("no_nvidia_gpu", "AMD Radeon RX 9070 XT"), "game");
    expect(note?.tone).toBe("danger");
    expect(note?.text).toContain("AMD Radeon RX 9070 XT");
  });
});

describe("routeUsesModel", () => {
  it("is false only for plain OptiScaler and Vulkan", () => {
    expect(routeUsesModel("optiscaler")).toBe(false);
    expect(routeUsesModel("reshade_vulkan")).toBe(false);
    expect(routeUsesModel("optiscaler_dlssnr")).toBe(true);
    expect(routeUsesModel("reshade_renodx")).toBe(true);
    expect(routeUsesModel("reshade_feeder")).toBe(true);
  });
});
