# tgg-ui

The texture editor's theme and components, on [gpui-ce](https://github.com/gpui-ce/gpui-ce).

`Theme` carries the active palette (Gallery or Paper) as a gpui global, and the
`tokens` module holds the spacing, type, radius and density scales. Sizes are
web pixels expressed in rems against a 16 px reference, so changing the window's
rem size zooms the whole UI while display scaling stays the operating system's.

Components: `Button`, `IconButton`, `MenuButton` and menus, `Dialog`, `Tooltip`,
`Tabs`, `Chip`, `Card`, `Slider`, `Breadcrumbs`, `PageHeader`, `Sidebar`,
`Pane` and `PaneColumn` (floating, foldable tool panes), `OptionList`,
`ThumbnailList`, and a title bar that works with and without a platform menu
bar. Keyboard operation and visible focus are part of each control;
`tests/keyboard.rs` covers them.

```bash
# Every component on one page.
cargo run -p tgg-ui --example gallery
```

The tokens mirror the textures.gg site's design system, and the site's tests
fail when the two diverge.
