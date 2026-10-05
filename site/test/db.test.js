// The real queries, run against PGlite (Postgres in WebAssembly).
import { test, before } from "node:test";
import assert from "node:assert/strict";
import { PGlite } from "@electric-sql/pglite";
import { accept, listGames, gameDetail, useSqlForTests, PER_HOUR } from "../lib/db.js";
import { validate } from "../lib/validate.js";
import { sample } from "./validate.test.js";

let pg;
before(async () => {
  pg = new PGlite();
  // Same shape as the Neon driver: a tagged template resolving to rows.
  useSqlForTests((strings, ...values) => pg.sql(strings, ...values).then((r) => r.rows));
});

const report = (patch) => validate({ ...sample(), ...patch }).value;

test("reports are stored, rate limited per game and per hour, and aggregated", async () => {
  assert.deepEqual(await accept(report(), "203.0.113.1"), { ok: true, key: "steam-2537590" });
  // Same network, same game, same day: refused.
  const dup = await accept(report({ outcome: "crashes" }), "203.0.113.1");
  assert.match(dup.limited, /already reported/);

  // One report is not enough to be listed.
  assert.equal((await listGames()).length, 0);
  assert.equal(await gameDetail("steam-2537590"), null);

  await accept(report({ outcome: "crashes", gpu: { name: "NVIDIA GeForce RTX 4090", vram_mb: 24564, driver: "616.56" } }), "198.51.100.2");
  await accept(report({ route: "reshade_renodx", before_upscale: null }), "198.51.100.3");

  const list = await listGames();
  assert.equal(list.length, 1);
  assert.equal(list[0].title, "Microsoft Flight Simulator 2024");
  assert.deepEqual([list[0].n, list[0].works, list[0].no_effect, list[0].crashes], [3, 2, 0, 1]);

  const d = await gameDetail("steam-2537590");
  assert.equal(d.summary.n, 3);
  assert.equal(d.setups.length, 2);
  assert.equal(d.gpus[0].gpu_name, "NVIDIA GeForce RTX 5070");
  assert.equal(d.recent.length, 3);
  assert.deepEqual(d.recent.at(-1).components, [["optiscaler-nr", "0.8.3"]]);
});

test("the hourly limit applies across games", async () => {
  const ip = "192.0.2.50";
  let limited = null;
  for (let i = 0; i <= PER_HOUR && !limited; i++) {
    const r = await accept(report({ launcher_id: String(9000 + i), title: `Game ${i}` }), ip);
    limited = r.limited ?? null;
  }
  assert.match(limited, /last hour/);
});

test("no IP address is stored anywhere", async () => {
  const dump = JSON.stringify([
    (await pg.query("select * from reports")).rows,
    (await pg.query("select * from throttle")).rows,
  ]);
  for (const ip of ["203.0.113.1", "198.51.100.2", "192.0.2.50"]) assert.ok(!dump.includes(ip), ip);
});
