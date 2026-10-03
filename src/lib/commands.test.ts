import { isErrorDto, toErrorDto } from "./commands";

describe("toErrorDto", () => {
  it("passes a real ErrorDto through", () => {
    const dto = { code: "io", message: "Could not access X.", detail: "x" };
    expect(isErrorDto(dto)).toBe(true);
    expect(toErrorDto(dto)).toBe(dto);
  });

  it("wraps a thrown Error", () => {
    const out = toErrorDto(new Error("kaput"));
    expect(out.code).toBe("unknown");
    expect(out.detail).toBe("kaput");
  });

  it("wraps a plain string", () => {
    expect(toErrorDto("nope").detail).toBe("nope");
  });
});
