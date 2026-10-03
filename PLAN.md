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

**Phase 1 — See (week 2)**
Steam/Epic/GOG adapters, exe resolution, PE analysis, anti-cheat detection, Library
grid with real status lines, Game screen with badges and the routing sentence.
No writing to game folders yet. "What will change" shows the plan.

**Phase 2 — Install (weeks 3–4)**
Component manifest + verified fetch, model picker, Route A and Route B, journal with
rollback, Update and Remove, logs, confirmations. First usable release (0.1).

**Phase 3 — Breadth (weeks 5–6)**
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
