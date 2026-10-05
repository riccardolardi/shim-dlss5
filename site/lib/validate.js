// Validation of a result report sent by the shim-dlss5 app
// (crates/core/src/report.rs). Everything is checked and bounded here;
// anything unexpected is rejected rather than cleaned up.

export const LAUNCHERS = ["steam", "epic", "gog", "xbox", "ubisoft", "ea", "custom"];
export const ROUTES = ["reshade_renodx", "reshade_feeder", "optiscaler_dlssnr", "optiscaler", "reshade_vulkan"];
export const OUTCOMES = ["works", "no_effect", "crashes"];

// Printable text only: no control characters, no bidi overrides.
const CLEAN = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩]/g;

function text(v, max, { required = false, pattern = null } = {}) {
  if (v === null || v === undefined) {
    if (required) throw new Error("missing field");
    return null;
  }
  if (typeof v !== "string") throw new Error("not a string");
  const s = v.replace(CLEAN, "").trim();
  if (required && !s) throw new Error("empty field");
  if (s.length > max) throw new Error(`longer than ${max}`);
  if (pattern && s && !pattern.test(s)) throw new Error("unexpected format");
  return s || null;
}

function oneOf(v, list) {
  if (!list.includes(v)) throw new Error(`not one of ${list.join(", ")}`);
  return v;
}

function bool(v) {
  if (v === null || v === undefined) return null;
  if (typeof v !== "boolean") throw new Error("not a boolean");
  return v;
}

function field(name, fn) {
  try {
    return fn();
  } catch (e) {
    throw new Error(`${name}: ${e.message}`);
  }
}

/** Returns `{ ok: true, value }` or `{ ok: false, error }`. */
export function validate(body) {
  try {
    if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("body must be a JSON object");
    if (body.schema !== 1) throw new Error("schema: unsupported");
    const value = {
      app_version: field("app_version", () => text(body.app_version, 20, { required: true, pattern: /^\d+\.\d+\.\d+([-.+][\w.]+)?$/ })),
      launcher: field("launcher", () => oneOf(body.launcher, LAUNCHERS)),
      launcher_id: field("launcher_id", () => text(body.launcher_id, 64, { pattern: /^[A-Za-z0-9._{}-]+$/ })),
      title: field("title", () => text(body.title, 120, { required: true })),
      route: field("route", () => oneOf(body.route, ROUTES)),
      before_upscale: field("before_upscale", () => bool(body.before_upscale)),
      components: field("components", () => {
        if (!Array.isArray(body.components) || body.components.length > 6) throw new Error("expected up to 6 entries");
        return body.components.map((c) => {
          if (!Array.isArray(c) || c.length !== 2) throw new Error("expected [id, version]");
          return [text(c[0], 40, { required: true, pattern: /^[a-z0-9-]+$/ }), text(c[1], 24, { required: true, pattern: /^[\w.+-]+$/ })];
        });
      }),
      game_dlss: field("game_dlss", () => text(body.game_dlss, 24, { pattern: /^[\d.]+$/ })),
      gpu: field("gpu", () => {
        const g = body.gpu;
        if (g === null || g === undefined) return null;
        if (typeof g !== "object" || Array.isArray(g)) throw new Error("not an object");
        const vram = g.vram_mb;
        if (vram !== null && vram !== undefined && !(Number.isInteger(vram) && vram >= 0 && vram <= 262144)) throw new Error("vram_mb out of range");
        return {
          name: text(g.name, 80, { required: true }),
          vram_mb: vram ?? null,
          driver: text(g.driver, 24, { pattern: /^[\w.]+$/ }),
        };
      }),
      model_signed: field("model_signed", () => bool(body.model_signed)),
      outcome: field("outcome", () => oneOf(body.outcome, OUTCOMES)),
      log: field("log", () => {
        const l = body.log ?? [];
        if (!Array.isArray(l) || l.length > 5) throw new Error("expected up to 5 lines");
        return l.map((s) => text(s, 220, { required: true }));
      }),
    };
    return { ok: true, value };
  } catch (e) {
    return { ok: false, error: e.message };
  }
}

/** Stable URL-safe key for a game: `steam-2537590`, or a slug of the title. */
export function gameKey(r) {
  const id = r.launcher_id
    ? r.launcher_id.replace(/[^A-Za-z0-9._-]/g, "").slice(0, 64)
    : r.title.toLowerCase().normalize("NFKD").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 64);
  return `${r.launcher}-${id || "unknown"}`;
}

export const GAME_KEY = /^[a-z]+-[A-Za-z0-9._-]{1,64}$/;
