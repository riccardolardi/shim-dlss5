// Handler behaviour that does not need a database: method, size, user agent,
// validation, and the database-unavailable path.
import { test } from "node:test";
import assert from "node:assert/strict";
import report, { clientIp } from "../api/report.js";
import games from "../api/games.js";
import { sample } from "./validate.test.js";

delete process.env.DATABASE_URL;

function call(handler, { method = "POST", headers = {}, body, query = {} } = {}) {
  return new Promise((resolve) => {
    const res = {
      statusCode: 200,
      headers: {},
      setHeader(k, v) { this.headers[k.toLowerCase()] = v; },
      status(c) { this.statusCode = c; return this; },
      json(o) { resolve({ status: this.statusCode, headers: this.headers, body: o }); },
      send(s) { resolve({ status: this.statusCode, headers: this.headers, body: s }); },
    };
    handler({ method, headers: { "user-agent": "shim-dlss5/0.1.4", ...headers }, body, query }, res);
  });
}

test("report: only POST from the app, with a valid body", async () => {
  assert.equal((await call(report, { method: "GET" })).status, 405);
  assert.equal((await call(report, { headers: { "user-agent": "curl/8" }, body: sample() })).status, 400);
  assert.equal((await call(report, { headers: { "content-length": "999999" }, body: sample() })).status, 413);
  const bad = await call(report, { body: { ...sample(), outcome: "x" } });
  assert.equal(bad.status, 400);
  assert.match(bad.body.error, /outcome/);
  assert.equal((await call(report, { body: "{not json" })).status, 400);
});

test("report: a valid report without a database is a 503, not a crash", async () => {
  const r = await call(report, { body: sample() });
  assert.equal(r.status, 503);
  assert.equal(r.headers["cache-control"], "no-store");
});

test("games: bad keys are 404, no database is a 503 page", async () => {
  const nf = await call(games, { method: "GET", query: { key: "../../etc" } });
  assert.equal(nf.status, 404);
  const down = await call(games, { method: "GET" });
  assert.equal(down.status, 503);
  assert.match(down.body, /unavailable/);
});

test("client ip comes from the first forwarded hop", () => {
  assert.equal(clientIp({ headers: { "x-forwarded-for": "203.0.113.7, 10.0.0.1" } }), "203.0.113.7");
  assert.equal(clientIp({ headers: { "x-real-ip": "198.51.100.2" } }), "198.51.100.2");
});
