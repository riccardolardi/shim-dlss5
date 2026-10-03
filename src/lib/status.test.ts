import { matchesFilter, statusLine } from "./status";

describe("statusLine", () => {
  it("names the route when installed", () => {
    const out = statusLine({ kind: "installed", route: "optiscaler" });
    expect(out.text).toBe("Installed · OptiScaler");
    expect(out.tone).toBe("success");
  });

  it("names the anti-cheat and uses the danger tone", () => {
    const out = statusLine({ kind: "anti_cheat", which: "easy_anti_cheat" });
    expect(out.text).toBe("Anti-cheat: Easy Anti-Cheat");
    expect(out.tone).toBe("danger");
  });

  it("shows the unsupported reason", () => {
    expect(statusLine({ kind: "unsupported", reason: "DirectX 9" }).text).toBe(
      "Unsupported: DirectX 9",
    );
  });
});

describe("matchesFilter", () => {
  it("counts update-available as installed", () => {
    expect(matchesFilter({ kind: "update_available", route: "optiscaler" }, "installed")).toBe(true);
    expect(matchesFilter({ kind: "ready", route: "optiscaler" }, "installed")).toBe(false);
  });

  it("all matches everything", () => {
    expect(matchesFilter({ kind: "pending" }, "all")).toBe(true);
  });
});
