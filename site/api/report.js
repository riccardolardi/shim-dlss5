// POST /api/report — one anonymous result report from the app.
import { validate } from "../lib/validate.js";
import { accept } from "../lib/db.js";

const MAX_BYTES = 16 * 1024;

export function clientIp(req) {
  const fwd = req.headers["x-forwarded-for"];
  if (typeof fwd === "string" && fwd) return fwd.split(",")[0].trim();
  return req.headers["x-real-ip"] || req.socket?.remoteAddress || "unknown";
}

export default async function handler(req, res) {
  res.setHeader("Cache-Control", "no-store");
  if (req.method !== "POST") {
    res.setHeader("Allow", "POST");
    return res.status(405).json({ error: "POST a report from the shim-dlss5 app." });
  }
  if (Number(req.headers["content-length"] || 0) > MAX_BYTES) {
    return res.status(413).json({ error: "Report too large." });
  }
  if (!String(req.headers["user-agent"] || "").startsWith("shim-dlss5/")) {
    return res.status(400).json({ error: "Reports are sent from the shim-dlss5 app." });
  }
  let body = req.body;
  if (typeof body === "string") {
    try {
      body = JSON.parse(body);
    } catch {
      return res.status(400).json({ error: "Body is not JSON." });
    }
  }
  const v = validate(body);
  if (!v.ok) return res.status(400).json({ error: v.error });
  try {
    const result = await accept(v.value, clientIp(req));
    if (result.limited) return res.status(429).json({ error: result.limited });
    return res.status(201).json({ ok: true, game: result.key });
  } catch (e) {
    console.error("report failed", e);
    return res.status(503).json({ error: "Could not store the report. Try again later." });
  }
}
