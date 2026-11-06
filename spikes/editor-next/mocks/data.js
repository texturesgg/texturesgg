// Mock data. Fighter names and file codes are the game's; skins, authors and
// the non-tgg mods are made up. tgg.* mods are the real ones in ~/dev/texturesgg/mods.
window.DATA = (function () {
  const F = (name, code, base, accent, eye) => ({ name, code, palette: { base, accent, eye: eye || "#bff3ff", shade: "#2a2622", rim: "#f5c84b" } });
  const fighters = [
    F("Fox", "PlFx", "#c98a4b", "#e9e3d6", "#9be7ff"),
    F("Falco", "PlFc", "#4f6fb8", "#d94c3d", "#ffe08a"),
    F("Marth", "PlMs", "#3d4f9a", "#e8e2d2"),
    F("Sheik", "PlSk", "#5866a6", "#e6e0d0", "#ff8fa3"),
    F("Captain Falcon", "PlCa", "#3e4fa8", "#f2c94c", "#ffd98a"),
    F("Jigglypuff", "PlPr", "#f2a7c3", "#5ec2e6"),
    F("Peach", "PlPe", "#f1a2bf", "#f5d36b"),
    F("Ice Climbers", "PlPp", "#7b5cc9", "#f4f1ea"),
    F("Samus", "PlSs", "#d3742f", "#5fbf6a", "#7cff9b"),
    F("Pikachu", "PlPk", "#f2cf3f", "#c8402f"),
    F("Luigi", "PlLg", "#3f9a4f", "#3655a8"),
    F("Dr. Mario", "PlDr", "#ece7dc", "#5a7bd1"),
    F("Ganondorf", "PlGn", "#4a3c38", "#b5862d", "#ffcf7a"),
    F("Yoshi", "PlYs", "#5fbf5a", "#e85b46"),
    F("Mario", "PlMr", "#c9403a", "#3958b0"),
    F("Donkey Kong", "PlDk", "#7a4a2c", "#d23b33"),
    F("Link", "PlLk", "#4f9a46", "#e2c58c"),
    F("Young Link", "PlCl", "#5fae4e", "#e7c99a"),
    F("Roy", "PlFe", "#c34a3b", "#2d3c78"),
    F("Mewtwo", "PlMt", "#c9bfd3", "#8a5bb8", "#c79bff"),
    F("Zelda", "PlZd", "#d9b6d6", "#c9a33f"),
    F("Ness", "PlNs", "#d4433b", "#f2c94c"),
    F("Kirby", "PlKb", "#f3a8bf", "#d64545"),
    F("Bowser", "PlKp", "#5f9a3c", "#e2a03b", "#ff7a4d"),
    F("Mr. Game & Watch", "PlGw", "#1c1c1c", "#3a3a3a", "#ffffff"),
    F("Pichu", "PlPc", "#f5d554", "#ef8a8a"),
  ];

  const S = (fighter, name, author, base, accent, eye, extra) => Object.assign({ fighter, name, author, palette: { base, accent, eye, shade: "#24211e", rim: "#f5c84b" } }, extra || {});
  const skins = [
    S("Fox", "Ember Fox", "mira", "#b8432d", "#f1d4a0", "#ffb36b", { installed: "Slot 2", changes: ["Body", "Scarf", "Visor"], saves: 1240 }),
    S("Fox", "Glacier", "kestrel", "#7fa8c9", "#eaf3fa", "#a8f0ff", { changes: ["Body", "Visor"], saves: 860 }),
    S("Falco", "Night Shift", "okapi", "#2d2f4f", "#e04f7a", "#7fe3ff", { installed: "Slot 1", changes: ["Body", "Jacket", "Laser"], saves: 2310 }),
    S("Marth", "Ivory Prince", "sable", "#ece4d2", "#c39b3b", "#bfe6ff", { changes: ["Cape", "Armor"], saves: 640 }),
    S("Sheik", "Moth", "pell", "#5b5246", "#c7b38a", "#ffe08a", { changes: ["Wrap", "Eyes"], saves: 1980 }),
    S("Captain Falcon", "Copper Wing", "dune", "#9a5430", "#f2d27a", "#ffcf8a", { changes: ["Suit", "Helmet"], saves: 410 }),
    S("Jigglypuff", "Sorbet", "mira", "#f6c38b", "#7fd0c0", "#2a2a2a", { installed: "Slot 4", changes: ["Body", "Bow"], saves: 990 }),
    S("Peach", "Midnight Bloom", "luz", "#3a2c52", "#e8b44a", "#bfe6ff", { changes: ["Dress", "Crown"], saves: 720 }),
    S("Samus", "Phazon", "orbit", "#2f5f6a", "#b6ff7a", "#d6ff9b", { changes: ["Suit", "Visor"], saves: 1530 }),
    S("Falco", "Sunbird", "kestrel", "#e0a63c", "#3a5fb8", "#ffffff", { changes: ["Body"], saves: 300 }),
    S("Marth", "Obsidian", "pell", "#25232a", "#7c6be0", "#c9b8ff", { changes: ["Cape", "Armor", "Sword"], saves: 1120 }),
    S("Fox", "Paper Crane", "luz", "#efe9dc", "#d23b33", "#2a2a2a", { changes: ["Body", "Scarf"], saves: 2780 }),
  ];

  const M = (o) => Object.assign({ tags: [], hooks: [], files: [], imports: [], installed: false, enabled: false }, o);
  const mods = [
    M({ id: "tgg.replay-player", name: "Replay player", version: "0.1.0", author: "textures.gg", tags: ["Tools"], hue: 205, glyph: "play",
      desc: "Watch .slp replays inside the game, with pause, frame step and scrubbing.", size: "412 KB", saves: 8420,
      hooks: ["Scene_ReplayLoad", "Pad_ReadInputs"], imports: ["tgg.core/input"], featured: true, installed: true, enabled: true }),
    M({ id: "tgg.ucf-legacy", name: "UCF (Slippi 3.16 and earlier)", version: "0.1.0", author: "textures.gg", tags: ["Gameplay"], hue: 32, glyph: "pad",
      desc: "Dashback and shield drop behave the same on every controller, matching Slippi 3.16 and earlier.", size: "96 KB", saves: 12900,
      hooks: ["ftCo_Dash_CheckBack", "ftCo_GuardOff_Check"], installed: true, enabled: true }),
    M({ id: "tgg.frozen-stadium", name: "Frozen Stadium", version: "0.1.0", author: "textures.gg", tags: ["Stages"], hue: 190, glyph: "snow",
      desc: "Pokémon Stadium stays on its neutral form. No transformations.", size: "38 KB", saves: 9870,
      hooks: ["grStadium_TransformUpdate"], installed: true, enabled: true }),
    M({ id: "tgg.stadium-preload", name: "Preloaded Stadium transformations", version: "0.1.0", author: "textures.gg", tags: ["Stages"], hue: 140, glyph: "layers",
      desc: "Loads every Stadium transformation up front, so changing forms never stalls.", size: "54 KB", saves: 2210,
      hooks: ["grStadium_TransformUpdate", "grStadium_LoadForms"], installed: true, enabled: true, update: "0.2.0" }),
    M({ id: "tgg.stadium-monitor", name: "Camera-free Stadium monitor", version: "0.1.0", author: "textures.gg", tags: ["Stages", "Visual"], hue: 260, glyph: "monitor",
      desc: "The big screen on Pokémon Stadium shows the whole fight instead of a camera feed.", size: "61 KB", saves: 1730,
      hooks: ["grStadium_MonitorDraw"] }),
    M({ id: "tgg.neutral-spawns", name: "Neutral spawns", version: "0.1.0", author: "textures.gg", tags: ["Gameplay"], hue: 50, glyph: "pin",
      desc: "Players start on tournament neutral spawn points on every legal stage.", size: "22 KB", saves: 11050, hooks: ["Stage_GetSpawnPoint"] }),
    M({ id: "tgg.offscreen-damage", name: "Offscreen damage", version: "0.1.0", author: "textures.gg", tags: ["Visual"], hue: 0, glyph: "eye",
      desc: "Keeps a fighter's damage readable while they're off the edge of the screen.", size: "31 KB", saves: 4310, hooks: ["Hud_DrawOffscreenBubble"] }),
    M({ id: "tgg.freeze-dead-up-fall", name: "Freeze screen KO fall", version: "0.1.0", author: "textures.gg", tags: ["Visual"], hue: 220, glyph: "pause",
      desc: "A screen KO holds still on the screen instead of sliding down it.", size: "18 KB", saves: 1260, hooks: ["ftCo_DeadUpFall_Anim"] }),
    M({ id: "kestrel.frame-meter", name: "Frame meter", version: "1.3.0", author: "kestrel", tags: ["Training"], hue: 95, glyph: "bars",
      desc: "A strip under each fighter that shows startup, active and recovery frames as they happen.", size: "140 KB", saves: 6400,
      hooks: ["Hud_Draw"], imports: ["tgg.core/hud"], files: ["fonts/meter.dat"] }),
    M({ id: "orbit.hitbox-view", name: "Hitbox view", version: "0.4.1", author: "orbit", tags: ["Training", "Visual"], hue: 330, glyph: "target",
      desc: "Draws hitboxes, hurtboxes and ledge grab boxes over the fight. Toggle with D-pad up.", size: "88 KB", saves: 5120, hooks: ["Fighter_DrawDebug"] }),
    M({ id: "luz.widescreen", name: "Widescreen 16:9", version: "2.0.0", author: "luz", tags: ["Visual"], hue: 280, glyph: "wide",
      desc: "Renders the fight in 16:9 with the HUD kept where you expect it.", size: "75 KB", saves: 7790, hooks: ["Camera_SetAspect", "Hud_Layout"] }),
    M({ id: "dune.quiet-menus", name: "Quiet menus", version: "0.9.0", author: "dune", tags: ["Menus"], hue: 15, glyph: "menu",
      desc: "Skips the intro movies and remembers the last stage and rules you picked.", size: "44 KB", saves: 3020,
      hooks: ["Menu_Boot"], files: ["MnSlChr.usd"] }),
  ];

  const port = { name: "tgg-melee", version: "0.4.2", target: "Linux x86-64", layout: "melee-ntsc-1.02" };

  return { fighters, skins, mods, port };
})();
