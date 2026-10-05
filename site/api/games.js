// GET /games and /games/<key> (rewritten here by vercel.json). Server-rendered
// HTML, cached at the edge for ten minutes.
import { listGames, gameDetail } from "../lib/db.js";
import { renderList, renderDetail, renderNotFound, renderError } from "../lib/render.js";
import { GAME_KEY } from "../lib/validate.js";

const CACHE = "public, s-maxage=600, stale-while-revalidate=3600";

export default async function handler(req, res) {
  res.setHeader("Content-Type", "text/html; charset=utf-8");
  const key = typeof req.query?.key === "string" ? req.query.key : null;
  try {
    if (!key) {
      res.setHeader("Cache-Control", CACHE);
      return res.status(200).send(renderList(await listGames()));
    }
    if (!GAME_KEY.test(key)) {
      return res.status(404).send(renderNotFound());
    }
    const detail = await gameDetail(key);
    res.setHeader("Cache-Control", CACHE);
    if (!detail) return res.status(404).send(renderNotFound());
    return res.status(200).send(renderDetail(detail));
  } catch (e) {
    console.error("games page failed", e);
    res.setHeader("Cache-Control", "no-store");
    return res.status(503).send(renderError());
  }
}
