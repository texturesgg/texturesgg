(function () {
  const { fighters, skins, mods, port } = window.DATA;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const icon = (n, cls = "ico") => `<svg class="${cls}"><use href="#i-${n}"/></svg>`;
  const fmt = (n) => (n >= 1000 ? (n / 1000).toFixed(n >= 10000 ? 0 : 1) + "k" : String(n));

  const S = { view: "game", fighter: 0, slot: 1, modTab: "discover", mod: "tgg.replay-player", filter: "All", move: "idle", playing: true, frame: 0 };

  // ---------------- navigation ----------------
  function go(view) {
    S.view = view;
    $$(".rail button").forEach((b) => b.classList.toggle("on", b.dataset.view === view));
    $$(".view").forEach((v) => v.classList.toggle("on", v.id === "v-" + view));
  }
  $$(".rail button").forEach((b) => b.addEventListener("click", () => go(b.dataset.view)));
  document.addEventListener("click", (e) => { const g = e.target.closest("[data-go]"); if (g) go(g.dataset.go); });

  // ---------------- game ----------------
  const gameFig = Figure.mount($("#game-canvas"), { palette: fighters[0].palette, interactive: true, spin: 0.25, x: 0.5, scale: 0.7, floor: 0.9, yaw: 0.6 });
  const skinFor = (f) => skins.find((s) => s.fighter === f.name && s.installed);
  const variants = (f) => {
    const s = skinFor(f);
    const base = [f.palette.base, "#b8433a", "#3f5fb0", "#4c9a50", "#e9e3d6", "#2a2724"];
    return base.map((c, i) => ({ color: s && s.installed === "Slot " + (i + 1) ? s.palette.base : c, skin: s && s.installed === "Slot " + (i + 1) ? s : null }));
  };
  function paletteFor(f, slot) {
    const v = variants(f)[slot];
    if (v.skin) return v.skin.palette;
    return Object.assign({}, f.palette, { base: v.color });
  }
  const SHARED = {
    Fox: [["Blaster", "target", "Original"], ["Shine", "snow", "Original"], ["Laser", "bars", "Original"]],
    Falco: [["Blaster", "target", "Original"], ["Shine", "snow", "Original"], ["Laser", "bars", "Changed by Night Shift"]],
  };

  function renderRoster() {
    $("#roster").innerHTML = fighters.map((f, i) => `<button class="fighter ${i === S.fighter ? "on" : ""}" data-i="${i}">
      <i class="${skinFor(f) ? "mod" : ""}" style="background: radial-gradient(70% 70% at 35% 30%, ${f.palette.base}, color-mix(in oklab, ${f.palette.base} 45%, #000))"></i><span>${f.name}</span></button>`).join("");
  }
  $("#roster").addEventListener("click", (e) => { const b = e.target.closest(".fighter"); if (b) selectFighter(+b.dataset.i); });

  function selectFighter(i, slot) {
    S.fighter = i;
    const f = fighters[i];
    const s = skinFor(f);
    S.slot = slot ?? (s ? +s.installed.slice(5) - 1 : 0);
    const h = $("#g-name"); const changed = h.textContent !== f.name; h.textContent = f.name;
    if (changed) { h.classList.remove("swap"); void h.offsetWidth; h.classList.add("swap"); }
    h.style.fontSize = f.name.length > 12 ? "72px" : "96px";
    $("#g-code").textContent = `${f.code} · 6 costumes`;
    $$(".fighter").forEach((b) => b.classList.toggle("on", +b.dataset.i === i));
    applySlot();
  }
  function applySlot() {
    const f = fighters[S.fighter], pal = paletteFor(f, S.slot);
    gameFig.set({ palette: pal });
    $("#v-game .stage-bg").style.setProperty("--tint", pal.base);
    renderInspector();
  }

  function renderInspector() {
    const f = fighters[S.fighter], vs = variants(f), v = vs[S.slot];
    const shared = SHARED[f.name] || [[`${f.code}.dat`, "cube", "Original"]];
    $("#g-inspector").innerHTML = `
      <div class="sec">
        <div class="sec-t">Costumes <span class="mono">${S.slot + 1} / 6</span></div>
        <div class="slots">${vs.map((x, j) => `<button class="slot ${j === S.slot ? "on" : ""}" data-slot="${j}"><i style="background: radial-gradient(70% 70% at 35% 30%, ${x.color}, color-mix(in oklab, ${x.color} 40%, #000))"></i>${x.skin ? '<span class="mod-dot"></span>' : ""}</button>`).join("")}</div>
        ${v.skin ? `<div class="installed">
            <div class="sw" style="background: linear-gradient(135deg, ${v.skin.palette.base}, ${v.skin.palette.accent})"></div>
            <div class="grow"><b>${v.skin.name}</b><span class="muted">by ${v.skin.author}</span></div>
            <span class="tag gold">Installed</span>
          </div>` : `<div class="installed">
            <div class="sw" style="background: linear-gradient(135deg, ${v.color}, #2a2724)"></div>
            <div class="grow"><b>Original costume</b><span class="muted">As the game ships it</span></div>
          </div>`}
        ${v.skin ? `<div class="row gap wrap" style="margin-top:10px">${v.skin.changes.map((c) => `<span class="tag">${c}</span>`).join("")}</div>` : ""}
      </div>
      <div class="sec">
        <div class="sec-t">Moves <a>All 312</a></div>
        <div class="row gap wrap">${["idle", "shine", "kick"].map((m) => `<button class="chip ${S.move === m ? "on" : ""}" data-move="${m}">${{ idle: "Idle", shine: "Down special", kick: "Forward smash" }[m]}</button>`).join("")}</div>
      </div>
      <div class="sec">
        <div class="sec-t">Shared by every costume</div>
        <div class="list-rows">${shared.map(([n, ic, st]) => `<div class="lrow"><span class="ib">${icon(ic)}</span><div class="grow">${n}<small>${st}</small></div>${st === "Original" ? "" : '<span class="tag gold">Changed</span>'}</div>`).join("")}</div>
      </div>
      <div class="actions">
        <button class="btn primary" data-go="editor">${icon("edit")}Edit textures</button>
        <button class="btn ghost">${icon("skins")}Swap skin</button>
        <button class="btn ghost" ${v.skin ? "" : "disabled style='opacity:.4'"}>${icon("undo")}Restore</button>
      </div>`;
  }
  $("#g-inspector").addEventListener("click", (e) => {
    const s = e.target.closest("[data-slot]"); if (s) { S.slot = +s.dataset.slot; applySlot(); }
    const m = e.target.closest("[data-move]"); if (m) { S.move = m.dataset.move; gameFig.set({ move: S.move }); renderInspector(); }
  });

  // ---------------- skins ----------------
  function renderSkins() {
    const names = ["All", "Installed", ...new Set(skins.map((s) => s.fighter))];
    $("#skin-filters").innerHTML = names.map((n) => `<button class="chip ${S.filter === n ? "on" : ""}" data-f="${n}">${n}</button>`).join("");
    const list = skins.filter((s) => S.filter === "All" || (S.filter === "Installed" ? s.installed : s.fighter === S.filter));
    $("#skin-grid").innerHTML = list.map((s, i) => `<div class="skin" data-k="${skins.indexOf(s)}" style="--c:${s.palette.base}">
        <div class="art"><canvas></canvas>
          ${s.installed ? `<span class="tag green"><span class="dot ok"></span>${s.installed}</span>` : ""}
          <div class="swatches">${[s.palette.base, s.palette.accent, s.palette.eye].map((c) => `<i style="background:${c}"></i>`).join("")}</div>
        </div>
        <div class="meta"><div><b>${s.name}</b><small>${s.fighter} · ${s.author}</small></div><span class="mono">${s.changes.length} parts</span></div>
      </div>`).join("");
    $$("#skin-grid .skin").forEach((card) => {
      const s = skins[+card.dataset.k];
      const fig = Figure.mount($("canvas", card), { palette: s.palette, scale: 0.78, floor: 0.93, yaw: 0.5 + Math.random() * 0.4 });
      card.addEventListener("mouseenter", () => fig.set({ spin: 1.4 }));
      card.addEventListener("mouseleave", () => fig.set({ spin: 0 }));
      card.addEventListener("click", () => {
        const fi = fighters.findIndex((f) => f.name === s.fighter);
        go("game"); selectFighter(fi);
        gameFig.set({ palette: s.palette }); $("#v-game .stage-bg").style.setProperty("--tint", s.palette.base);
      });
    });
  }
  $("#skin-filters").addEventListener("click", (e) => { const c = e.target.closest("[data-f]"); if (c) { S.filter = c.dataset.f; renderSkins(); } });

  // ---------------- mods ----------------
  const byId = (id) => mods.find((m) => m.id === id);
  function clashes() {
    const owners = {};
    for (const m of mods) if (m.installed && m.enabled) for (const h of m.hooks) (owners[h] ||= []).push(m);
    return Object.entries(owners).filter(([, ms]) => ms.length > 1);
  }
  const glyph = (m, cls = "") => `<span class="glyph ${cls}" style="--h:${m.hue}">${icon(m.glyph)}</span>`;
  const installedCount = () => mods.filter((m) => m.installed).length;

  function modCard(m) {
    const btn = m.installed
      ? `<span class="tag ${m.enabled ? "green" : ""}">${m.enabled ? icon("check") + "On" : "Off"}</span>`
      : `<button class="btn ghost sm" data-install="${m.id}">${icon("plus")}Install</button>`;
    return `<div class="mod ${S.mod === m.id ? "sel" : ""}" data-mod="${m.id}">
      <div class="mod-top">${glyph(m)}<div><b>${m.name}</b><small>${m.author} · <span class="mono">${m.version}</span></small></div></div>
      <p>${m.desc}</p>
      <div class="mod-foot">${m.tags.map((t) => `<span class="tag">${t}</span>`).join("")}<span class="grow"></span><span class="mono">${fmt(m.saves)}</span>${btn}</div>
    </div>`;
  }

  function renderMods() {
    $$("#mod-tabs button").forEach((b) => b.classList.toggle("on", b.dataset.tab === S.modTab));
    $("#mod-tabs [data-tab=installed] .mono").textContent = installedCount();
    const body = $("#mods-body");
    const cl = clashes();
    $(".rail [data-view=mods] .badge").textContent = cl.length + mods.filter((m) => m.update && m.installed).length;
    if (S.modTab === "discover") {
      const f = mods.find((m) => m.featured);
      body.innerHTML = `
        <div class="featured" data-mod="${f.id}" style="--h:${f.hue}">
          <svg class="big"><use href="#i-${f.glyph}"/></svg>
          <span class="k">New on textures.gg</span>
          <h3>${f.name}</h3>
          <p>${f.desc}</p>
          <div class="row gap">${f.installed ? `<span class="tag green">${icon("check")}Installed</span>` : `<button class="btn primary" data-install="${f.id}">Install</button>`}<span class="muted mono">${f.size} · ${fmt(f.saves)} saves</span></div>
        </div>
        <div class="row gap wrap cats">${["Everything", "Gameplay", "Training", "Visual", "Stages", "Menus", "Tools"].map((c, i) => `<button class="chip ${i ? "" : "on"}">${c}</button>`).join("")}</div>
        <div class="mod-grid">${mods.filter((m) => !m.featured).map(modCard).join("")}</div>`;
    } else if (S.modTab === "installed") {
      const inst = mods.filter((m) => m.installed);
      body.innerHTML = `
        ${cl.map(([hook, ms]) => `<div class="callout">${icon("alert")}<div>
            <b>${ms[0].name} and ${ms[1].name} both replace the same function</b>
            <p>Only one mod can own <code>${hook}</code>. The game won't start with both turned on.</p>
            <div class="row gap"><button class="btn ghost sm" data-off="${ms[1].id}">Turn off ${ms[1].name}</button><button class="btn ghost sm" data-off="${ms[0].id}">Turn off ${ms[0].name}</button></div>
          </div></div>`).join("")}
        <div class="inst-list">${inst.map((m) => `<div class="inst ${m.enabled ? "" : "off"} ${S.mod === m.id ? "sel" : ""}" data-mod="${m.id}">
            ${glyph(m)}<div class="grow"><b>${m.name}</b><small><span class="mono">${m.version}</span> · ${m.hooks.length} function${m.hooks.length > 1 ? "s" : ""}${m.files.length ? ` · ${m.files.length} file` : ""}</small></div>
            ${m.update ? `<button class="btn ghost sm">Update to <span class="mono">${m.update}</span></button>` : ""}
            ${cl.some(([, ms]) => ms.includes(m)) ? `<span class="tag red">${icon("alert")}Conflict</span>` : ""}
            <button class="switch ${m.enabled ? "on" : ""}" data-toggle="${m.id}" aria-label="On"></button>
          </div>`).join("")}</div>
        <p class="hint">Mods install into <span class="mono">~/Games/tgg-melee/mods</span>. Turning one off keeps it there.</p>`;
    } else {
      const up = mods.filter((m) => m.update);
      body.innerHTML = up.length ? `<div class="inst-list">${up.map((m) => `<div class="inst" data-mod="${m.id}">${glyph(m)}<div class="grow"><b>${m.name}</b><small><span class="mono">${m.version} → ${m.update}</span> · Loads every form in the background now</small></div><button class="btn primary sm">Update</button></div>`).join("")}</div>` : `<div class="empty">Everything is up to date.</div>`;
    }
    renderDrawer();
  }

  function fakeHash(id) { let h = 2166136261; for (const c of id) h = Math.imul(h ^ c.charCodeAt(0), 16777619); let s = ""; for (let i = 0; i < 8; i++) { h = Math.imul(h ^ (h >>> 13), 1274126177); s += (h >>> 0).toString(16).padStart(8, "0").slice(0, 8); } return s; }

  function renderDrawer() {
    const m = byId(S.mod), cl = clashes();
    const clashWith = (h) => { const c = cl.find(([hook]) => hook === h); return c ? c[1].filter((x) => x !== m).map((x) => x.name)[0] : null; };
    const hash = fakeHash(m.id);
    $("#mod-drawer").innerHTML = `
      ${glyph(m, "lg")}
      <h3>${m.name}</h3>
      <div class="muted">${m.author} · <span class="mono">${m.version}</span></div>
      <div class="row gap wrap" style="margin-top:10px">${m.tags.map((t) => `<span class="tag">${t}</span>`).join("")}</div>
      <div class="dact">
        ${!m.installed ? `<button class="btn primary" data-install="${m.id}">${icon("plus")}Install</button>`
          : `<button class="btn ${m.enabled ? "ghost" : "primary"}" style="flex:1;justify-content:center;height:36px" data-toggle="${m.id}">${m.enabled ? "Turn off" : "Turn on"}</button><button class="btn ghost" style="height:36px" data-remove="${m.id}">Remove</button>`}
      </div>
      <p class="lead">${m.desc}</p>
      <div class="sec" style="margin-top:22px"><div class="sec-t">Replaces</div>
        <div class="mono-list">${m.hooks.map((h) => { const w = clashWith(h); return `<div class="${w ? "clash" : ""}">${icon(w ? "alert" : "cube", "ico " + (w ? "bad" : "faint"))}${h}${w ? `<span class="note">also ${w}</span>` : ""}</div>`; }).join("")}</div>
      </div>
      <div class="sec"><div class="sec-t">Game files</div>
        ${m.files.length ? `<div class="mono-list">${m.files.map((f) => `<div>${icon("layers", "ico faint")}${f}</div>`).join("")}</div>` : `<div class="muted">Ships no game files.</div>`}
      </div>
      ${m.imports.length ? `<div class="sec"><div class="sec-t">Needs</div><div class="mono-list">${m.imports.map((x) => `<div>${icon("check", "ico ok")}${x}<span class="note">in ${port.name}</span></div>`).join("")}</div></div>` : ""}
      <div class="sec"><div class="sec-t">Fits your game</div>
        <div class="ok-line">${icon("check")}${port.name} ${port.version} · <span class="mono">${port.layout}</span></div>
      </div>
      <div class="sec"><div class="sec-t">Package</div>
        <dl class="facts"><dt>Size</dt><dd class="mono">${m.size}</dd><dt>Saves</dt><dd class="mono">${m.saves.toLocaleString()}</dd><dt>Registry</dt><dd>textures.gg</dd><dt>SHA-256</dt><dd class="mono" title="${hash}">${hash.slice(0, 8)}…${hash.slice(-6)}</dd><dt>Id</dt><dd class="mono">${m.id}</dd></dl>
      </div>`;
  }

  $("#v-mods").addEventListener("click", (e) => {
    const t = e.target.closest("[data-tab]"); if (t) { S.modTab = t.dataset.tab; return renderMods(); }
    const tg = e.target.closest("[data-toggle]"); if (tg) { const m = byId(tg.dataset.toggle); m.enabled = !m.enabled; S.mod = m.id; return renderMods(); }
    const off = e.target.closest("[data-off]"); if (off) { byId(off.dataset.off).enabled = false; S.mod = off.dataset.off; return renderMods(); }
    const ins = e.target.closest("[data-install]"); if (ins) { const m = byId(ins.dataset.install); m.installed = m.enabled = true; S.mod = m.id; return renderMods(); }
    const rm = e.target.closest("[data-remove]"); if (rm) { const m = byId(rm.dataset.remove); m.installed = m.enabled = false; return renderMods(); }
    const md = e.target.closest("[data-mod]"); if (md) { S.mod = md.dataset.mod; renderMods(); }
  });

  // ---------------- editor ----------------
  const edit = { palette: Object.assign({}, skins[0].palette), tex: 0 };
  const editFig = Figure.mount($("#edit-canvas"), { palette: edit.palette, interactive: true, scale: 0.6, floor: 0.8, yaw: 0.4, phase: 0 });
  $("#v-editor .stage-bg").style.setProperty("--tint", edit.palette.base);

  function rng(seed) { return () => ((seed = Math.imul(seed ^ (seed >>> 15), 2246822519) + 0x9e3779b9 | 0) >>> 0) / 4294967296; }
  function texture(kind, pal, n = 32) {
    const c = document.createElement("canvas"); c.width = c.height = n; const x = c.getContext("2d"); const r = rng(kind.length * 977);
    const fill = (col) => { x.fillStyle = col; x.fillRect(0, 0, n, n); };
    if (kind === "Body") { fill(pal.base); for (let i = 0; i < n * 6; i++) { x.fillStyle = r() > 0.5 ? "rgba(255,255,255,0.08)" : "rgba(0,0,0,0.14)"; x.fillRect(r() * n | 0, r() * n | 0, 1, 2 + (r() * 3 | 0)); } x.fillStyle = pal.accent; x.globalAlpha = 0.9; x.beginPath(); x.ellipse(n * 0.5, n * 0.85, n * 0.3, n * 0.25, 0, 0, 7); x.fill(); }
    else if (kind === "Scarf") { fill(pal.accent); for (let i = 0; i < n; i += 6) { x.fillStyle = "rgba(0,0,0,0.12)"; x.fillRect(0, i, n, 2); } }
    else if (kind === "Visor") { const g = x.createLinearGradient(0, 0, n, n); g.addColorStop(0, pal.eye); g.addColorStop(1, "#102030"); x.fillStyle = g; x.fillRect(0, 0, n, n); x.fillStyle = "rgba(255,255,255,0.5)"; x.fillRect(n * 0.15, n * 0.2, n * 0.5, 2); }
    else if (kind === "Gloves") { fill(pal.accent); x.fillStyle = "rgba(0,0,0,0.25)"; x.fillRect(0, n * 0.7, n, 2); x.fillRect(n * 0.5, 0, 1, n * 0.7); }
    else if (kind === "Boots") { fill(pal.accent); x.fillStyle = "rgba(0,0,0,0.35)"; x.fillRect(0, n * 0.75, n, n * 0.25); }
    else if (kind === "Eyes") { fill("#f4efe6"); x.fillStyle = "#1b3a2a"; x.beginPath(); x.arc(n * 0.5, n * 0.5, n * 0.28, 0, 7); x.fill(); x.fillStyle = "#000"; x.beginPath(); x.arc(n * 0.5, n * 0.5, n * 0.12, 0, 7); x.fill(); }
    else { fill(pal.base); x.globalAlpha = 0.3; for (let i = 0; i < 40; i++) { x.fillStyle = r() > 0.5 ? "#fff" : "#000"; x.fillRect(r() * n | 0, r() * n | 0, 3, 3); } }
    return c.toDataURL();
  }
  const TEX = [["Body", "128×128", "CMPR"], ["Scarf", "64×32", "CMPR"], ["Visor", "32×32", "RGB5A3"], ["Gloves", "32×32", "CMPR"], ["Boots", "32×32", "CMPR"], ["Eyes", "32×32", "I8"], ["Tail detail", "64×64", "CMPR"], ["Ears", "32×32", "CMPR"]];
  function renderTex() {
    $("#tex-pane").innerHTML = `<div class="pane-t">Textures <span class="mono">${TEX.length}</span></div>` +
      TEX.map(([n, s, f], i) => `<div class="tex ${i === edit.tex ? "on" : ""}" data-t="${i}"><span class="th" style="background-image:url(${texture(n, edit.palette)})"></span><div><b>${n}</b><small>${s} · ${f}</small></div></div>`).join("") +
      `<p class="hint">Drop a PNG on a texture to replace it, or open it in your image editor and the model updates when you save.</p>`;
  }
  $("#tex-pane").addEventListener("click", (e) => { const t = e.target.closest("[data-t]"); if (t) { edit.tex = +t.dataset.t; renderTex(); renderColors(); } });

  function renderColors() {
    const [n, s, f] = TEX[edit.tex];
    const rows = [["base", "Body", "material"], ["accent", "Scarf, gloves, boots", "material"], ["eye", "Visor glow", "vertex"]];
    $("#color-pane").innerHTML = `<div class="pane-t">${n} <span class="mono">${s} · ${f}</span></div>
      <img class="tex-preview" src="${texture(n, edit.palette, 64)}">
      <div class="row gap" style="margin-bottom:16px"><button class="btn ghost sm" style="flex:1;justify-content:center">${icon("in")}Replace</button><button class="btn ghost sm" style="flex:1;justify-content:center">${icon("out")}Export</button></div>
      <div class="pane-t">Colors</div>
      ${rows.map(([k, label, kind]) => `<div class="color-row"><label style="background:${edit.palette[k]}"><input type="color" value="${edit.palette[k]}" data-c="${k}"></label><div class="grow"><b>${label}</b><small>${edit.palette[k].toUpperCase()} · ${kind}</small></div></div>`).join("")}
      <p class="hint">Click a swatch to change it. The model updates as you drag.</p>`;
  }
  $("#color-pane").addEventListener("input", (e) => {
    const k = e.target.dataset.c; if (!k) return;
    edit.palette[k] = e.target.value;
    editFig.set({ palette: edit.palette });
    $("#v-editor .stage-bg").style.setProperty("--tint", edit.palette.base);
    e.target.parentElement.style.background = e.target.value;
    e.target.closest(".color-row").querySelector("small").textContent = `${e.target.value.toUpperCase()} · ${k === "eye" ? "vertex" : "material"}`;
    $(".tex-preview").src = texture(TEX[edit.tex][0], edit.palette, 64);
    $$("#tex-pane .th").forEach((th, i) => (th.style.backgroundImage = `url(${texture(TEX[i][0], edit.palette)})`));
  });

  const MOVES = { idle: { label: "Idle", frames: 60 }, shine: { label: "Down special", frames: 40, spawn: 0.1, active: [0.05, 0.25] }, kick: { label: "Forward smash", frames: 49, active: [0.3, 0.55] } };
  let editMove = "kick";
  function renderTimeline() {
    const m = MOVES[editMove];
    $("#timeline").innerHTML = `<div class="tl-top">
        <button class="play" id="play">${icon(S.playing ? "pause" : "play")}</button>
        ${Object.entries(MOVES).map(([k, v]) => `<button class="chip ${k === editMove ? "on" : ""}" data-em="${k}">${v.label}</button>`).join("")}
        <span class="grow"></span>
        <span class="mono muted" id="fcount">F 0 / ${m.frames}</span>
        <button class="chip">0.25×</button><button class="chip on">1×</button>
      </div>
      <div class="track" id="track"><div class="ticks"></div>
        ${m.active ? `<div class="active" style="left:${m.active[0] * 100}%;width:${(m.active[1] - m.active[0]) * 100}%"><span>Hitbox</span></div>` : ""}
        ${m.spawn != null ? `<div class="spawn" style="left:${m.spawn * 100}%"><span>Spawns reflector</span></div>` : ""}
        <div class="head" id="head"></div></div>`;
    editFig.set({ move: editMove });
  }
  $("#timeline").addEventListener("click", (e) => {
    if (e.target.closest("#play")) { S.playing = !S.playing; renderTimeline(); }
    const em = e.target.closest("[data-em]"); if (em) { editMove = em.dataset.em; S.frame = 0; renderTimeline(); }
  });
  let scrubbing = false;
  const scrub = (e) => { const r = $("#track").getBoundingClientRect(); S.frame = Math.max(0, Math.min(0.999, (e.clientX - r.left) / r.width)); };
  $("#timeline").addEventListener("pointerdown", (e) => { if (e.target.closest("#track")) { scrubbing = true; S.playing = false; scrub(e); renderTimeline(); } });
  window.addEventListener("pointermove", (e) => scrubbing && scrub(e));
  window.addEventListener("pointerup", () => (scrubbing = false));

  let lastT = performance.now();
  (function tick(now) {
    const dt = (now - lastT) / 1000; lastT = now;
    const m = MOVES[editMove];
    if (S.playing) S.frame = (S.frame + dt * 60 / m.frames) % 1;
    const head = $("#head"); if (head) head.style.left = S.frame * 100 + "%";
    const fc = $("#fcount"); if (fc) fc.textContent = `F ${Math.floor(S.frame * m.frames)} / ${m.frames}`;
    editFig.set({ phase: editMove === "idle" ? 0 : S.frame * 1.3 });
    requestAnimationFrame(tick);
  })(lastT);

  // ---------------- command palette ----------------
  const cmd = { items: [], sel: 0 };
  function openCmd() { $("#cmdk").hidden = false; $("#cmdk-input").value = ""; filterCmd(); $("#cmdk-input").focus(); }
  function closeCmd() { $("#cmdk").hidden = true; }
  function filterCmd() {
    const q = $("#cmdk-input").value.toLowerCase();
    const all = [
      ...[["Go to Game", "game", "game"], ["Go to Skins", "skins", "skins"], ["Browse mods", "mods", "mods"], ["Edit textures", "editor", "edit"]].map(([n, v, ic]) => ({ g: "Actions", n, html: `<span class="glyph" style="--h:40">${icon(ic)}</span>`, run: () => go(v) })),
      ...fighters.map((f, i) => ({ g: "Fighters", n: f.name, sub: f.code, html: `<i style="background:${f.palette.base}"></i>`, run: () => { go("game"); selectFighter(i); } })),
      ...skins.map((s) => ({ g: "Skins", n: s.name, sub: `${s.fighter} · ${s.author}`, html: `<i style="background:linear-gradient(135deg,${s.palette.base},${s.palette.accent})"></i>`, run: () => { go("skins"); } })),
      ...mods.map((m) => ({ g: "Mods", n: m.name, sub: m.installed ? "Installed" : m.author, html: glyph(m), run: () => { go("mods"); S.mod = m.id; renderMods(); } })),
    ];
    cmd.items = all.filter((x) => !q || x.n.toLowerCase().includes(q) || (x.sub || "").toLowerCase().includes(q)).slice(0, 40);
    cmd.sel = 0; drawCmd();
  }
  function drawCmd() {
    let g = "", html = "";
    cmd.items.forEach((x, i) => { if (x.g !== g) { g = x.g; html += `<div class="pl-group">${g}</div>`; } html += `<div class="pl-item ${i === cmd.sel ? "on" : ""}" data-i="${i}">${x.html}<span class="grow">${x.n}</span><small>${x.sub || ""}</small></div>`; });
    $("#cmdk-list").innerHTML = html || `<div class="empty">Nothing matches.</div>`;
    const on = $(".pl-item.on"); if (on) on.scrollIntoView({ block: "nearest" });
  }
  $("#omni").addEventListener("click", openCmd);
  $("#cmdk").addEventListener("click", (e) => { const it = e.target.closest(".pl-item"); if (it) { cmd.items[+it.dataset.i].run(); closeCmd(); } else if (e.target.id === "cmdk") closeCmd(); });
  $("#cmdk-input").addEventListener("input", filterCmd);
  document.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); $("#cmdk").hidden ? openCmd() : closeCmd(); return; }
    if ($("#cmdk").hidden) { if (!e.target.closest("input") && "1234".includes(e.key) && e.key !== "") go(["game", "skins", "mods", "editor"][+e.key - 1]); return; }
    if (e.key === "Escape") closeCmd();
    if (e.key === "ArrowDown") { cmd.sel = Math.min(cmd.items.length - 1, cmd.sel + 1); drawCmd(); e.preventDefault(); }
    if (e.key === "ArrowUp") { cmd.sel = Math.max(0, cmd.sel - 1); drawCmd(); e.preventDefault(); }
    if (e.key === "Enter" && cmd.items[cmd.sel]) { cmd.items[cmd.sel].run(); closeCmd(); }
  });

  // ---------------- boot ----------------
  renderRoster(); selectFighter(0); renderSkins(); renderMods(); renderTex(); renderColors(); renderTimeline();
  const want = location.hash.slice(1); if (want) { const [v, extra] = want.split(":"); go(v); if (v === "mods" && extra) { S.modTab = extra; renderMods(); } if (extra === "cmdk") openCmd(); }
})();
