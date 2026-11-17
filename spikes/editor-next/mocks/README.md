# tgg-editor next: HTML mocks

Throwaway HTML/CSS/JS mockups for a re-imagined editor, to iterate on looks
before building anything in Rune. No build step, no dependencies.

```bash
xdg-open stage.html          # or open it in any browser
```

`stage.html#mods`, `#mods:installed`, `#skins`, `#editor` and `#game:cmdk`
open straight onto a view. Keys `1`–`4` switch views, `Ctrl K` opens the
command palette.

- `tokens.css`: the site's Gallery tokens plus desktop extras (glass, spring easing).
- `figure.js`: a stand-in for the hsd-render viewport (a shaded mannequin on a canvas).
- `data.js`: mock fighters, skins and mods (the `tgg.*` mods are the real ones).
- `stage.*`: direction A, "Stage": the live fighter is the room and the UI floats over it.
- `b-*`: direction B, "Catalog": typography and hairline rules, data in mono,
  the fighter as a framed plate, on tgg-editor's shell. Start with `b-language.html` (the rules), then
  `b.html` (the app). Hash flags, comma-separated: `fox`/`falco` open a fighter, `mods`, `installed`,
  `library`, `nosb` (sidebar hidden), `paper`, `still` (no motion, for screenshots).
