import { setLanguage, t } from "./index";

describe("t", () => {
  it("returns the English string", () => {
    expect(t("nav.library")).toBe("Library");
  });

  it("substitutes variables and leaves unknown ones visible", () => {
    expect(t("settings.about.version", { version: "0.1.0" })).toBe("Version 0.1.0");
    expect(t("library.lastScan", {})).toBe("Last scan {time}");
  });

  it("falls back to English for an unknown language", () => {
    setLanguage("xx");
    expect(t("nav.settings")).toBe("Settings");
    setLanguage("en");
  });
});
