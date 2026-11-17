(function () {
  const { fighters, skins, mods, port } = DATA;
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];
  const icon = (n) => `<svg class="ico"><use href="#i-${n}"/></svg>`;
  const flat = (p) => Object.assign({}, p, { rim: "#77726a" });
  const S = { place: "melee", open: null, slot: 0, move: "idle", playing: true, frame: 0, modTab: "browse", mod: "tgg.stadium-preload" };

  // A list's marker glides to its selected row instead of the highlight
  // jumping. First placement doesn't animate.
  function mark(lst) {
    const m = lst.querySelector(":scope > .marker");
    const s = lst.querySelector(".sel") || lst.querySelector(":scope > .place.on");
    if (!s) { m.style.opacity = 0; return; }
    m.style.opacity = 1; m.style.height = s.offsetHeight + "px"; m.style.transform = `translateY(${s.offsetTop}px)`;
    if (!lst.dataset.ready) { m.style.transition = "none"; requestAnimationFrame(() => { m.style.transition = ""; lst.dataset.ready = 1; }); }
  }

  // ---------------- shell ----------------
  function go(place) {
    if (S.open != null) closeFighter(true);
    S.place = place;
    $$(".place[data-place]").forEach((b) => b.classList.toggle("on", b.dataset.place === place));
    mark($("#places"));
    for (const v of ["melee", "library", "mods"]) {
      const el = $("#v-" + v), show = v === place;
      if (show && el.hidden) { el.hidden = false; el.classList.remove("enter"); void el.offsetWidth; el.classList.add("enter"); }
      if (!show) el.hidden = true;
    }
    if (place === "mods") renderMods();
    titlerow();
  }
  function titlerow() {
    // A top-level page says its own name, so the row stays empty.
    if (S.open != null) {
      const f = fighters[S.open];
      $("#crumbs").innerHTML = `<button class="up" data-up>Melee</button><span class="slash">/</span><span class="leaf">${f.name}</span>`;
      $("#acts").innerHTML = `<button class="b ink">Edit textures <span class="k">E</span></button>`;
    } else { $("#crumbs").innerHTML = ""; $("#acts").innerHTML = ""; }
  }
  $("#places").addEventListener("click", (e) => { const b = e.target.closest("[data-place]"); if (b) go(b.dataset.place); });
  $("#crumbs").addEventListener("click", (e) => { if (e.target.closest("[data-up]")) closeFighter(); });
  $("#toggle").addEventListener("click", () => $("#shell").classList.toggle("nosb"));
  $("#theme").addEventListener("click", () => {
    const d = document.documentElement, paper = d.dataset.theme !== "paper";
    d.dataset.theme = paper ? "paper" : "gallery"; $("#theme").textContent = paper ? "Gallery" : "Paper";
  });

  const skinFor = (f) => skins.find((s) => s.fighter === f.name && s.installed);
  const recent = [skins[0], skins[2], skins[6]];
  $("#recent").innerHTML = recent.map((s, i) => `<div class="rec" data-rec="${i}"><span class="thumb"><canvas></canvas></span><div class="grow"><b>${s.name}</b><small>${s.fighter} · ${s.installed}</small></div>${i === 0 ? '<span class="dirty" title="Unsaved changes"></span>' : ""}</div>`).join("");
  $$("#recent canvas").forEach((c, i) => Figure.mount(c, { palette: flat(recent[i].palette), scale: 0.9, floor: 0.96, yaw: 0.5, floorColor: "rgba(0,0,0,0)" }));
  $("#recent").addEventListener("click", (e) => {
    const r = e.target.closest("[data-rec]"); if (!r) return;
    const s = recent[+r.dataset.rec]; go("melee");
    const i = fighters.findIndex((f) => f.name === s.fighter);
    openFighter(i, $(`#roster [data-f="${i}"] .plate`));
  });

  // ---------------- melee roster ----------------
  $("#roster").innerHTML = fighters.map((f, i) => {
    const s = skinFor(f);
    return `<div class="card" data-f="${i}"><div class="plate"><canvas></canvas></div>
      <div class="nm">${f.name}</div><div class="stl ${s ? "mine" : ""}">${s ? "1 custom" : "Vanilla"}</div></div>`;
  }).join("");
  $$("#roster .card").forEach((card) => {
    const f = fighters[+card.dataset.f], s = skinFor(f);
    const fig = Figure.mount($("canvas", card), { palette: flat(s ? s.palette : f.palette), scale: 0.8, floor: 0.9, yaw: 0.55, floorColor: "rgba(0,0,0,0)" });
    card.addEventListener("mouseenter", () => fig.set({ spin: 1.1 }));
    card.addEventListener("mouseleave", () => fig.set({ spin: 0 }));
    card.addEventListener("click", () => openFighter(+card.dataset.f, $(".plate", card)));
  });

  // ---------------- subject: one fighter on the stage ----------------
  const stageFig = Figure.mount($("#stage"), { palette: flat(fighters[0].palette), interactive: true, spin: 0.12, scale: 0.6, floor: 0.84, yaw: 0.5, floorColor: "rgba(0,0,0,0)", phase: 0 });
  const NAMES = ["Default", "Red", "Blue", "Green", "White", "Black"], COLS = [null, "#b8433a", "#3f5fb0", "#4c9a50", "#e9e3d6", "#2a2724"], FILE = ["Nr", "Re", "Bu", "Gr", "Wh", "Bk"];
  const slots = (f) => { const s = skinFor(f); return NAMES.map((n, i) => { const mine = s && s.installed === "Slot " + (i + 1); return { n: mine ? s.name : n, col: mine ? s.palette.base : COLS[i] || f.palette.base, skin: mine ? s : null, file: f.code + FILE[i] + ".dat" }; }); };
  const MOVES = [["idle", "Wait", 60, null], ["shine", "Down special", 40, [0.05, 0.25]], ["kick", "Forward smash", 49, [0.3, 0.55]], ["", "Neutral special", 52, null], ["", "Up special", 76, null], ["", "Dash attack", 38, null]];
  const SHARED = { Fox: [["Blaster", "PlFx.dat", 0], ["Reflector", "EfFxData.dat", 0], ["Laser", "EfFxData.dat", 0]], Falco: [["Blaster", "PlFc.dat", 0], ["Reflector", "EfFxData.dat", 0], ["Laser", "EfFxData.dat", 1]] };

  function clipTo(el, rect) {
    const b = $("#body").getBoundingClientRect();
    return `inset(${rect.top - b.top}px ${b.right - rect.right}px ${b.bottom - rect.bottom}px ${rect.left - b.left}px round 4px)`;
  }
  // The card's plate opens into the stage, and the stage closes back into it.
  function openFighter(i, plate) {
    const f = fighters[i], s = skinFor(f);
    S.open = i; S.slot = s ? +s.installed.slice(5) - 1 : 0; S.move = "idle"; S.frame = 0;
    const v = $("#v-subject");
    renderSubject();
    v.hidden = false;
    if (plate) {
      v.classList.remove("opening"); v.style.clipPath = clipTo(v, plate.getBoundingClientRect());
      void v.offsetWidth; v.classList.add("opening"); v.style.clipPath = "inset(0px 0px 0px 0px round 0px)";
    }
    titlerow();
  }
  function closeFighter(instant) {
    const v = $("#v-subject"), plate = $(`#roster [data-f="${S.open}"] .plate`);
    S.open = null; titlerow();
    if (instant || !plate || $("#v-melee").hidden) { v.hidden = true; v.style.clipPath = ""; return; }
    v.classList.add("opening"); v.style.clipPath = clipTo(v, plate.getBoundingClientRect());
    v.addEventListener("transitionend", function done() { v.removeEventListener("transitionend", done); if (S.open == null) { v.hidden = true; v.style.clipPath = ""; } });
  }

  function renderSubject() {
    const f = fighters[S.open], sh = SHARED[f.name] || [[f.code + ".dat", "", 0]];
    $("#subject-panes").innerHTML = `
      <div class="pane" id="p-costumes"><div class="pane-h">Costumes <span class="m">6</span><span class="chev">${icon("chev")}</span></div>
        <div class="pane-b"><div class="in"><div class="lst" id="slot-list"><div class="marker"></div><div class="items"></div></div></div><div class="slot-acts" id="slot-acts"></div></div></div>
      <div class="pane" id="p-shared"><div class="pane-h">Shared <span class="m">${sh.length}</span><span class="chev">${icon("chev")}</span></div>
        <div class="pane-b"><div class="in">${sh.map(([n, file, ch]) => `<div class="r"><b>${n}</b><span class="m fa">${file}</span>${ch ? '<span class="st mine">Changed</span>' : '<span class="st off">Original</span>'}</div>`).join("")}</div></div></div>
      <div class="pane folded" id="p-moves"><div class="pane-h">Moves <span class="m">312</span><span class="chev">${icon("chev")}</span></div>
        <div class="pane-b"><div class="in"><div class="lst" id="move-list"><div class="marker"></div><div class="items"></div></div></div></div></div>`;
    renderSlots(); renderMoves(); renderTimeline();
  }
  function renderSlots(prev) {
    const f = fighters[S.open], ss = slots(f), cur = ss[S.slot];
    $("#slot-list .items").innerHTML = ss.map((x, i) => `<div class="r ${i === S.slot ? "sel" : ""}" data-s="${i}"><span class="sw" style="background:${x.col}"></span><b style="font-weight:${x.skin ? 700 : 500}">${x.n}</b>${x.skin ? '<span class="st mine" style="margin-left:auto">Yours</span>' : `<span class="m fa">${x.file}</span>`}</div>`).join("");
    mark($("#slot-list"));
    const acts = $("#slot-acts");
    acts.innerHTML = `<div class="what">${cur.skin ? `<b>${cur.skin.name}</b> by ${cur.skin.author}. Changes ${cur.skin.changes.join(", ").toLowerCase()}.` : `<b>${f.name}, ${cur.n.toLowerCase()}</b>, as the game ships it.`}</div>
      <button class="b">Install a skin here <span class="k">I</span></button>${cur.skin ? `<button class="b">Restore the original <span class="k">R</span></button>` : ""}`;
    if (prev !== undefined && prev !== S.slot) { acts.classList.remove("swapin"); void acts.offsetWidth; acts.classList.add("swapin"); }
    stageFig.set({ palette: flat(cur.skin ? cur.skin.palette : Object.assign({}, f.palette, { base: cur.col })) });
    $("#fig-l").textContent = `Fig. 1 · ${f.name}, slot ${S.slot + 1}`;
  }
  function renderMoves() {
    $("#move-list .items").innerHTML = MOVES.map(([k, n, fr], i) => `<div class="r ${k === S.move ? "sel" : ""}" data-mv="${i}"><span>${n}</span><span class="m fa">${fr} f</span></div>`).join("");
    mark($("#move-list"));
    const mv = MOVES.find((m) => m[0] === S.move);
    $("#fig-r").textContent = `${mv[1]} · ${mv[2]} frames`;
    stageFig.set({ move: S.move });
  }
  function renderTimeline() {
    const mv = MOVES.find((m) => m[0] === S.move);
    $("#timeline").innerHTML = `<button class="play" id="play">${icon(S.playing ? "pause" : "play")}</button><span style="font-weight:600;white-space:nowrap">${mv[1]}</span>
      <div class="track" id="track"><div class="rail"></div>${mv[3] ? `<div class="act" style="left:${mv[3][0] * 100}%;width:${(mv[3][1] - mv[3][0]) * 100}%"></div>` : ""}<div class="head" id="head"></div></div>
      <span class="m fa" id="fcount"></span>`;
  }
  $("#subject-panes").addEventListener("click", (e) => {
    const h = e.target.closest(".pane-h");
    if (h) { const p = h.parentElement; p.classList.toggle("folded"); setTimeout(() => { mark($("#slot-list")); mark($("#move-list")); }, 320); return; }
    const s = e.target.closest("[data-s]"); if (s) { const prev = S.slot; S.slot = +s.dataset.s; renderSlots(prev); }
    const m = e.target.closest("[data-mv]"); if (m) { const k = MOVES[+m.dataset.mv][0]; if (k) { S.move = k; S.frame = 0; renderMoves(); renderTimeline(); } }
  });
  $("#timeline").addEventListener("click", (e) => { if (e.target.closest("#play")) { S.playing = !S.playing; renderTimeline(); } });
  let scrub = false;
  const scrubTo = (e) => { const r = $("#track").getBoundingClientRect(); S.frame = Math.max(0, Math.min(0.999, (e.clientX - r.left) / r.width)); };
  $("#timeline").addEventListener("pointerdown", (e) => { if (e.target.closest("#track")) { scrub = true; S.playing = false; renderTimeline(); scrubTo(e); } });
  addEventListener("pointermove", (e) => scrub && scrubTo(e));
  addEventListener("pointerup", () => (scrub = false));
  let last = performance.now();
  (function tick(now) {
    const dt = (now - last) / 1000; last = now;
    if (S.open != null) {
      const mv = MOVES.find((m) => m[0] === S.move);
      if (S.playing) S.frame = (S.frame + dt * 60 / mv[2]) % 1;
      const h = $("#head"); if (h) h.style.left = S.frame * 100 + "%";
      const c = $("#fcount"); if (c) c.textContent = `F ${String(Math.floor(S.frame * mv[2])).padStart(2, "0")} / ${mv[2]}`;
      stageFig.set({ phase: S.move === "idle" ? 0 : S.frame * 1.3 });
    }
    requestAnimationFrame(tick);
  })(last);

  // ---------------- library ----------------
  $("#library").innerHTML = skins.map((s, i) => `<div class="card" data-k="${i}"><div class="plate"><canvas></canvas></div><div class="nm">${s.name}</div><div class="stl ${s.installed ? "mine" : ""}">${s.fighter}${s.installed ? " · " + s.installed : ""}</div></div>`).join("");
  $$("#library .card").forEach((card) => {
    const fig = Figure.mount($("canvas", card), { palette: flat(skins[+card.dataset.k].palette), scale: 0.8, floor: 0.9, yaw: 0.55, floorColor: "rgba(0,0,0,0)" });
    card.addEventListener("mouseenter", () => fig.set({ spin: 1.1 }));
    card.addEventListener("mouseleave", () => fig.set({ spin: 0 }));
  });

  // ---------------- mods ----------------
  const byId = (id) => mods.find((m) => m.id === id);
  const clashes = () => { const o = {}; for (const m of mods) if (m.installed && m.enabled) for (const h of m.hooks) (o[h] ||= []).push(m); return Object.entries(o).filter(([, ms]) => ms.length > 1); };
  const status = (m, cl) => !m.installed ? `<span class="fa">—</span>` : cl.some(([, ms]) => ms.includes(m)) ? `<span class="st bad">Conflict</span>` : m.update && m.enabled ? `<span class="st up">${m.update}</span>` : m.enabled ? `<span class="st on">On</span>` : `<span class="st off">Off</span>`;
  const hash = (id) => { let h = 2166136261; for (const c of id) h = Math.imul(h ^ c.charCodeAt(0), 16777619); let s = ""; for (let i = 0; i < 8; i++) { h = Math.imul(h ^ (h >>> 13), 1274126177); s += (h >>> 0).toString(16).padStart(8, "0"); } return s; };
  let shownEntry = null;

  function renderMods() {
    const cl = clashes(), inst = mods.filter((m) => m.installed), ups = mods.filter((m) => m.update);
    $("#mods-warn").style.opacity = cl.length ? 1 : 0;
    $("#mod-tabs").innerHTML = [["browse", "Browse", mods.length], ["installed", "Installed", inst.length], ["updates", "Updates", ups.length]].map(([k, n, c]) => `<button data-tab="${k}" class="${S.modTab === k ? "on" : ""}">${n} <span class="m">${c}</span></button>`).join("");
    // The notice folds away when the conflict is resolved, rather than vanishing.
    if (cl.length) {
      const [hook, ms] = cl[0];
      $("#notice").innerHTML = `<div class="notice"><span class="st bad">Conflict</span><p><b>${ms[0].name}</b> and <b>${ms[1].name}</b> both replace <code>${hook}</code>. The game won't start until one is off.</p>
        <div class="acts"><button class="b" data-off="${ms[1].id}">Turn off ${ms[1].name.split(" ")[0]}…</button><button class="b" data-off="${ms[0].id}">Turn off ${ms[0].name.split(" ")[0]}…</button></div></div>`;
    }
    $("#notice-wrap").classList.toggle("shut", !cl.length);
    const list = S.modTab === "installed" ? inst : S.modTab === "updates" ? ups : mods;
    $("#mod-list .items").innerHTML = list.map((m) => `<div class="mrow ${m.id === S.mod ? "sel" : ""} ${m.installed && !m.enabled ? "dim" : ""}" data-id="${m.id}">
      <span>${m.installed ? `<span class="check ${m.enabled ? "on" : ""}" data-toggle="${m.id}"></span>` : ""}</span>
      <span style="min-width:0"><span class="nm">${m.name}<span>${m.author}</span></span><span class="ds">${m.desc}</span></span>
      <span class="cat">${m.tags.join(", ")}</span><span class="m">${m.version}</span><span class="m">${m.size}</span><span class="m">${m.saves.toLocaleString()}</span><span>${status(m, cl)}</span></div>`).join("");
    mark($("#mod-list"));
    entry(cl);
  }
  function entry(cl) {
    const m = byId(S.mod), h = hash(m.id), el = $("#entry");
    const other = (fn) => { const c = cl.find(([k]) => k === fn); return c && c[1].find((x) => x !== m); };
    el.innerHTML = `<div class="entry">
      <div class="no"><span>No. ${String(mods.indexOf(m) + 1).padStart(3, "0")}</span><span>${m.id}</span></div>
      <h2 class="t-h2">${m.name}</h2>
      <div class="mu">${m.author} · <span class="m">${m.version}</span></div>
      <p class="lead">${m.desc}</p>
      <div class="acts">${!m.installed ? `<button class="b ink lg" data-install="${m.id}">Install <span class="k">Enter</span></button>` : `<button class="b ${m.enabled ? "" : "ink"} lg" data-toggle="${m.id}">${m.enabled ? "Turn off" : "Turn on"} <span class="k">Space</span></button>${m.update ? `<button class="b ink lg">Update</button>` : ""}<button class="b quiet danger" data-remove="${m.id}">Remove</button>`}</div>
      <div class="sect"><div class="sect-h"><span class="no">01</span><h3>Replaces</h3><span class="aside">${m.hooks.length}</span></div>${m.hooks.map((fn) => { const o = other(fn); return `<div class="fn ${o ? "clash" : ""}"><span class="m">${fn}</span>${o ? `<span class="why">also ${o.name.split(" ")[0]}</span>` : `<span class="ok">only this mod</span>`}</div>`; }).join("")}</div>
      <div class="sect"><div class="sect-h"><span class="no">02</span><h3>Game files</h3><span class="aside">${m.files.length || "none"}</span></div>${m.files.length ? m.files.map((f) => `<div class="fn"><span class="m">${f}</span><span class="ok">replaces the disc's</span></div>`).join("") : `<p class="t-small" style="margin:0">Ships no game files.</p>`}</div>
      <div class="sect"><div class="sect-h"><span class="no">03</span><h3>Needs</h3></div>${m.imports.length ? m.imports.map((x) => `<div class="fn"><span class="m">${x}</span><span class="ok">in ${port.name}</span></div>`).join("") : `<p class="t-small" style="margin:0">Nothing from other mods.</p>`}</div>
      <div class="sect"><div class="sect-h"><span class="no">04</span><h3>Package</h3></div><dl class="dl"><dt>Built for</dt><dd class="m">${port.layout}</dd><dt>Size</dt><dd class="m">${m.size}</dd><dt>SHA-256</dt><dd class="m">${h.slice(0, 10)}…${h.slice(-6)}</dd></dl></div></div>`;
    if (shownEntry !== m.id) { el.firstChild.classList.add("swapin"); shownEntry = m.id; }
  }
  $("#v-mods").addEventListener("click", (e) => {
    const t = e.target.closest("[data-toggle]"); if (t) { const m = byId(t.dataset.toggle); m.enabled = !m.enabled; S.mod = m.id; return renderMods(); }
    const o = e.target.closest("[data-off]"); if (o) { byId(o.dataset.off).enabled = false; S.mod = o.dataset.off; return renderMods(); }
    const i = e.target.closest("[data-install]"); if (i) { const m = byId(i.dataset.install); m.installed = m.enabled = true; return renderMods(); }
    const r = e.target.closest("[data-remove]"); if (r) { const m = byId(r.dataset.remove); m.installed = m.enabled = false; return renderMods(); }
    const tb = e.target.closest("[data-tab]"); if (tb) { S.modTab = tb.dataset.tab; return renderMods(); }
    const row = e.target.closest(".mrow[data-id]"); if (row) { S.mod = row.dataset.id; renderMods(); }
  });

  // ---------------- keys ----------------
  addEventListener("keydown", (e) => {
    if (e.target.closest("input")) return;
    if (e.ctrlKey && e.key === "\\") { $("#shell").classList.toggle("nosb"); return; }
    if (e.key === "Escape" && S.open != null) { closeFighter(); return; }
    if (S.open != null && (e.key === "ArrowDown" || e.key === "ArrowUp")) { const prev = S.slot; S.slot = Math.max(0, Math.min(5, S.slot + (e.key === "ArrowDown" ? 1 : -1))); renderSlots(prev); e.preventDefault(); return; }
    if (S.place === "mods") {
      const rows = $$("#mod-list .mrow"), i = rows.findIndex((r) => r.dataset.id === S.mod);
      if (e.key === "ArrowDown" || e.key === "j") { S.mod = rows[Math.min(rows.length - 1, i + 1)].dataset.id; renderMods(); e.preventDefault(); }
      if (e.key === "ArrowUp" || e.key === "k") { S.mod = rows[Math.max(0, i - 1)].dataset.id; renderMods(); e.preventDefault(); }
      if (e.key === " ") { const m = byId(S.mod); if (m.installed) { m.enabled = !m.enabled; renderMods(); } e.preventDefault(); }
    }
    if (S.open == null && "123".includes(e.key)) go(["melee", "library", "mods"][+e.key - 1]);
  });

  // ---------------- boot ----------------
  const want = location.hash.slice(1).split(",");
  if (want.includes("still")) document.documentElement.classList.add("still");
  if (want.includes("paper")) { document.documentElement.dataset.theme = "paper"; $("#theme").textContent = "Gallery"; }
  if (want.includes("nosb")) $("#shell").classList.add("nosb");
  if (want.includes("installed")) S.modTab = "installed";
  go(want.includes("mods") ? "mods" : want.includes("library") ? "library" : "melee");
  const fi = fighters.findIndex((f) => want.includes(f.name.toLowerCase()));
  if (fi >= 0) openFighter(fi, null);
})();
