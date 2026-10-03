# shim — Plan

A small, honest DLSS 5 injector for Windows. It scans your game library, tells you
what it will do to a game, does it, and can undo it. Nothing else.

Name: **shim** (repository `shim-dlss5`). A shim is exactly what it installs: a thin
proxy DLL beside the game, plus files next to it, all of which can be taken out again.

---

## 1. Goal and principles

**One job:** put the DLSS 5 neural-rendering pass into a game, keep it current, and
take it back out cleanly.

Principles, in priority order:

1. **Reversible.** Every file we write is recorded; every file we overwrite is backed up.
   Uninstall restores the game byte-for-byte to what it was.
2. **Transparent.** Before anything happens the user can read the exact file plan.
   No silent failures, no hidden background work.
3. **Bundle nothing.** No NVIDIA binaries, no third-party DLLs in our repo or installer.
   Components are fetched from their publishers at run time, pinned by version and
   SHA-256. The DLSS 5 model is user-supplied.
4. **Simple by default, complete under "Advanced".** Auto-routing picks the route;
   the user sees one sentence and one button.
5. **Small.** Installer under 15 MB. No .NET runtime, no Electron, no obfuscation.

---

## 2. What we deliberately leave out

Everything below exists in DLSS 5 MANAGER and is **out of scope**, permanently
unless stated otherwise:

| Dropped | Why |
|---|---|
| Live Flow / NeuralScreen (desktop post-processing, frame gen, HDR, Spout, recording) | Not injection. Depends on spoofing GPU generation and renaming a worker to `nvngx.dll` to pass NVIDIA's caller check. Legal and anti-cheat risk, separate C++ toolchain. |
| Community reports, chat, DMs, gallery, developer badges | Needs a private server, moderation, and a persistent device fingerprint. Not our job. |
| Device fingerprinting, HMAC-signed API, telemetry of any kind | No server, no identity. |
| Cloud asset bucket (`r2.dev`) with unverified DLL downloads | Replaced by a signed, pinned component manifest (§6). |
| Bundled 560 MB `mod files` payload | Replaced by run-time fetch from publishers. |
| Obfuscation (Obfuscar), unsigned release | Open source, signed release with published checksums. |
| 9 photo backgrounds, glass/glare system, luminance compensation, custom background import | One light and one dark theme from tokens. |
| Two navigation layouts (top bar / sidebar) | One layout. |
| UI sounds | None. |
| Ko-fi nag modal, YouTube links in Settings | A single About page with links. |
| Overlay add-on, overlay themes, overlay hotkeys | The in-game panels of ReShade / OptiScaler / RenoDX already exist. |
| "Dynamic mods" payload replace/restore, "extra dynamic" user files per route | Advanced users can place files by hand; we don't manage arbitrary extras. |
| Emulator catalogue and emulator tab | Phase 4 at the earliest, only if asked for. |
| AMD / Intel route | Phase 4, only with a redistributable upstream. Not in v1. |
| 17-language localisation | English first. i18n-ready string table from day one, translations later. |
| Drag-to-reorder cards | Sort and filter instead. |

---

## 3. Legal and safety constraints

These shape the architecture and are non-negotiable.

- **DLSS 5 is an unreleased NVIDIA build.** We never download, host, mirror, or ship
  `nvngx_dlssnr.dll` or any NVIDIA runtime. The user points us at a file they already
  have. We record its SHA-256 and Authenticode status and show both.
- **No spoofing.** We never patch driver functions, fake GPU architecture, or rename
  our own binaries to NVIDIA names.
- **Respect upstream licences.** Only components with a licence that allows automatic
  download from the publisher's own release page are in the manifest. Nothing with
  "all rights reserved" or AGNYA terms. See §6.
- **Anti-cheat is a hard stop.** A game with EAC, BattlEye, Vanguard, Ricochet,
  GameGuard, nProtect, XIGNCODE, Javelin, ACE, or mhyprot markers shows a blocking
  dialog with the ban warning. Install proceeds only after an explicit typed
  confirmation and is flagged in the manifest.
- **No elevation by default.** `asInvoker`. If the game folder is not writable we say
  so and offer a one-time elevated relaunch for that action only.
- **Uninstall never deletes by filename.** Only paths listed in our manifest for that
  game are touched. A user's own ReShade or OptiScaler stays.
- **Signed releases with `SHA256SUMS.txt`.** Code-sign the installer when a certificate
  is available; publish hashes regardless.

---

## 4. User experience

### 4.1 Screens

Three screens plus Settings, reached from a left rail. No modals except confirmations.

**Library**
- Top: search, filter chips `All · Installed · Update available · Anti-cheat · Unsupported`,
  Scan button with last-scan time.
- Grid of cover cards. Under each title one plain status line:
  `Installed · OptiScaler` / `Ready` / `Anti-cheat: EAC` / `Update available` /
  `Unsupported: DirectX 9`.
- Right-click: Open folder · Rescan · Change cover · Hide from library.
- Empty state: "No games found. Add a folder or run a scan."

**Game**
- Cover hero, title, badges: `DirectX 12 · 64-bit · Steam · DLSS 3.7 present · EAC`.
- One sentence: what Install will do and why.
  Example: *"This game ships DLSS, so OptiScaler will load as `dxgi.dll` with your
  DLSS 5 model beside the executable."*
- One primary button: **Install** / **Update** / **Remove**. Secondary: Play, Open folder.
- Link: **What will change** → a list of every file to be written, backed up, or
  removed. Nothing executes from this view.
- **Advanced** (collapsed): route override, add-on toggles with one-line descriptions
  in plain English, proxy DLL name, in-game menu hotkeys.
- During install: inline progress with the current step. Afterwards: a result line
  and a collapsible log. Failures show the error and that a full rollback happened.
- Remove asks for confirmation and shows which backups will be restored.

**Components**
- One row per component (§6): name, pinned version, publisher URL, local hash status
  (`verified` / `missing` / `mismatch`), Update button.
- The DLSS 5 model row: file picker, SHA-256, signature status, "where did this come
  from" note. Never auto-fetched.

**Settings**
- Scan sources: toggle per launcher, custom folders, hidden games list.
- Theme: System / Light / Dark. Language.
- Check for updates on launch (on/off). Update opens the GitHub release page only.
- About: version, licences, link to repo.

### 4.2 Core flow

1. Launch → cached library shows at once; a scan runs quietly in the background with
   a progress pill. No countdowns, no modals.
2. Click a game.
3. Read the sentence. Click **Install**.
4. Done. Status line on the card updates.

Three clicks. Anti-cheat games add one confirmation.

### 4.3 Design system

- Tokens only: colour, spacing, radius, type scale defined once; light and dark themes.
- Font: Inter. Type scale: 12 / 14 / 16 / 20 / 28.
- One accent colour. Semantic colours for success, warning, danger.
- Two button styles (primary, secondary) plus a danger variant.
- Sentence case everywhere. No ALL-CAPS labels, no joke names.
- Density similar to DLSS5-Swapper's home and DLSS5-Autopilot's library, with
  stronger status visibility than either.

---

## 5. Domain: detection and routing

### 5.1 Library discovery

Each launcher is an adapter behind one trait returning `Vec<DiscoveredGame>`:

| Adapter | Source |
|---|---|
| Steam | `HKCU\Software\Valve\Steam\SteamPath` → `libraryfolders.vdf` → `appmanifest_*.acf` (proper VDF parser, check `StateFlags` = fully installed) |
| Epic | `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item` |
| GOG | `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\*` |
| Xbox / MS Store | `Get-AppxPackage`-equivalent via registry + `appxmanifest.xml` in `WindowsApps` (read-only, best effort) |
| Ubisoft Connect | `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\*` |
| EA app | `HKLM\SOFTWARE\WOW6432Node\Electronic Arts\EA Desktop` + install dir manifests |
| Battle.net | `%ProgramData%\Battle.net\Agent\product.db` (protobuf) — phase 3 |
| Heroic | `~/.config/heroic` json — phase 3 |
| Custom folders | Each subfolder with a plausible game exe |

No full-drive scanning. No "repack" folder heuristics. No artificial sleeps.
Artwork (Steam CDN `library_600x900`, exe icon fallback) is fetched **after** the scan,
asynchronously, with a local cache.

Game identity: `game_id = sha256(normalised_install_path)`. Never a runtime hash.

### 5.2 Executable resolution

Breadth-first walk to depth 5 skipping known non-game dirs (`Engine/`, `redist/`,
`_CommonRedist/`, `crashreport/`, …). Score candidates on: title similarity to file
name and version-resource description, launcher-word penalties, Unreal
`Binaries/Win64/*-Win64-Shipping.exe` bonus, Unity `<Name>_Data` sibling bonus,
RED Engine `bin/x64*` bonus, presence of `steam_api64.dll`, size. Deterministic,
unit-tested on fixture trees.

### 5.3 Analysis per game

| Fact | How |
|---|---|
| Bitness | COFF machine type (`0x14C` → 32, `0x8664` → 64) |
| Graphics APIs | Byte scan of `.rdata`/`.idata`/`.data` for `d3d12.dll`, `D3D12CreateDevice`, `d3d11.dll`, `d3d9.dll`, `vulkan-1.dll`, `vkCreateInstance`, `opengl32.dll`; plus renderer DLLs beside the exe. Result: ordered set, e.g. `[DX12, Vulkan]` |
| Ships DLSS | `nvngx_dlss.dll`, `nvngx_dlssg.dll`, `nvngx_dlssd.dll`, or Streamline `sl.*.dll` anywhere to depth 6; record versions |
| DLSS 5 present | `nvngx_dlssnr.dll` beside exe or in a DLSS dir |
| Existing ReShade / OptiScaler | Version-resource ProductName / OriginalFilename of proxy DLLs, so we can refuse to clobber a user's own setup |
| Anti-cheat | Folder and file markers (EasyAntiCheat, BattlEye, `vgk.sys`, GameGuard, `x3.xem`, Javelin, ACE, mhyprot, Ricochet) plus a short title list (CS2, Valorant, CoD, Overwatch) |
| Engine | Unreal / Unity / RED / other, from the same signals, informational |

Cached per game with exe size + mtime invalidation.

### 5.4 Routing (v1)

```
if anti_cheat          → BLOCKED (override requires typed confirmation)
if bitness == 32       → UNSUPPORTED in v1 (phase 3: host64 route)
if api has DX12 or DX11:
    if ships_dlss      → Route A: OptiScaler
    else               → Route B: ReShade + RenoDX DLSS5 + Feeder
elif api has Vulkan:
    if ships_dlss      → Route A: OptiScaler (vulkan proxy)
    else               → Route C: ReShade (vulkan) + Feeder
elif api is DX9 / DX10 → UNSUPPORTED in v1 (phase 3: dgVoodoo)
else                   → UNSUPPORTED
```

The chosen route and the reason are shown as the one sentence on the Game screen.
Advanced lets the user override the route; the override is stored per game.

### 5.5 Routes

**Route A — OptiScaler (game ships DLSS)**
1. Refuse if a proxy slot is occupied by a foreign non-ReShade DLL we don't recognise.
2. Copy OptiScaler payload (minus docs, redist, setup scripts) beside the exe.
3. Copy `OptiScaler.dll` as the first free proxy name: DX12 `dxgi, winmm, version, dbghelp, d3d12, wininet, winhttp`; Vulkan `winmm` first.
4. Copy the user's `nvngx_dlssnr.dll` beside the exe.
5. Leave `OptiScaler.ini` untouched except for the menu hotkey if the user set one.

**Route B — ReShade + RenoDX DLSS5 + Feeder (no DLSS, DX11/DX12, 64-bit)**
1. Install ReShade (add-on build) for the detected API by copying `ReShade64.dll` to
   the proxy name (`dxgi.dll` / `d3d11.dll` / `d3d12.dll`) and writing a minimal
   `ReShade.ini`. If ReShade's setup exe must be used, run it headless with a timeout
   and record every artefact it leaves.
2. Copy `renodx-dlss5.addon64` and `dlss5-feed.addon64` beside the exe.
3. Copy the model beside the exe.
4. Write `ReShade.ini` add-on settings only where keys are missing.

**Route C — ReShade (Vulkan) + Feeder** — as B without RenoDX.

**Update** = same route, newer component versions, same manifest id, backups preserved.
**Remove** = restore every backed-up file, delete every file we added, in reverse order,
delete our manifest. Nothing else.

### 5.6 Install transaction

```
plan     = Planner::plan(game, route, components) -> Vec<FileOp>
                                 // Write{src,dst} | Backup{dst} | Delete{dst} | EditIni{...}
preview  = plan.describe()       // what the user reads under "What will change"
journal  = Journal::begin(game_id)
for op in plan: journal.apply(op)?   // backup before overwrite, record after each op
journal.commit()                     // writes manifest (AppData + sidecar beside exe)
on error: journal.rollback()         // restores every backup, removes every added file
```

Manifest per game (JSON): schema version, game id, exe path, route, component
versions and hashes, add-on flags, timestamp, anti-cheat override flag, list of
`{target, backup_path, was_existing, sha256_before, sha256_after}`.

---

## 6. Components and the manifest

A `components.json` in **our** repo, signed (minisign) and fetched from our GitHub
release, pins each component:

```json
{
  "schema": 1,
  "components": [
    {
      "id": "optiscaler",
      "name": "OptiScaler",
      "license": "GPL-3.0",
      "publisher": "https://github.com/optiscaler/OptiScaler",
      "version": "x.y.z",
      "asset": "https://github.com/optiscaler/OptiScaler/releases/download/…/….7z",
      "sha256": "…",
      "extract": { "strip": ["docs/", "redist/", "images/", "tests/"] }
    }
  ]
}
```

v1 component list (all fetched from the publisher's own release page):

| Component | Publisher | Licence | Notes |
|---|---|---|---|
| OptiScaler | optiscaler/OptiScaler | GPL-3.0 | Route A |
| ReShade (add-on build) | reshade.me | BSD-3 | Route B/C. Setup exe or extracted DLL per upstream terms |
| RenoDX DLSS 5 add-on | clshortfuse/renodx | MIT | Route B |
| DLSS5-Feeder add-on | jlrouzies-fr/DLSS5-Feeder | MIT | Route B/C |
| DLSS 5 model `nvngx_dlssnr.dll` | **user-supplied** | NVIDIA | Never fetched. Hash + signature shown |

Explicitly **not** included: LumeniteFX (AGNYA), DLSS-NR-on-AMD (all rights reserved),
dgVoodoo2 (closed; phase 3 only via user-supplied file), Deep Fried Chicken, MFG
unlock add-ons, any NVIDIA Streamline/DLSS runtime updates.

Download rules: HTTPS only, pinned URL, size limit, SHA-256 verified before anything
is extracted, extracted into `%LOCALAPPDATA%\shim\components\<id>\<version>\`,
never executed by us except ReShade's setup if unavoidable. A failed hash is an
error, not a warning.

---

## 7. Architecture

### 7.1 Stack

- **Tauri 2** shell, Windows x64 target.
- **Rust** core crate `shim-core` (no Tauri dependency): discovery, analysis,
  planning, journal, components, persistence.
- **Rust** `shim-win` crate: registry, Authenticode, elevation helpers, behind traits
  so the core compiles and tests on macOS/Linux with fakes.
- **TypeScript** front end: React 19 + Vite + Tailwind 4, Zustand for state,
  typed Tauri commands via `tauri-specta`.
- Build and e2e on `windows-latest` in GitHub Actions. No mock layer: the app runs
  natively on macOS for UI work, with Windows-only features (scan, install)
  disabled and labelled as such. Development moves to a Windows PC from Phase 1.

Key crates: `goblin` (PE), `winreg`, `keyvalues-parser` (VDF), `serde`, `reqwest`,
`sha2`, `tokio`, `tracing`, `thiserror`, `minisign-verify`, `sevenz-rust`/`zip`.

### 7.2 Module layout

```
shim/
├─ crates/
│  ├─ core/
│  │  ├─ discovery/      steam.rs epic.rs gog.rs xbox.rs ubisoft.rs ea.rs custom.rs
│  │  ├─ analysis/       exe_resolver.rs pe.rs apis.rs dlss.rs anticheat.rs engine.rs
│  │  ├─ routing/        router.rs routes/{optiscaler.rs, reshade_renodx.rs, reshade_vk.rs}
│  │  ├─ install/        planner.rs journal.rs manifest.rs ini.rs
│  │  ├─ components/     manifest.rs fetch.rs verify.rs store.rs
│  │  ├─ artwork/        steam_cdn.rs icon.rs cache.rs
│  │  ├─ persistence/    paths.rs library.rs settings.rs
│  │  └─ error.rs        one typed error enum, user-facing message + detail
│  └─ win/               registry.rs authenticode.rs elevate.rs
├─ src-tauri/            commands.rs (thin), events.rs, main.rs
├─ src/                  React app
│  ├─ screens/           Library/ Game/ Components/ Settings/
│  ├─ components/        Card, StatusLine, Badge, Button, Dialog, LogDrawer
│  ├─ design/            tokens.css theme.ts
│  ├─ store/             library.ts game.ts components.ts
│  └─ i18n/              en.json
├─ components.json       pinned component manifest (signed on release)
├─ fixtures/             fake game folder trees for tests
└─ .github/workflows/    ci.yml release.yml
```

Rules: files 200–400 lines, functions under 50 lines, immutable data (plans and
manifests are values, never mutated in place), no `unwrap` outside tests, every
error carries a user-facing message.

### 7.3 Persistence (`%LOCALAPPDATA%\shim\`)

| Path | Content |
|---|---|
| `library.json` | discovered games + analysis cache |
| `settings.json` | scan sources, theme, language, update check |
| `installs\<game_id>.json` | install manifest (also `shim.json` sidecar beside exe) |
| `backups\<game_id>\<sha10>_<name>` | original files |
| `components\<id>\<version>\` | verified extracted components |
| `model\` | nothing; we store only the path and hash of the user's model |
| `cache\covers\` | artwork |
| `logs\` | rotating install logs |

---

## 8. Error handling and feedback

- One `Error` enum in core with variants like `GameFolderNotWritable`, `ComponentHashMismatch`,
  `ForeignProxyPresent`, `AntiCheatBlocked`, `RollbackFailed`. Each has `user_message()`
  and `detail()`.
- Front end shows `user_message` inline where the action was taken, with `detail`
  behind "Show details".
- Scan errors per adapter are collected and reported, never swallowed.
- Every install writes a log file; the log drawer shows the same text.
- `RollbackFailed` is the one unrecoverable state: it lists the exact files and their
  backup paths so the user can finish by hand.

---

## 9. Testing

- **Core unit tests** on fixture folder trees: exe resolution, API detection on tiny
  hand-built PE files, anti-cheat markers, routing table, planner output, journal
  rollback (inject failure at each op).
- **Component verification tests**: hash mismatch, truncated download, bad signature.
- **UI tests** (Vitest + Testing Library) against the mock command layer.
- **E2E** on Windows CI with a synthetic game folder: scan → install → verify files →
  remove → verify byte-identical restore.
- Coverage target 80% on core.

---

## 10. Phases

**Phase 0 — Scaffold (week 1)** ✅ done 2026-10-03
Tauri 2 project, core crate (models, settings, library, discovery trait, persistence
with quarantine, typed errors), Windows glue crate, design tokens, light/dark shell with
the three screens as empty states, generated TS bindings, CI on Windows. Runs on macOS
with scanning disabled.

**Phase 1 — See (week 2)** ✅ done 2026-10-03 (core smoke-tested on real Steam games; the built app launches on Windows — click Scan once in `npm run tauri dev` to see it end to end)
Steam/Epic/GOG adapters, exe resolution, PE analysis, anti-cheat detection, Library
grid with real status lines, Game screen with badges and the routing sentence.
No writing to game folders yet. "What will change" shows the plan.

**Phase 2 — Install (weeks 3–4)** ✅ code done 2026-10-03; see §12.0b for the six
decisions that need your review and the one open question (OptiScaler + model) that
needs a real RTX 50 test before 0.1 is called usable.
Component manifest + verified fetch, model picker, Route A and Route B, journal with
rollback, Update and Remove, logs, confirmations. First usable release (0.1).

**Phase 3 — Breadth (weeks 5–6)** ✅ code done 2026-10-03 except Route C (see §12.0c
for why it stays out and exactly what it would take); release workflow publishes
hashes, code signing waits for a certificate.
Route C (Vulkan), Xbox/Ubisoft/EA adapters, custom folders, hidden games, artwork
cache, update checker, signed release with checksums, i18n table. Release 0.2.

**Phase 4 — Only if asked**
32-bit via host64, DX9 via user-supplied dgVoodoo, emulators, AMD route if a
redistributable upstream exists, more translations.

---

## 11. Open questions

1. **ReShade distribution.** reshade.me publishes a setup exe, not raw DLLs. Confirm
   whether running it headless (`--api … --headless`) is acceptable, or whether the
   add-on-build DLL may be extracted and placed directly under its licence.
2. **RenoDX add-on versioning.** Nightly vs tagged release; which build is stable
   with the current Feeder. Pin one and document the pairing in `components.json`.
3. **Model provenance UX.** How much to say about where `nvngx_dlssnr.dll` comes
   from. Current stance: show hash and signature status, link nothing.
4. **Code signing certificate.** Cost vs. SmartScreen friction. Publish hashes from
   day one either way.

---

## 12. Handoff (written 2026-10-03, end of Phase 0 on macOS; updated the same day after Phase 1 on Windows)

Read this first if you are picking the project up.

### 12.0 Phase 1 on the Windows PC (2026-10-03)

- Toolchain installed via winget: Rustup (stable MSVC), Node 24 LTS, VS Build Tools
  2022 with the C++ workload and Windows SDK 10.0.26100. `npm install` on Windows
  added every platform's optional native bindings to `package-lock.json` (542 lines,
  all additions); that is the lockfile the plan wanted, so keep it.
- `.gitattributes` forces `eol=lf`; the PC has `core.autocrlf=true` and without it
  every ts-rs output showed as modified.
- Core: 103 Rust tests, 15 front-end tests, clippy `-D warnings` on the whole
  workspace and `tsc` clean. Phase 1 code (all in `crates/core/src`):
  - `discovery/{vdf,steam,epic,gog}.rs` — own ~150-line VDF parser (no crate),
    Steam via registry + `libraryfolders.vdf` + ACF, Epic `.item` JSON, GOG registry.
    GOG and Steam read the registry through the `Registry` trait and are tested
    with `platform::testing::FakeRegistry`. Fixtures in `fixtures/{steam,epic}`.
  - `analysis/{walk,pe,apis,dlss,anticheat,exe_resolver,mod}.rs` — one bounded
    folder walk per game (`WALK_DEPTH = 9`, Unreal keeps DLSS nine levels down),
    hand-rolled PE header/section reader (no goblin), import-string API scan over
    `.rdata/.idata/.data`, DLSS/Streamline/model presence with version from
    `VS_FIXEDFILEINFO` in `.rsrc`, foreign ReShade/OptiScaler via strings in proxy
    DLLs, anti-cheat markers, deterministic exe scoring, engine guess. Analysis is
    cached by exe size+mtime (`analyse_cached`).
  - `routing/mod.rs` — PLAN §5.4 as a pure function, `GameStatus::Ready` now carries
    the `reason` sentence. Foreign ReShade/OptiScaler → `Unsupported` for now.
  - `install/mod.rs` — planner **preview only** (`PlannedChange {kind, path, note}`),
    no file ops yet.
  - `scan.rs` — discover → merge → analyse → route, with a progress callback.
- Tauri: `scan_library` runs in `spawn_blocking` and emits `scan://progress`
  (`ScanProgress`); new command `plan_preview(game_id)`. Front end shows progress in
  the Library header; the Game page shows fact badges, the route sentence, and the
  planned file list; Install button is present but disabled.
- Smoke test against the real machine: `cargo run -p shim-win --example scan`
  (writes nothing). Found 4 Steam games in ~3 s: Assetto Corsa Rally, Bodycam and
  STALKER 2 → OptiScaler (ship DLSS, versions read), MSFS 2024 → Unsupported because
  the user's own ReShade is installed. Epic and GOG are not installed on this PC, so
  those adapters are only fixture-tested.

### 12.0b Phase 2 on the Windows PC (2026-10-03, same day)

All four Phase 2 steps are in. What exists:

- **Components** (`crates/core/src/components/`): `components.json` at the repo root
  is compiled in (`ComponentManifest::embedded()`) and validated by a test. Pins:
  OptiScaler 0.9.4 (7z), ReShade 6.8.0 add-on build (the setup exe *is a zip* with
  `ReShade64.dll` inside, so we unpack it and never run it), DLSS5-Feeder 1.17.0.
  Hashes were computed from the publishers' assets on 2026-10-03. `fetch.rs` streams
  with a size cap and SHA-256; `extract.rs` handles zip/7z with path-traversal
  checks and strip/only rules; `store.rs` keeps `components\<id>\<version>\` with a
  `.shim-component.json` record of every file's hash (`Verified`/`Missing`/`Mismatch`).
  minisign verification exists (`verify_signature`, tested with a generated key) but
  `PUBLIC_KEY` is `None`: no remote manifest fetch until a release key exists.
- **Install transaction** (`crates/core/src/install/`): `planner.rs` builds `FileOp`s
  (Copy / WriteText) per route; `journal.rs` backs up before overwrite, records after
  each write, persists a journal file after each op, rolls back on failure, and
  `uninstall` walks the manifest backwards; `recover()` finishes an interrupted
  install at app start. `manifest.rs` writes `installs\<id>.json` + `shim.json`.
  Tests: byte-identical install→uninstall for both routes, failure injected at
  every op, crash recovery, unwritable folder, leftover reporting.
- **Install state in the scan**: `scan::analyse_game` layers `Installed` /
  `UpdateAvailable` from the manifest, so our own proxy DLL never reads as foreign.
- **User files**: `Settings.model_path`, `renodx_addon_path`, `dlss_runtime_path`;
  `inspect_file` returns size, SHA-256 and Authenticode (`shim-win/authenticode.rs`,
  WinVerifyTrust + signer name; tested against a real signed binary).
- **Tauri**: `get_components`, `fetch_component` (event `component://progress`),
  `inspect_file`, `install_game(game_id, confirm_anti_cheat)` and `remove_game`
  (event `install://progress`), `get_install`, `plan_preview` now returns a
  `Preview {changes, exact, blocker}`. One `busy` flag serialises install/remove/fetch.
- **UI**: Components screen with fetch buttons, progress bars and three file pickers
  (tauri-plugin-dialog); Game page with Install / Update (= remove + install) /
  Remove, inline remove confirmation listing the file count, anti-cheat typed
  confirmation (`REMOVE-MY-DOUBTS`), progress line, result line, and the installed
  file list from the manifest.
- **Real-asset check**: `cargo run -p shim-win --example e2e_install -- <assets dir>`
  unpacks the real downloads and runs both routes through plan → journal →
  uninstall on a synthetic folder, asserting byte-identical restore.

Decisions taken in Phase 2 that change the plan (please review):

1. **RenoDX DLSS 5 add-on is user-supplied**, like the model. It is only published
   on the RenoDX Discord; third parties mirror it. §3/§6 forbid fetching from
   anywhere but the publisher's release page, so it joins the "your files" list.
2. **Route B also needs the user's `nvngx_dlss.dll`**: DLSS5-Feeder documents that a
   game without DLSS needs the DLSS runtime beside the exe, and we never fetch
   NVIDIA runtimes. Three user files for Route B, one for Route A.
3. **Route C (Vulkan without DLSS) is Phase 3.** ReShade on Vulkan is a machine-wide
   implicit layer registered in `HKLM`; a per-game file journal cannot undo it
   cleanly. The router now returns `Unsupported` for it.
4. **ReShade is unpacked, not run.** The add-on setup exe carries a zip with
   `ReShade64.dll`; we take only that and `ReShade64.json`.
5. **`ReShade.ini` and `ReShadePreset.ini` are generated** (add-ons on, shader path,
   preset enabling `DLSS5_Feed` with `DLSS5_MV_PROVIDER=0`). Feeder recommends a
   motion-vector provider shader (`=3`, LumeniteFX, which §6 excludes as AGNYA); the
   default provider works but is "not recommended" by Feeder. Open question for
   Advanced.
6. ~~Open question: whether upstream OptiScaler loads `nvngx_dlssnr.dll`.~~
   **Answered the same evening: it does not.** See §12.0d for the corrected routes.

### 12.0c Phase 3 on the Windows PC (2026-10-03, same day)

- **Adapters**: Ubisoft (`Launcher\Installs\<id>\InstallDir` + name from the
  `Uplay Install <id>` uninstall entry), EA (vendor keys `EA Games` / `Electronic
  Arts` / `EA Sports` with `Install Dir`, plus uninstall entries whose Publisher is
  Electronic Arts, deduped by folder, EA app/Origin skipped), Xbox
  (`<drive>:\XboxGames\<Name>\Content\MicrosoftGame.config`, title from
  `ShellVisuals DefaultDisplayName`, exe from the first `<Executable Name>`; the
  read-only `WindowsApps` store is not scanned), Custom folders (each subfolder with
  an exe within 3 levels). `default_adapters(&settings.scan)` now needs the sources.
  **None of these launchers is installed on this PC**, so they are fixture-tested only.
- **Library**: right-click menu on a card (Open folder / Rescan / Hide), hidden count
  link, hidden list with Unhide in Settings; custom folders add/remove in Settings.
- **Artwork**: `artwork.rs` fetches Steam `library_600x900.jpg` into
  `cache\covers\<game_id>.jpg` after a scan (`fetch_covers` command, `cover://ready`
  event). Non-Steam games keep the title card; no exe-icon fallback.
- **Update check**: `update.rs` asks the GitHub latest-release API on launch when
  `check_updates` is on; a banner offers to open the release page. Nothing is
  downloaded. The repo is private today, so the API returns 404 until it is public
  or a token is used; the banner simply stays hidden.
- **Open folder** runs `explorer.exe /select,<exe>` from Rust (path from the library,
  no plugin scope). **Elevation**: `relaunch_elevated` (ShellExecuteW `runas`) is
  offered inline when an install fails with `game_folder_not_writable`.
- **Release workflow** `.github/workflows/release.yml`: on tag `v*`, checks the tag
  matches `Cargo.toml` and `tauri.conf.json`, builds the NSIS installer + portable
  exe, writes `SHA256SUMS.txt`, creates a *draft* release. No code signing yet.
- **Route C stays out.** ReShade on Vulkan = `ReShade64.dll` + `.json` in
  `%ProgramData%\ReShade`, a `REG_DWORD 0` named by the json path under
  `HKLM\SOFTWARE\Khronos\Vulkan\ImplicitLayers`, and the exe path appended to
  `%ProgramData%\ReShade\ReShadeApps.ini` (verified in crosire/reshade's setup
  source). Both writes need administrator rights, which §3 forbids by default; it
  also needs a registry *write* on the `Registry` trait and journal entries for
  files outside the game folder. Doable later behind the elevation flow.

### 12.0d Route A was wrong; fixed after the first real test (2026-10-03, evening)

The first real install (Bright Memory Infinite RT Benchmark, v0.1.0, Route A =
upstream OptiScaler + model) produced no DLSS 5 pass. Diagnosis came from the
one working DLSS 5 setup on this PC, MSFS 2024, which someone else's installer
had set up: **no OptiScaler at all**. It is ReShade 6.8.0 add-on build as
`dxgi.dll`, `renodx-dlss5.addon64`, `nvngx_dlssnr.dll`, and this in `ReShade.ini`:

```
[RenoDX.DLSS5]
NeuralUplift=1
NREnableUpscaling=0
NRIntensity=1
NRPreset=0
```

`ReShade.log` shows the add-on hooking the game's own `nvngx_dlss.dll`
(`NGX hooks installed`, `feature create intercepted: feature=1 (DLSS/DLAA)`) and
pre-loading the NR runtime. Upstream OptiScaler is an upscaler *replacement*; it
never loads the model. The §5.4 table and §5.5 routes are superseded by this:

| Game | Default route | Optional modes (Advanced, per game) |
|---|---|---|
| Ships DLSS, DX11/DX12 | `ReShadeRenoDx`: ReShade + RenoDX add-on + model, ini above (+`EnableHooks=2`) | `OptiScalerDlssNr`: Dagherbou's OptiScaler DLSS-NR fork (GPL-3, GitHub releases, pinned 0.2.0) + model, `[DlssNr] Enabled=true` patched into its ini; `OptiScaler`: upstream 0.9.4, no model |
| No DLSS, DX11/DX12 | `ReShadeFeeder`: the above + DLSS5-Feeder + preset + user's `nvngx_dlss.dll` | none |
| Vulkan | Unsupported (Phase 3) | |

`InstallMode { Dlss5, OptiScalerDlss5, OptiScalerOnly }` lives on `Game.mode`
(library.json, carried through merges), set by `set_game_mode`, refused while
installed. The fork's own README: RTX 50 only, game must already use DLSS, DX12
(DX11 via the bridge, Vulkan too), "Enable Neural Rendering" is off by default —
hence the ini patch. Its NR colour composition is RenoDX's (MIT attribution).

The model on this PC (`8270b350…`) is a modified build: shim shows
"Signature by NVIDIA Corporation does not match the file", and the RenoDX log
says "custom runtime accepted; untested build". Expected for non-RTX-50 builds.

Old `optiscaler` install records from 0.1.0 still load (the variant stays in
`Route`) so they can be removed through the app.

### 12.1 Where things stand (end of Phase 0)

- Repo: `git@github.com:riccardolardi/shim-dlss5.git`, branch `main`. Product name
  **shim**; crates `shim-core`, `shim-win`, Tauri crate/exe `shim`. Local folder on
  the Mac was still called `dlss5er`; the name does not matter.
- Phase 0 is complete and runs. On macOS the app boots, renders the three screens,
  saves settings, quarantines damaged JSON and shows the warning, and logs to
  `<data dir>/logs/shim.log`. Scanning is disabled off-Windows and the UI says so.
- Data dir: `%LOCALAPPDATA%\shim` (override with `SHIM_DATA_DIR`).
- Test counts at handoff: 45 Rust (core), 13 front end. clippy `-D warnings`,
  rustfmt and `tsc` are clean. CI is fully green (core on Linux and macOS, front
  end, and the Windows app job, which compiles the Tauri app with MSVC and uploads
  `shim.exe` as an artifact). It took three lockfile fixes to get there; see 12.4.
- Nothing has been written into a game folder yet. There is no install code.

### 12.2 How to get going on Windows

```powershell
winget install Rustlang.Rustup   # or rustup-init.exe; stable toolchain
winget install OpenJS.NodeJS.LTS # Node 22
# Visual Studio Build Tools with "Desktop development with C++" + Windows SDK
# WebView2 is preinstalled on Windows 10/11
git clone git@github.com:riccardolardi/shim-dlss5.git && cd shim-dlss5
npm install
npm run tauri dev                # hot-reloading app
cargo test --workspace           # also regenerates src/lib/generated/*.ts
npm test
```

First thing to confirm on Windows: `npm run tauri dev` opens the window, the
Library's Scan button is enabled (`app_info.can_scan` is true), and
`%LOCALAPPDATA%\shim\logs\shim.log` is written. Then CI's Windows job should be
green on the next push.

### 12.3 Code map (what exists, where to add things)

| Path | What it is | Notes for Phase 1 |
|---|---|---|
| `crates/core/src/model.rs` | `Launcher`, `GraphicsApi`, `Bitness`, `AntiCheat`, `Route`, `GameStatus`, `DiscoveredGame`, `Analysis`, `Game` | All `#[ts(export)]`; run `cargo test -p shim-core` after changing |
| `crates/core/src/error.rs` | One `Error` enum with `code()`, `user_message()`, `detail()`, `to_dto()` | Add variants, never swallow |
| `crates/core/src/discovery/mod.rs` | `LauncherAdapter` trait, `discover_all`, `default_adapters()` (empty) | Add `steam.rs`, `epic.rs`, `gog.rs` here; register in `default_adapters` under `#[cfg(windows)]` or make them take the `Registry` trait so they test everywhere |
| `crates/core/src/platform.rs` | `Registry`, `SignatureChecker` traits, `Unavailable` impls | Adapters read the registry only through `Registry` |
| `crates/win/src/registry.rs` | Real `Registry` via `winreg` 0.56 | No WOW64 flag: name `SOFTWARE\WOW6432Node\...` explicitly |
| `crates/core/src/library.rs` | `Library`, `game_id()` (sha256 of normalised path), `merge_scan`, `with_hidden` | `merge_scan` keeps games of failed/unavailable launchers; tests cover it |
| `crates/core/src/persist.rs` | `read_json`, `read_json_or_quarantine`, `write_json` (temp + fsync + rename) | Use for every file we own |
| `crates/core/src/settings.rs` | `Settings`, `ScanSources`, `validated()` | |
| `crates/core/src/paths.rs` | `AppPaths` | |
| `crates/core/src/api.rs` | `AppInfo`, `ScanReport` (Tauri boundary shapes) | Keep boundary types here so one test regenerates all TS |
| `src-tauri/src/commands.rs` | `app_info`, `get_settings`, `save_settings`, `get_library`, `scan_library` | `scan_library` has a re-entry guard; make it `spawn_blocking` once scans do real I/O |
| `src-tauri/src/lib.rs` | logging (stderr + file), fatal message box on Windows | Logging is initialised before `AppState::boot` on purpose |
| `src-tauri/tauri.conf.json` | window, CSP, `assetProtocol` scope `$LOCALDATA/shim/cache/covers/**` | Covers via `convertFileSrc` will work once files land there |
| `src/store/app.ts` | zustand store: screen, info, settings, library, scan | Optimistic settings save with safe rollback |
| `src/lib/status.ts` | status line + filter logic (tested) | |
| `src/i18n/en.json` | all UI strings | Keys only; no hard-coded English in components |
| `src/design/tokens.css` | the only place colours/radii/type live | |
| `fixtures/` | does not exist yet | Create fake game trees for exe/API/anti-cheat tests |

### 12.4 Things learned the hard way (do not relearn)

- **ts-rs 12**: export dir is set once in `.cargo/config.toml` (`TS_RS_EXPORT_DIR`);
  do not use `export_to` per type. `u64` exports as `bigint`; annotate
  `#[ts(type = "number | null")]` on timestamps. Bindings are committed and CI
  diffs them.
- **Native-binding lockfile trap (npm/cli#4828)**: rolldown (vite 8), lightningcss and
  `@tailwindcss/oxide` (Tailwind 4) and `@tauri-apps/cli` each ship their native code
  as per-platform optional packages. A lockfile generated on one OS has resolved
  entries only for that OS, so `npm ci` (and `npm install`, which trusts the lock)
  fails everywhere else. Fix in place: `package.json` declares the `linux-x64-gnu`
  and `win32-x64-msvc` variants of all four (plus rolldown's `wasm32-wasi` fallback)
  as root `optionalDependencies` at the exact versions the lock resolves, and pins
  `lightningcss` via `overrides` so there is one version. When bumping vite,
  tailwind or the tauri cli, bump the matching binding versions in the same change.
- **Tauri CI order**: `tauri::generate_context!` embeds `../dist` at compile time,
  so `npm run build` must run before any `cargo` step that compiles `src-tauri`.
- **zustand selectors** must return stable references. `useApp(s => s.x ?? [])`
  re-renders forever and blanks the window. Select the parent and derive outside.
- **cargo fmt and the editor**: formatting rewrites files; re-read before editing.
- **Quarantine, don't reset**: damaged `settings.json`/`library.json` are renamed
  to `.bad` and reported via `AppInfo.startup_warnings`. Keep that behaviour for
  every file we add (manifests especially).
- Release builds have no console (`windows_subsystem = "windows"`); the file log
  and the fatal `MessageBoxW` are the only visible output.
- **Walk depth**: Unreal games keep `nvngx_dlss.dll` under
  `Engine/Plugins/Marketplace/DLSS/Binaries/ThirdParty/Win64/` (9 levels). A depth
  of 6 missed STALKER 2's DLSS and routed it wrong. `WALK_DEPTH` is 9; the walk
  still takes well under a second per game because junk folders are skipped.
- **Unity detection**: "any folder ending in `_Data`" is too loose (MSFS 2024 has one
  deep in its content). Only the exe's own `<Name>_Data` or `UnityPlayer.dll` count.
- **Test PE files**: `pe::testing::build_pe` aligns sections to 512 bytes, so a test
  that wants the file size to change must add more than 512 bytes.
- **Git Bash heredocs** on this PC collapse `\\` to `\` even when quoted. Write
  fixtures with Windows paths via the Write tool or PowerShell.
- `git status` lies about generated files when `core.autocrlf` is on; trust
  `git diff --stat`. `.gitattributes` now pins LF.
- **7z strip rules**: with `sevenz-rust2`, an entry you skip must still be read
  through (`io::copy` to a sink) or the solid stream's checksum fails on the next
  entry. The first OptiScaler unpack only worked because no strip rule matched.
- **windows-sys feature gates**: `WTHelperProvDataFromStateData` and
  `WTHelperGetProvSignerFromChain` need `Win32_Security_Cryptography_Catalog` *and*
  `Win32_Security_Cryptography_Sip`, not just `Win32_Security_WinTrust`.
- **reqwest 0.13** has no `rustls-tls` feature name any more; default TLS (schannel
  on Windows) with `features = ["blocking"]` is enough and smaller.
- **Parallel tests sharing a temp path**: two tests building a fake zip at the same
  `%TEMP%` name raced. Build fixtures inside the test's own tempdir.
- **Review pass after Phase 3 (2026-10-03) found and fixed**: (1) the journal wrote
  its record only *after* a file was written, so a crash mid-copy left a corrupted
  target with no record — now a provisional record is persisted first and filled in
  after; (2) `fs::copy` carries the read-only attribute and `remove_file` fails on
  read-only files on Windows — `make_writable` runs before every overwrite/removal
  and after every copy; (3) empty-folder cleanup climbed `parent()` without a bound —
  it now stops at the exe folder, which is also stored in the journal file;
  (4) a manifest that fails to save after all files were written is treated as a
  failed op and rolled back; (5) `recover_all` scans `installs\*.journal.json` at
  boot instead of only known game ids; (6) archive entries are read through a
  512 MB cap and a 64 MB pre-allocation cap (forged headers, bombs); (7) the folder
  walk skips symlinks/junctions. Tests cover each.
- **Windows paths on Unix CI**: the core is tested on macOS/Linux too, where
  `Path::is_absolute("D:\\x")` is false, `file_name()` does not split on `\`, and
  `join` inserts `/`. Paths that come from launcher records are Windows paths by
  nature: use `discovery::{is_windows_absolute, last_segment, win_join}` on them,
  never `Path` semantics. This had the Unix CI jobs red from Phase 1 to Phase 3
  while the Windows job was green; always check *all* CI jobs (`gh run list`).

### 12.5 Review items deliberately deferred

From the Phase 0 code review, not yet done:

- `Registry` trait has no WOW64 view parameter; add one if an adapter needs both
  views of the same key.
- `Library.tsx` uses `useApp()` without a selector (re-renders on every store
  change). Fine at this size; switch to selectors when the grid gets big.
- `settings.language` is wired to `setLanguage` but there is only `en`.
- `opener:default` capability covers URLs only; "Open folder" needs
  `opener:allow-open-path` with a scope when implemented.
- Commands are sync except `save_settings`/`scan_library`; move real I/O to
  `tauri::async_runtime::spawn_blocking`. (`scan_library` done in Phase 1.)

Added after Phase 1:

- `Analysis.dlss_version` reads `nvngx_dlss.dll` only; Streamline-only games
  (`sl.*.dll` + `nvngx_dlssg.dll`) report `ships_dlss` without a version.
- Foreign ReShade/OptiScaler currently yields `Unsupported`. Phase 2 must tell our
  own installs apart (the `shim.json` sidecar) before Installed/UpdateAvailable can
  be derived, and should offer "take over" for a foreign ReShade behind Advanced.
- `GameStatus::Installed`/`UpdateAvailable` are never produced yet.
- Game page "Open folder" is still disabled (needs `opener:allow-open-path`).
- Exe scoring ignores the version-resource description (`FileDescription`); add it
  if a real library produces a wrong pick. ACR, Bodycam, STALKER 2, MSFS 2024 are
  all right today.
- The Game page's `relative()` helper lives in `screens/Game.tsx`; move to `lib/` when
  a second screen needs it.

### 12.6 Phases 1 and 2 — ✅ done 2026-10-03 (see §12.0 and §12.0b)

Test counts: 139 core + 2 win Rust tests, 15 front-end tests; clippy `-D warnings`
across the workspace, rustfmt and `tsc` clean; `e2e_install` example green against
the real payloads.

Before calling 0.1 usable (do these first, in this order):

1. **Run the app and click through once**: `npm run tauri dev` → Components: Fetch
   all three (real downloads, progress bars) and pick a model file → a Ready game →
   Install → Remove. The core paths are tested; the UI click-path is not.
2. **Real RTX 50 test of Route A** (OptiScaler + model) on one of the Steam games;
   decide on `Dx12Upscaler=dlss` and add an `EditIni` op if needed (§12.0b item 6).
3. **Generate the minisign release key**, set `PUBLIC_KEY`, and add the remote
   manifest fetch (`fetch_remote_manifest`) so pins can update without a release.

Phase 3 is also in (§12.0c). What remains before tagging `v0.1.0`:

4. Click through the new UI once: right-click menu, custom folder add, hidden list,
   Components fetch, Install/Remove. Then `git tag v0.1.0 && git push --tags` and
   publish the draft release the workflow creates.
5. Make the repo public (or give the update checker a token) so the update banner
   can work; verify the Steam cover fetch fills the Library cards.
6. Later: Route C behind the elevation flow; code signing; exe-icon cover fallback;
   version-resource `FileDescription` in exe scoring if a real library picks wrong.

Original Phase 1 goal, kept for reference: Library shows real games with real status
lines; Game page shows badges and the routing sentence; nothing is written to game
folders.

1. **Fixtures**: `fixtures/games/<case>/...` with tiny fake exes (hand-built PE
   headers are enough for bitness and import-string scans), DLSS dlls, anti-cheat
   markers, Unreal/Unity layouts. Tests read these; no registry needed.
2. **Steam adapter** (`discovery/steam.rs`): `SteamPath` from `HKCU\Software\Valve\Steam`,
   `steamapps\libraryfolders.vdf` via `keyvalues-parser`, each `appmanifest_*.acf`
   (title, installdir, appid, `StateFlags` == 4 fully installed). Skip tools/redists
   by appid list. Test the parsers on fixture VDF/ACF text.
3. **Epic** (`%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`) and
   **GOG** (`HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\*`).
4. **Analysis** (`analysis/`): `exe_resolver.rs` (scoring per PLAN §5.2),
   `pe.rs` (goblin: machine type, section bytes), `apis.rs` (string scan per §5.3),
   `dlss.rs` (DLSS/Streamline/model presence + versions), `anticheat.rs` (markers
   per §5.3). Pure functions over a path; cached by exe size+mtime.
5. **Routing** (`routing/router.rs`): the tree in §5.4, returning `Route` plus a
   `reason` string that becomes the one sentence on the Game page.
6. **Wire-up**: `scan_library` runs discovery then analysis in `spawn_blocking`,
   emits progress events (`tauri::Emitter`), sets `GameStatus`. Game page gets
   badges, the sentence, and a disabled Install button with "What will change"
   showing the planned file list from a `Planner` stub.
7. Artwork can wait for Phase 3; the card already handles `cover: null`.

### 12.7 Reference: what the original did that is worth copying as ideas

From the analysis of NODIX-TECH/DLSS-5-MANAGER (source-available, not reusable code):

- Route choice: DX9 → DX9 route; DX10 → DX11 route; ships DLSS → OptiScaler;
  DX11 → DX11 route; else DX12. Vulkan only by user choice.
- OptiScaler proxy slot order: DX12 `dxgi, winmm, version, dbghelp, d3d12, wininet,
  winhttp`; Vulkan `winmm` first. It never writes `OptiScaler.ini` except the menu key.
- API detection: scan `.rdata/.idata/.data` for `d3d12.dll`, `D3D12CreateDevice`,
  `d3d11.dll`, `d3d9.dll`, `vulkan-1.dll`, `vkCreateInstance`, `opengl32.dll`; ignore
  proxy DLLs whose version resource says ReShade/OptiScaler/dgVoodoo.
- Anti-cheat markers: EasyAntiCheat(_EOS) dirs, `start_protected_game.exe`,
  BattlEye `beservice/beclient/bedaisy.sys`, `*_BE.exe`, `vgk.sys`, GameGuard `.des`,
  XIGNCODE `x3.xem`, EA Javelin, Tencent ACE, mhyprot, Ricochet `randgrid.sys`.
- Bugs to avoid: runtime-random game ids, uninstall by filename, rollback that only
  removes files added this run, blocking HTTP inside the scan.

Competitors worth a look for UX density: DLSS5-Swapper (rakanki911), DLSS5-Autopilot
(Kizzuwatnaa, "what will happen?" preview and run-time fetch model), Optiscaler-Client.
