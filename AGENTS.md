# AGENTS.md

Rules for agents working in this repository. The workspace guide in
`~/dev/AGENTS.md` applies too; this file overrides it where they differ.

This repository is `texturesgg/texturesgg` (public): the Melee DAT crates, the
`tgg` command line, and the desktop app tgg-studio. The "Platform overview"
section covers every textures.gg repository; `site` and `tgg-melee` point here
for it.

## Platform overview

### Repositories and folders

Everything lives under `~/dev/texturesgg`. That folder is not a git repository:
run git, cargo, pnpm and `direnv exec .` from inside one of the repositories.

| Path | Remote | What |
| --- | --- | --- |
| `core/` | `texturesgg/texturesgg` (public) | Rust crates, `tgg` CLI, tgg-studio |
| `site/` | `texturesgg/site` (public) | textures.gg on Cloudflare, the mod registry, the meet Worker |
| `tgg-melee/` | `texturesgg/tgg-melee` (private) | the native Melee port with the mod runtime built in |
| `mods/` | `texturesgg/mods` (public) | official mods, one folder per mod |
| `external/` | none | reference clones: `melee` (decomp), `slippi-ssbm-asm`, `aurora`, `melee-unlocked`, `melee-party`, `zeron` and others |
| `tgg-melee-goals/` | local git | current launch, handoff, and active design docs (archived docs in git history) |
| `slp-corpus/` | none | Slippi replays for the port's replay checks |

- Read the decomp in `external/melee` or Slippi's codes in
  `external/slippi-ssbm-asm` before guessing how the game behaves. Clone new
  long-lived references into `external/`, not a scratch folder.
- `tgg-melee-goals/launch-plan.md` is the shared launch state; active docs
  (`HANDOFF.md`, `copy-pass.md`, `competition-promo.md`, `public-audit.md`,
  `threat-model.md`, `matchmaking.md`, `sets-model.md`, `player-counts.md`)
  sit beside it. Read the relevant one before changing that area, and record
  new decisions there.
- The clean Melee disc on this machine is
  `~/Games/Slippi/Super Smash Bros. Melee (USA) (En,Ja) (Rev 2).iso`.

### What the platform is

- textures.gg is a real site with real users (skin downloads every day). The
  project exists to build good tech and run a complete agentic delivery loop,
  not to chase growth. Frame suggestions around technical merit.
- It is free and open source. The only money is for creators (Ko-fi or
  Patreon links on their pages). Don't propose donation asks, paid tiers, or
  charging makers for builds. When costs come up, prefer cutting churn (scripts
  over native builds, fewer rebuilds).
- The pieces: tgg-melee (the port), mods built against its SDK, the registry
  on the site (publish, review, build, serve), the `tgg` CLI, and tgg-studio
  (the player's app: play, mods, sets, skins, replays).
- Launch target is 2026-12-01. tgg-studio, code mods and the site's `code-mods`
  flag launch together. Don't ship one early, and don't add an in-app flag for
  code mods.

### Identity and licensing

- Company: "Saavylab LLC" (lowercase l). Author contact:
  `jayson@saavylab.com`. X handle: `@texturesgg`. Use these exactly in
  notices, metadata, footers and legal pages. Never publish the owner's
  personal email.
- Code is GPL-3.0-or-later, including the official mods.
- Never mention Nucleus or other competing tools in tracked files.
- Report and contact address: `contact@textures.gg`; security reports go to
  `security@textures.gg`.

### Merge, deploy and release policy

| Repo | Changes land by | Agents may | Owner only |
| --- | --- | --- | --- |
| core | branch + PR | merge their own PR once required checks are green | `app-v*` and `cli-v*` release tags, crates.io publishing |
| site | branch + PR | merge their own PR once required checks are green | production data and admin writes, Cloudflare secrets and resources, the `code-mods` flag |
| tgg-melee | commits on `main` | commit or merge locally into `main` after the gate, then push | `v*` release tags |
| mods | commits on `main` | commit to `main` and push when CI is green | per-mod release tags, registry publishing and approvals |

- Don't merge a PR the owner asked to review or hold. A hold given in one
  session is lifted only by the owner in that session.
- Merging to site `main` deploys production (see `site/AGENTS.md`).
- Anything new that costs money per use needs the owner first.
- Don't raise tagging or publishing as an ask unless a real test or the launch
  needs it.
- No long-lived integration branches.

### Mod platform rules

- The port is a faithful Melee 1.02 plus API points. Every change to the game
  is a mod people choose to load, published through the registry. When a
  feature needs port support, add hooks or API to the port and build the
  feature as an official mod. Don't propose new built-in modules. The one
  exception is the fighter and stage registries.
- Mods reuse the game's own menus and screens through the API, as Slippi does.
  Custom screens only where the game has none.
- Luau scripts are the default way to make a mod; native C is the exception.
  When someone proposes a native mod, first ask whether its need belongs in
  the curated layer (`tgg-melee/docs/curated.md`) so scripts can do it.
  Infrastructure (netplay, http) stays in C, and scripts get Luau APIs on top
  of it. A script never changes a match through a native mod's functions,
  only through the curated table.
- Mod classes: gameplay, display, offline. Non-gameplay mods must never block
  two players from netplay. `display` is assigned by review, never declared;
  `offline` may be declared because the port switches those mods off online.
- One mod per behavior (one per Slippi rule, not a "rules" mod). Grouping is
  the platform's job, through sets.
- "Set" is the only word for a list of mods: curated, community, or a
  player's own. Never "setup", "collection" or "profile" in player-facing
  text. Sets follow the latest versions of their mods. Skins travel with the
  player, not in a set.
- Versions: a replay or netplay session records the exact mod set (id,
  version, built hash, gameplay settings). Any gameplay change is a new
  version. A rule variant is a version of the same mod, not a new mod. The
  registry keeps every version. Dependencies use version ranges.
- API 1.x: add new API as a minor with `since`; never change a stable 1.0
  shape; things still moving go behind the experimental guard
  (`tgg-melee/docs/tiers.md`).
- Player counts: no cap of their own in rooms, queues or files. A mode's
  players, the netplay build's limit (4) and the meeting point's ceiling (8)
  stay separate, and a room is the smallest. Use lists and a `max`, never a
  two- or four-player shape or the word "opponent".
- Native code trust: don't widen automatic fetching of native mods or loosen
  native review. Third-party native mods are allowed with review and the
  per-publisher prompt.
- Distribution always goes publish, review, registry build. GitHub Actions in
  `tgg-melee` and `mods` are development checks only. The registry's builder
  gets minimal fixes until its planned rewrite after launch.
- Every port release changes the layout id. Rebuild the registry for the new
  layouts before a port release becomes the latest.
- Netplay and matchmaking: peer-to-peer rollback with STUN and hole punching,
  a capped TURN relay as fallback, no authoritative game server. The meeting
  point is the site's Worker at `meet.textures.gg`. Networking for mods lives
  in the `tgg.http` mod, and the account token never enters the game (it gets
  a short-lived scoped ticket). Unranked queues follow Slippi's current
  competitive stage rules.
- Never use Slippi's servers or services without the Slippi team's blessing.
- Official mods (`texturesgg/mods`): one folder per mod, tags
  `<mod-id>-<version>`, `ci/build-all.sh <sdk>` builds them all, CI builds
  against tgg-melee main's SDK at `dl.textures.gg/tgg-melee/main/`. It is
  public: check new files for secrets, game data and third-party credit
  (`CREDITS.md`) before pushing. Rerun its `tools/fuzz` after changing
  netplay's packets, the replay reader or `tgg.http`.
- Training mods each stand alone; a separate menu mod only bundles them in
  the native menus. Drills give an empty stage, a clean screen and save-state
  resets.
- Skins as mods have ids `skin.<lowercase file name>`; the registry reserves
  the `skin.` prefix.

### Product and copy

- The first five minutes are the product. Design onboarding end to end:
  useful at once, no bulk setup, steps that resume where they failed, errors
  in the player's terms. Don't reduce a UX failure to its narrowest fix.
- Make things friendly by making them easy, not by adding explanatory text.
  Keep dialogs terse and UI strings whole (no concatenated fragments) so they
  can be localized later.
- Player-facing copy sounds like the owner: plain, casual, low-key, normal
  capitalization. `tgg-melee-goals/copy-pass.md` holds the approved rules and
  samples. Errors are two short sentences; no colons or semicolons inside
  sentences; say "you", "download". Say literally what a thing does; no
  scene-setting ("Just you on the stage").
- tgg-studio serves two games: the player's own ISO (skins, played in Slippi)
  and tgg-melee (mods, replays, online). Every page and action makes clear
  which game it is about.

### Machines

- Mac mini (M4, macOS beta): `tailscale ssh saavy@saavys-mac-mini`, fish
  shell (wrap commands in `bash -lc '...'`). No Nix; Homebrew toolchain. Use
  it for manual Mac checks. It is never a CI or registry builder.
- MacBook Pro: `tailscale ssh jayson@jaysons-macbook-pro`. The owner's
  machine: use it only when offered, and touch nothing outside
  `~/dev/saavylab`. In its fish shell, source
  `/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh` before
  `direnv exec . CMD`.
- Windows 11 VM on this PC: `~/vms/windows-11.conf` (quickemu), ssh on host
  port 22220 as `Quickemu`. It has the VC++ runtime, CMake, Ninja, GCC, Git and
  a scheduled task `tggjob` for running in the desktop session. Use it for
  real-Windows checks instead of saying there is no Windows machine.

## This repository

### Layout and commands

`README.md` lists the crates and the commands; the CI is
`.github/workflows/ci.yml`.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p hsd-render -p melee-dat -p dat-edit --no-default-features \
  --target wasm32-unknown-unknown -- -D warnings
cargo test --locked --workspace
cargo run --release -p tgg-studio -- --iso PATH
cargo build --release -p tgg-cli          # the tgg binary, in target/release/tgg
```

- Disc tests: `TGG_MELEE_ISO=<iso> cargo test -p melee-dat -p tgg-studio
  --features melee-dat/melee-iso,tgg-studio/melee-iso`. CI only compiles
  them, so run them yourself when a change touches what they cover.
- The dev shell has no Node. For a scripted edit that needs it, run
  `direnv exec ../site node`.
- `tgg-mod` relies on the SDK's CMake contract in
  `tgg-melee/docs/package-format.md` ("What tgg relies on"). Change both
  together.
- Design docs for the CLI and the app's mod handling: `docs/mod-distribution.md`,
  `docs/mod-profiles.md`, `docs/sets.md`.
- A mod in `texturesgg/mods` is published from its own history: clone a
  `git subtree split -P <mod>` into a temp repo and run
  `tgg mod publish` there.

### Crates

- Every crate owns one concept. Put code where its concept lives, and say so
  when code is about to land in the wrong crate instead of building on it.
  `dat-parser` is generic HSD and read-only; Melee knowledge goes in
  `melee-dat`; writing goes in `dat-edit`; `hsd-render` is Melee-free.
- Naming: `Hsd*` types in `dat_parser::hsd`, `Melee*` for what a host holds in
  `melee-dat`, parts named by subject (`Stage…`, `Fighter…`, `Costume…`).
  Modules with children are `dir/mod.rs`. Env vars start with `TGG_`.
- The libraries are public reading material: clear names for what a thing is
  now, a README per crate for an outside reader. Public renames that are a
  matter of taste are the owner's call: propose one, then apply it everywhere
  in one change.
- `hsd-render`, `melee-dat`, `dat-edit` and the crates below them build for
  `wasm32-unknown-unknown`; the site runs them. Use rayon freely in app-only
  crates; in crates the site builds, keep a serial path behind
  `cfg(target_family = "wasm")`, as `gx_texture::encode` does.
- Crates on crates.io are permanent. The site pins the published versions
  (0.2.0), so a change it needs means a new release, which the owner
  publishes.

### Tests and the renderer

- Tests build their bytes by hand (`tgg-app/src/test_dat.rs`, inline bytes in
  `dat-parser`). No game files, community DATs or extracted data as fixtures.
  The only game input is the clean disc named by `TGG_MELEE_ISO` (or `--iso`),
  and a test that needs it fails loudly without it.
- Rendering changes run the pixel regression from the README: predict which
  cases change, run it with `--software`, look at the captures in
  `target/pixel-regression/`, then `--update` and list the changed cases in
  the PR. Baseline hashes (`crates/hsd-render/pixel-baseline.json`) are from
  lavapipe and only compare on that adapter.
- When counting test failures, count result lines (`grep "^test result" |
  grep -vc " 0 failed"`) rather than reading a truncated list.
- If tgg-studio finds no GPU adapter on the NixOS desktop, run with
  `VK_LOADER_DEBUG=error,driver`. A `GLIBC_x.yy not found` from the Mesa
  driver means this flake's nixpkgs is older than the system's
  (`~/nixos-desktop`): update the lock to match. Judge by the loader log, not
  by the process staying up.

### Melee facts that shape features

- Visible color is not always a texture. Stage and fighter color often comes
  from vertex colors (direct, inline in display lists), material colors or
  TEV constants. Editing features should show where a surface's color comes
  from and offer an edit there.
- Fox's colors are all material diffuse under his textures, so recoloring
  him shows nothing. Use the `stadium-recolored` scene to see the Colors pane
  work.
- `melee-dat` refuses fighter animations with motion flags `& 0x1c0`
  (`PartialAnimation`). These are not partial-body blends: on a state change
  the game snaps one bone's pose from the animation and still plays the clip
  over the whole body. The binder can accept them and play the clip; check
  the decomp's fighter animation code before changing it.

### tgg-studio

- Built on Rune (`anomalyco/rune`, pinned in `Cargo.toml`). Before designing
  chrome or a component, look at how Hex (`gh repo clone anomalyco/hex`,
  `src/rune_ui/`) does it, and follow it unless there's a reason not to. Say
  when a choice copies Hex.
- Keep state, actions and view apart. Each feature folder has `mod.rs` (plain
  state), `actions.rs` (handlers over `&mut App`, calling `tgg-app`) and
  `view.rs` (pure `&App` to element; a press only names an action). Business
  logic lives in the UI-free `tgg-app` crate. `ui/` knows nothing of app
  state.
- `src/ui/` is the design system: `tokens.rs` plus components in
  `controls.rs`, `surfaces.rs`, `marks.rs`. Screens use tokens and components
  only; a screen's own geometry is a named constant. `ui::tests::screens_use_the_tokens`
  enforces it.
- Keyboard shortcuts go through `src/keys.rs`: `BINDINGS` maps a chord to a
  `Command` and a scope, `run` dispatches it, and `keys::hint` labels buttons
  and tooltips. Never match keys ad hoc in a feature.
- Check UI work headless with `--snap out.png --scene NAME` (scenes are listed
  in `main.rs` and deterministic). Verify a UI refactor with before and after
  snaps of every scene.
- The code is rustfmt-formatted and CI checks it. Edit Rust with the editor
  tool, not `sed` or `perl` substitutions.
