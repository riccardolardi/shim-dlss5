// Neon Postgres access. DATABASE_URL comes from the Vercel ↔ Neon integration.
import { createHmac } from "node:crypto";
import { neon } from "@neondatabase/serverless";
import { gameKey } from "./validate.js";

/** Reports from one IP per hour, and per game per IP per day. */
export const PER_HOUR = 10;
export const PER_GAME_PER_DAY = 1;
/** A game is listed once this many reports exist. */
export const MIN_REPORTS = 2;

let sqlClient = null;
let schemaReady = null;

function sql() {
  if (sqlClient) return sqlClient;
  if (!process.env.DATABASE_URL) throw new Error("DATABASE_URL is not set");
  sqlClient = neon(process.env.DATABASE_URL);
  return sqlClient;
}

/** Tests only: run the same queries against another Postgres (PGlite). */
export function useSqlForTests(tag) {
  sqlClient = tag;
  schemaReady = null;
}

async function ensureSchema() {
  schemaReady ??= (async () => {
    const q = sql();
    await q`create table if not exists reports (
      id bigserial primary key,
      created_at timestamptz not null default now(),
      game_key text not null,
      launcher text not null,
      launcher_id text,
      title text not null,
      route text not null,
      before_upscale boolean,
      components jsonb not null,
      game_dlss text,
      gpu_name text,
      gpu_vram_mb integer,
      gpu_driver text,
      model_signed boolean,
      outcome text not null,
      log jsonb not null,
      app_version text not null
    )`;
    await q`create index if not exists reports_game_key on reports (game_key)`;
    // Rate-limit counters keyed by an HMAC of the IP with a daily salt.
    // Rows older than a day are deleted; the IP itself is never stored.
    await q`create table if not exists throttle (
      key text primary key,
      n integer not null,
      window_start timestamptz not null
    )`;
  })();
  try {
    await schemaReady;
  } catch (e) {
    schemaReady = null;
    throw e;
  }
}

/** HMAC of the IP and today's date: unlinkable across days, never reversible. */
export function ipKey(ip, scope, now = new Date()) {
  const secret = process.env.REPORT_SALT || process.env.DATABASE_URL || "";
  const day = now.toISOString().slice(0, 10);
  return createHmac("sha256", secret).update(`${day}|${scope}|${ip}`).digest("hex");
}

async function bump(key, windowSql) {
  const q = sql();
  const rows =
    windowSql === "hour"
      ? await q`insert into throttle (key, n, window_start) values (${key}, 1, now())
          on conflict (key) do update set
            n = case when throttle.window_start < now() - interval '1 hour' then 1 else throttle.n + 1 end,
            window_start = case when throttle.window_start < now() - interval '1 hour' then now() else throttle.window_start end
          returning n`
      : await q`insert into throttle (key, n, window_start) values (${key}, 1, now())
          on conflict (key) do update set
            n = case when throttle.window_start < now() - interval '1 day' then 1 else throttle.n + 1 end,
            window_start = case when throttle.window_start < now() - interval '1 day' then now() else throttle.window_start end
          returning n`;
  return rows[0].n;
}

/** Store one validated report. Returns `{ ok }` or `{ limited: message }`. */
export async function accept(r, ip) {
  await ensureSchema();
  const q = sql();
  const key = gameKey(r);
  if (Math.random() < 0.05) await q`delete from throttle where window_start < now() - interval '1 day'`;
  if ((await bump(ipKey(ip, "hour"), "hour")) > PER_HOUR) return { limited: "Too many reports from this network in the last hour." };
  if ((await bump(ipKey(ip, `game:${key}`), "day")) > PER_GAME_PER_DAY) return { limited: "This game was already reported from this network today." };
  await q`insert into reports (game_key, launcher, launcher_id, title, route, before_upscale, components,
      game_dlss, gpu_name, gpu_vram_mb, gpu_driver, model_signed, outcome, log, app_version)
    values (${key}, ${r.launcher}, ${r.launcher_id}, ${r.title}, ${r.route}, ${r.before_upscale},
      ${JSON.stringify(r.components)}, ${r.game_dlss}, ${r.gpu?.name ?? null}, ${r.gpu?.vram_mb ?? null},
      ${r.gpu?.driver ?? null}, ${r.model_signed}, ${r.outcome}, ${JSON.stringify(r.log)}, ${r.app_version})`;
  return { ok: true, key };
}

export async function listGames() {
  await ensureSchema();
  return sql()`select game_key,
      mode() within group (order by title) as title,
      count(*)::int as n,
      (count(*) filter (where outcome = 'works'))::int as works,
      (count(*) filter (where outcome = 'no_effect'))::int as no_effect,
      (count(*) filter (where outcome = 'crashes'))::int as crashes,
      max(created_at) as last
    from reports
    group by game_key
    having count(*) >= ${MIN_REPORTS}
    order by count(*) desc, max(created_at) desc
    limit 500`;
}

export async function gameDetail(key) {
  await ensureSchema();
  const q = sql();
  const [summary] = await q`select game_key,
      mode() within group (order by title) as title,
      count(*)::int as n,
      (count(*) filter (where outcome = 'works'))::int as works,
      (count(*) filter (where outcome = 'no_effect'))::int as no_effect,
      (count(*) filter (where outcome = 'crashes'))::int as crashes,
      max(created_at) as last
    from reports where game_key = ${key} group by game_key`;
  if (!summary || summary.n < MIN_REPORTS) return null;
  const setups = await q`select route, before_upscale,
      count(*)::int as n,
      (count(*) filter (where outcome = 'works'))::int as works,
      (count(*) filter (where outcome = 'no_effect'))::int as no_effect,
      (count(*) filter (where outcome = 'crashes'))::int as crashes
    from reports where game_key = ${key}
    group by route, before_upscale order by count(*) desc`;
  const gpus = await q`select coalesce(gpu_name, 'Unknown GPU') as gpu_name,
      count(*)::int as n,
      (count(*) filter (where outcome = 'works'))::int as works,
      (count(*) filter (where outcome = 'no_effect'))::int as no_effect,
      (count(*) filter (where outcome = 'crashes'))::int as crashes
    from reports where game_key = ${key}
    group by 1 order by count(*) desc limit 20`;
  const recent = await q`select created_at, route, before_upscale, gpu_name, gpu_vram_mb, gpu_driver,
      game_dlss, components, model_signed, outcome, app_version
    from reports where game_key = ${key}
    order by created_at desc limit 30`;
  return { summary, setups, gpus, recent };
}
