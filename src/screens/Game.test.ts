import { describe, expect, it } from "vitest";
import { relative } from "./Game";

describe("relative", () => {
  it("strips the install folder, case-insensitively and across slash styles", () => {
    expect(relative("C:\\Games\\X", "C:\\Games\\X\\bin\\x64\\x.exe")).toBe("bin\\x64\\x.exe");
    expect(relative("c:/games/x/", "C:\\Games\\X\\x.exe")).toBe("x.exe");
  });

  it("leaves paths outside the folder untouched", () => {
    expect(relative("C:\\Games\\X", "D:\\Other\\x.exe")).toBe("D:\\Other\\x.exe");
    expect(relative("C:\\Games\\X", "C:\\Games\\XY\\x.exe")).toBe("C:\\Games\\XY\\x.exe");
  });
});
