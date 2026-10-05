# shim-dlss5

**Puts DLSS 5 neural rendering into a game, keeps it current, and takes it out
again byte-for-byte.** Windows, DirectX 11/12. Nothing else.

shim-dlss5 scans your Steam / Epic / GOG / Xbox / Ubisoft / EA libraries, reads each
game's executable to see what it uses, tells you in one sentence what it would
change, and installs with a journal that can undo everything. Three clicks:
Scan, open a game, Install.

> **You supply the model.** The DLSS 5 model (`nvngx_dlssnr.dll`) is an NVIDIA
> file that shim-dlss5 never downloads, hosts or mirrors. You point shim-dlss5 at your copy;
> it shows you the file's SHA-256 and whether its NVIDIA signature is intact.
> The same goes for the RenoDX DLSS 5 add-on, which is only published on the
> RenoDX Discord.

## What it does

| Game | Default route | Optional (Advanced, per game) |
|---|---|---|
| Ships DLSS, DX11/DX12 | ReShade (add-on build) hosting the **RenoDX DLSS 5 add-on** over the game's own DLSS output, plus your model | **OptiScaler + DLSS 5**: [wilsjo2's OptiScaler fork](https://github.com/wilsjo2/OptiScaler-DLSSNR-PreSR-Multipass) with the pass run *before* the upscaler, at render resolution (cheaper); or plain OptiScaler without the pass |
| No DLSS, DX11/DX12 | ReShade + [DLSS5-Feeder](https://github.com/jlrouzies-fr/DLSS5-Feeder) (synthesises depth/motion inputs) + RenoDX add-on + your DLSS runtime + model | — |
| Vulkan, DX9/10, 32-bit, anti-cheat | Not routed | — |

Every third-party component is fetched from its publisher's own release page
at run time, pinned by version and SHA-256 in [`components.json`](components.json),
verified before it is unpacked, and never executed by shim-dlss5 (ReShade's setup exe
is only unpacked for its DLL). Games with kernel anti-cheat markers are blocked
unless you type a confirmation.

### Reversible, really

Before an install you read the exact file list. During it, every file that
would be overwritten is backed up first, and every write is recorded *before*
it happens, so even a crash mid-copy is undone at the next start. **Remove**
walks that record backwards and restores the folder byte-for-byte — including
the logs and captures the components themselves write while running. shim-dlss5
never deletes by filename and never touches a ReShade or OptiScaler install it
did not make.

## Install

Download the installer or the portable exe from
[Releases](https://github.com/riccardolardi/shim-dlss5/releases). Hashes are in
`SHA256SUMS.txt`. The binaries are not code-signed yet, so SmartScreen will warn
on first run; verify the hash if that bothers you (it should).

Then: Components → pick your `nvngx_dlssnr.dll` and `renodx-dlss5.addon64`
(and `nvngx_dlss.dll` for games without DLSS), Fetch the components you need,
Scan, open a game, Install. Turn DLSS on in the game's own settings for the
default route — the add-on hooks the game's DLSS pass.

### Hardware notes

The neural pass is heavy. Known from real runs: a 12 GB card running MSFS 2024
at 4K with frame generation *and* the pass after upscaling hung the GPU; the
pre-upscale OptiScaler mode and the default RenoDX route both fit. Some model
builds only initialise on RTX 50; the ReShade or OptiScaler log beside the exe
says so if yours refuses.

## Support

shim-dlss5 is free and will stay free. If it saved you an evening:
**[ko-fi.com/riccardolardi](https://ko-fi.com/riccardolardi)** —
also reachable from the heart in the app.

## Build from source

```sh
npm install
npm run tauri dev        # desktop app with hot reload
npm run tauri build      # target/release/bundle/nsis/shim_<version>_x64-setup.exe
npm test                 # front-end unit tests
cargo test --workspace   # Rust tests; also regenerates src/lib/generated/*.ts
```

Tauri 2 · Rust core (`crates/core`, OS-independent, tested on Windows, macOS
and Linux) · Windows glue (`crates/win`) · React 19 + Vite + Tailwind 4. On
macOS/Linux the app runs for UI work; scanning and installing are Windows-only
and say so. See [PLAN.md](PLAN.md) for scope, decisions and the handoff log.

```
crates/core/   models, discovery, analysis, routing, components, install journal
crates/win/    registry, Authenticode, elevation (Windows only)
src-tauri/     Tauri shell: state + commands, nothing else
src/           React app: screens, components, store, i18n, generated types
```

## Licence

DLSS and NVIDIA are trademarks of NVIDIA Corporation. shim-dlss5 is not affiliated with or endorsed by NVIDIA, ReShade, RenoDX or OptiScaler. Provided as is, without warranty; never use it in anti-cheat-protected multiplayer games.

MIT. Third-party components shim-dlss5 downloads keep their own licences
(ReShade BSD-3, DLSS5-Feeder MIT, OptiScaler and its forks GPL-3); none of them
is bundled.
