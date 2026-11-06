# The editor on Rune: plan

The textures.gg desktop app rebuilt on Rune in the Catalog design
(`mocks/b-language.html`), growing to hold the mod loader and browser. The
proof of concept (`app/`) shows hsd-render drawing a fighter on Rune's own
device; this plan is how the real app gets built without losing what
tgg-editor already does well.

## Goals

- Everything tgg-editor does today, then Mods: the same game, library,
  installs, editing and undo, on the same files on disk, so a player can move
  between the two apps while both exist.
- The Catalog design and its motion rules, built once as the app's own
  components.
- No slow work on the UI thread. Reading the disc, hashing slots, loading
  models and installing all happen off it.
- Every screen reachable as a headless scene, and every flow tested through
  Rune's headless runtime.

Not goals for the first release: the web build, Windows packaging (it comes
nearly free; ship it when Linux and macOS are done), new editing features.

## Findings that shape the plan

From reading Rune's source and docs at the pinned rev:

- **Files dropped on the window don't arrive on Wayland** (winit 0.30); they
  do on macOS, Windows and X11. tgg-editor takes drops on every page, and
  Hyprland is Wayland. File it upstream (the first real test of how Rune
  handles issues); until then dialogs cover every drop, and XWayland is the
  fallback to check.
- **The menu bar and native popup menus are macOS only.** Linux keeps an
  in-app menu, as tgg-editor's menu button does today.
- **Custom window chrome is macOS only.** On Linux the choice is the system
  title bar, or a borderless window with our own drag band.
- **There's no per-component state.** Everything lives in the app's state;
  rune-kit slices it with accessor closures (`|s| &mut s.combo`) and keeps a
  `XxxState` per component in the app. We do the same.
- **`.matched(id)`** flies an element from its old rect to its new one when
  one leaves as another mounts: the roster card opening into the stage, as
  the mock does, is built in.
- **Background work has four documented paths**: a `Waker` from `setup` with
  `poll` picking results up, `AppHandle::update` from any thread,
  `rune::Latest<T>` for newest-wins jobs, and `Live`/`Pixels`/`ImageCache` for
  data that skips the view. Slow work starts in `on_ready`.
- **No device-lost callback**; a changed `DeviceKey` is the only signal. The
  viewport rebuilds on it, as the PoC does.
- **No programmatic focus** for elements other than text fields. Fine for
  now; noted for keyboard flows.

## Crates

```text
crates/
  hsd-render     wgpu 29 on main (tgg-editor); wgpu 30 on the studio branch
  tgg-app        new: what the app does, with no UI (moved from tgg-editor)
  tgg-ui         unchanged; tgg-editor's components on gpui-ce
  tgg-editor     unchanged; retires when tgg-studio replaces it
  tgg-studio     new: the app on Rune, and its name from here on
```

**Everything on the studio branch.** hsd-render borrows its host's device, so
it must use its host's wgpu: 29 for gpui-ce, 30 for Rune. `main` keeps wgpu 29
and tgg-editor ships from it untouched. The studio branch carries hsd-render
on wgpu 30 (three call sites, and 32-bit float depth), tgg-app and
tgg-studio, merges `main` in often, and lands when tgg-studio replaces
tgg-editor. tgg-editor and tgg-ui aren't workspace members on the branch: they
can't build against wgpu 30, and gpui-ce and Rune can't share one lockfile.

**tgg-app, no UI.** The gpui-free parts of tgg-editor, copied rather than
moved, so tgg-editor on `main` is untouched and merges stay clean: `game` (the
player's disc, slots, installs into it), `library`, `install` (history and
undo), `settings` (the same `editor.json`), `disk`, `ids`, `log`, `net`,
`update`, `moves`, what a report sends, and the external-editor watcher's
logic. A change to one of these on `main` is carried into tgg-app by hand
until the switch, when tgg-editor and its copies go.

**tgg-studio.** A member of the core workspace on the branch, depending on
tgg-app, hsd-render, melee-dat, dat-edit, gc-iso and tgg-mod, and on Rune at a
pinned `dev` rev.

## Inside tgg-studio

```text
src/
  main.rs        arguments, the App builder, scenes
  state.rs       App: the whole state, and how it starts
  jobs.rs        background work: workers, results into state via poll
  shell.rs       the window's frame: sidebar, title row, the place (view)
  window.rs      the window itself: closing it
  nav/          places, the subject, overlays, Escape, "up"
  ui/            the Catalog components: tokens, fonts, sidebar, title row,
                 plate, pane, pane column, rows and tables, buttons, status
                 marks, timeline, dialog frame
  stage/         the live model and the viewport compositor
  melee/         the roster and a fighter or stage opened
  library/       skins, review of new skins, installs
  editor/        the document, textures, colors, files, external edits
  mods/          browse, installed, updates, conflicts (tgg-mod)
  welcome/, settings/, report/
```

**State, controller and view stay apart.** Each feature folder owns its slice
of the app and splits it three ways:

- `mod.rs`: its state, plain data with no behaviour, and the module's spec.
- `actions.rs`: the controller. Functions over `&mut App` (and `Tx` when they
  open a dialog or set a timer) that change state, calling tgg-app for what
  the app does; no elements.
- `view.rs`: a function of `&App` returning elements. It reads, never
  writes; a press names an action, with nothing but the wiring inline.

What the app does (games, slots, the library, installs) lives below all
three, in tgg-app, with no UI at all. `ui/` holds components that know
nothing of the app's state or actions; views wire them. Files stay under
800 lines.

## State

One `App` value, read by the view and changed only by handlers between
frames. Its shape:

```rust
struct App {
    settings: Settings,          // the one copy; written back debounced
    game: Option<GameState>,     // the player's disc and what each slot holds
    library: LibraryState,       // skins, and their changes
    renders: Renders,            // costume renders by key, as Pixels
    nav: Nav,                    // place, subject, overlay, sidebar
    stage: Stage,                // the one live model both Melee and Edit show
    editor: Option<Document>,    // a costume open for editing
    mods: ModsState,
    jobs: Jobs,                  // handles to background work
    notice: Option<Notice>,
}
```

The rules:

- **The view only reads.** Components are functions of `&App` (or a slice of
  it) returning `El`. Handlers are plain functions in the module that owns
  the slice: `fn open_fighter(app: &mut App, fighter: Character, tx: &mut Tx)`.
- **One live model.** `Stage` holds the model showing (its bytes, playback,
  camera, highlight, edited textures, shared-model spawns). Opening a fighter
  fills it; Edit textures keeps it and adds the document; leaving the editor
  reloads it from the disc and keeps the camera and move, as tgg-editor's
  hand-off does today. The compositor owns only GPU resources and reads
  `Stage` each frame.
- **Viewport events through state.** Picks and refused textures land in a
  queue on `Stage`; `poll` drains them into handlers, the way tgg-editor
  defers them out of paint today.
- **Continuations are data.** The unsaved-changes prompt keeps
  `Pending { Close | Leave(Destination) | Open(path) }`; saving to the library
  and then leaving is sequential code over `&mut App`, with no events
  bouncing between owners.
- **No caches pushed into pages.** Renders, slot states and skin changes live
  once in `App`; every screen reads them.
- **Background results come back one way.** Workers send results down a
  channel and wake the window; `poll` applies them and says whether anything
  showed changed. Newest-wins jobs (loading the model for a selection) use
  `Latest`. Timers (the external editor's 500 ms check, debounced settings
  writes) are `wake_after` deadlines in state, not tasks.
- **Undo stays dat-edit's.** `TextureDocument` keeps the bytes and the undo
  stack, as now.
- **Files on disk don't change shape.** Settings, library, install history,
  render cache and logs keep their paths and formats.

## Screens

The Catalog layout, on tgg-editor's shell:

- **Sidebar of places**: Melee, Library, Mods, recent edits with their
  renders, Settings at the foot. Hides whole; its toggle stays put.
- **Title row**: breadcrumbs and the one action the context needs; empty on
  a top-level page, which says its own name.
- **Melee**: the roster (Fighters, Stages) as plates. A plate opens into the
  stage (`.matched`), with the pane column (Costumes, Shared, Moves) and the
  timeline. Escape goes back into the plate.
- **Edit**: the same stage with the editor's panes (Textures, Texture,
  Colors, Moves) and its title row (where it came from, saved or not, Save
  and Save and install).
- **Library**: skins as plates grouped by fighter, tabs for in your game and
  not; adding skins opens Review.
- **Mods**: the list with the details docked in the pane column; conflicts
  and missing imports stated plainly, from tgg-mod.
- **Overlays**: Settings, Review, Report and the unsaved-changes prompt as
  modals (`El::modal`); menus and selects from rune-kit restyled.
- **Welcome**: finding the player's disc.

**Components.** The pieces that make the look (sidebar, title row, plate,
pane and pane column, rows, status marks, timeline, buttons) are ours, built
from `El` with the design tokens. Plumbing-heavy pieces come from rune-kit
with a `Kit` built from our palette: text fields, menus and context menus,
selects, dialogs' focus handling, tables and trees if we need them.

## Conventions

- tgg-editor's: every module opens with a plain-English `//!` spec in the
  player's words; item docs are short sentences; full-word names;
  `pub(crate)` by default; `thiserror` errors whose messages finish a
  sentence; failures the player should see become a notice, the rest are
  logged; edition 2024 and the workspace lints.
- Rune's: pin `dev` revs; slow work in `on_ready`; `poll` true only on a
  visible change; keys prefixed by feature (`"melee/roster/PlFx"`); handlers
  capture ids, never indices; bounded caches; numbers from `--profile perf`.
- Tests: flows through `rune::Runtime` (open a fighter, edit, undo, save,
  install, the unsaved prompt, a mod conflict), on hand-written DATs and test
  discs as today; real-disc tests behind `melee-iso`; a scene for every
  screen, rendered in CI with `--snap` so a change to a screen is a diff.
- Commits: `feat(tgg-studio): …` in tgg-editor's sentence style.

## Order of work

Each step ends working, tested and committed.

0. **macOS and Windows: later, in their own sessions.** Linux first. What the
   Mac showed so far: Rune's native Metal backend refused `Depth24Plus`
   (hsd-render now uses `Depth32Float`), then refused depth-stencil
   attachments altogether, so hsd-render's pass can't run on it as is.
   Start from forcing wgpu on macOS (`App::gpu_backend(GpuBackend::Wgpu)`);
   the `RUNE_GPU_BACKEND=wgpu` run didn't open a window, cause not yet seen.
1. **tgg-app**: tgg-editor's UI-free modules copied into their own crate,
   with their tests passing there.
2. **The shell**: tgg-studio in the workspace; tokens, fonts and the core
   components; places, title row, Welcome; scenes and the first flow tests.
3. **Melee**: background slot states and renders; the roster; a fighter
   opened (viewport with picking, moves, shared models and their spawns,
   timeline); the plate-to-stage motion.
4. **Library and installs**: skins, Review, install and undo install,
   restoring the original.
5. **Edit**: the document, textures (PNG in and out, external editor),
   colors, undo, save and save and install, the unsaved prompt.
6. **Mods** on tgg-mod: browse textures.gg, install into the port, turn on
   and off, conflicts, updates.
7. **The rest and the switch**: Settings, Report, the update check,
   packaging (`dist/`, `scripts/`), a parity checklist against tgg-editor;
   then tgg-studio ships in tgg-editor's place and tgg-editor, tgg-ui and
   their copies retire. The player's files (`editor.json`, the library,
   install history) stay where they are, so everything carries over.

## Decided

1. The app is tgg-studio, for good: the app is outgrowing "editor".
2. Linux windows are borderless, with the app's own drag band and window
   buttons, as the design draws them.
3. While Rune can't take file drops on Wayland, dialogs cover every drop;
   the issue is filed upstream.
