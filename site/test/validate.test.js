import { test } from "node:test";
import assert from "node:assert/strict";
import { validate, gameKey, GAME_KEY } from "../lib/validate.js";

export const sample = () => ({
  schema: 1,
  app_version: "0.1.4",
  launcher: "steam",
  launcher_id: "2537590",
  title: "Microsoft Flight Simulator 2024",
  route: "optiscaler_dlssnr",
  before_upscale: true,
  components: [["optiscaler-nr", "0.8.3"]],
  game_dlss: "310.6.0.0",
  gpu: { name: "NVIDIA GeForce RTX 5070", vram_mb: 12227, driver: "616.56" },
  model_signed: false,
  outcome: "works",
  log: ["[I] DlssNr_Dx12::Dispatch DLSS-NR running at 2560x1440"],
});

test("a report exactly as the app builds it is accepted", () => {
  const v = validate(sample());
  assert.equal(v.ok, true, v.error);
  assert.equal(v.value.gpu.vram_mb, 12227);
  assert.deepEqual(v.value.components, [["optiscaler-nr", "0.8.3"]]);
});

test("optional fields may be null or missing", () => {
  const r = { ...sample(), launcher_id: null, before_upscale: null, gpu: null, game_dlss: null, model_signed: null };
  delete r.log;
  const v = validate(r);
  assert.equal(v.ok, true, v.error);
  assert.deepEqual(v.value.log, []);
});

test("bad input is rejected with the field name", () => {
  const cases = [
    [{ schema: 2 }, /schema/],
    [{ route: "evil" }, /route/],
    [{ outcome: "great" }, /outcome/],
    [{ title: "x".repeat(121) }, /title/],
    [{ title: "   " }, /title/],
    [{ launcher_id: "../etc" }, /launcher_id/],
    [{ components: [["x"], ["y"]] }, /components/],
    [{ gpu: { name: "g", vram_mb: -1 } }, /gpu/],
    [{ log: ["a", "b", "c", "d", "e", "f"] }, /log/],
    [{ app_version: "latest" }, /app_version/],
  ];
  for (const [patch, re] of cases) {
    const v = validate({ ...sample(), ...patch });
    assert.equal(v.ok, false, JSON.stringify(patch));
    assert.match(v.error, re);
  }
  assert.equal(validate(null).ok, false);
  assert.equal(validate([]).ok, false);
});

test("control and bidi characters are stripped from text", () => {
  const v = validate({ ...sample(), title: "Game‮\u0007 Name" });
  assert.equal(v.value.title, "Game Name");
});

test("game keys are stable and URL-safe", () => {
  assert.equal(gameKey({ launcher: "steam", launcher_id: "2537590", title: "x" }), "steam-2537590");
  assert.equal(gameKey({ launcher: "custom", launcher_id: null, title: "Mein Spiel: Teil 2!" }), "custom-mein-spiel-teil-2");
  assert.equal(gameKey({ launcher: "ea", launcher_id: "{JEDI}", title: "x" }), "ea-JEDI");
  for (const k of ["steam-2537590", "custom-mein-spiel-teil-2"]) assert.match(k, GAME_KEY);
  assert.doesNotMatch("steam-../../x", GAME_KEY);
});
