// A stand-in for the hsd-render viewport: a shaded 3D mannequin drawn on a
// 2D canvas, so the mocks can show "a live fighter" without the real renderer.
// Figure.mount(canvas, opts) -> { set(opts) }. One animation loop drives all.
(function () {
  const figures = new Set();

  function hex(h) {
    const n = parseInt(h.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  function mix(a, b, t) {
    const A = hex(a), B = hex(b);
    return "rgb(" + A.map((v, i) => Math.round(v + (B[i] - v) * t)).join(",") + ")";
  }
  function lerpHex(a, b, t) {
    if (a[0] !== "#" || b[0] !== "#") return b;
    const A = hex(a), B = hex(b);
    return "#" + A.map((v, i) => Math.round(v + (B[i] - v) * t).toString(16).padStart(2, "0")).join("");
  }
  const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];

  // Joints in metres, y up, facing +z at yaw 0.
  function skeleton(t, move, phase) {
    const bob = Math.sin(t * 2.4) * 0.016;
    const sway = Math.sin(t * 2.4 + 0.8) * 0.03;
    const j = {
      pelvis: [0, 0.9 + bob, 0],
      chest: [0, 1.3 + bob * 1.3, 0.03],
      neck: [0, 1.46 + bob * 1.4, 0.04],
      head: [0, 1.63 + bob * 1.5, 0.05],
      sL: [0.21, 1.38 + bob, 0.02], sR: [-0.21, 1.38 + bob, 0.02],
      eL: [0.32, 1.14 + bob, 0.16 + sway], eR: [-0.31, 1.12 + bob, 0.04 - sway * 0.5],
      hL: [0.17, 1.26 + bob, 0.33 + sway], hR: [-0.24, 0.98 + bob, 0.2],
      hipL: [0.1, 0.88 + bob, 0], hipR: [-0.1, 0.88 + bob, 0],
      kL: [0.17, 0.5 + bob * 0.5, 0.14], kR: [-0.19, 0.49 + bob * 0.5, -0.07],
      fL: [0.17, 0.07, 0.2], fR: [-0.21, 0.07, -0.15],
    };
    let shine = 0;
    if (move === "kick") {
      const k = Math.sin(Math.min(phase, 1) * Math.PI);
      j.kL = add(j.kL, [0, 0.34 * k, 0.2 * k]);
      j.fL = add(j.fL, [0.02, 0.78 * k, 0.5 * k]);
      j.chest = add(j.chest, [0, -0.02 * k, -0.08 * k]);
      j.head = add(j.head, [0, -0.02 * k, -0.1 * k]);
      j.neck = add(j.neck, [0, -0.02 * k, -0.09 * k]);
      j.hR = add(j.hR, [0, 0.2 * k, -0.25 * k]);
    } else if (move === "shine") {
      const c = Math.min(phase * 4, 1);
      const dn = 0.16 * c;
      for (const n of ["pelvis", "chest", "neck", "head", "sL", "sR", "eL", "eR", "hL", "hR", "hipL", "hipR"]) j[n] = add(j[n], [0, -dn, 0]);
      j.kL = add(j.kL, [0.03 * c, -dn * 0.55, 0.1 * c]);
      j.kR = add(j.kR, [-0.03 * c, -dn * 0.55, 0.04 * c]);
      shine = c * (0.85 + 0.15 * Math.sin(t * 40));
    }
    return { j, shine };
  }

  function draw(f, now) {
    const { canvas, ctx, o } = f;
    const W = canvas.width, H = canvas.height;
    ctx.clearRect(0, 0, W, H);
    const t = now / 1000 + f.seed;
    if (o.spin) f.yaw += o.spin * f.dt;
    f.yaw += f.vel * f.dt; f.vel *= Math.pow(0.04, f.dt);
    const phase = o.phase != null ? o.phase : o.move !== "idle" ? ((t * 0.55) % 1.6) / 1.2 : 0;
    const { j, shine } = skeleton(t, o.move || "idle", phase);
    const p = o.palette;

    const D = 4.2, fy = H * (o.scale || 0.82) * D / 1.85, cy = H * (o.floor ?? 0.9);
    const cx = W * (o.x ?? 0.5);
    const cs = Math.cos(f.yaw), sn = Math.sin(f.yaw);
    const proj = (v) => {
      const x = v[0] * cs + v[2] * sn, z = -v[0] * sn + v[2] * cs;
      const k = fy / (D - z);
      return { x: cx + x * k, y: cy - v[1] * k, z, k };
    };

    // Floor glow and contact shadow.
    const c0 = proj([0, 0, 0]);
    const rx = 0.95 * c0.k;
    let g = ctx.createRadialGradient(c0.x, c0.y, 0, c0.x, c0.y, rx);
    g.addColorStop(0, o.floorColor || "rgba(245,200,75,0.18)");
    g.addColorStop(1, "rgba(245,200,75,0)");
    ctx.save(); ctx.translate(c0.x, c0.y); ctx.scale(1, 0.24); ctx.translate(-c0.x, -c0.y);
    ctx.fillStyle = g; ctx.beginPath(); ctx.arc(c0.x, c0.y, rx, 0, 7); ctx.fill();
    g = ctx.createRadialGradient(c0.x, c0.y, 0, c0.x, c0.y, rx * 0.42);
    g.addColorStop(0, "rgba(0,0,0,0.55)"); g.addColorStop(1, "rgba(0,0,0,0)");
    ctx.fillStyle = g; ctx.beginPath(); ctx.arc(c0.x, c0.y, rx * 0.42, 0, 7); ctx.fill();
    ctx.restore();

    const forward = (v, d) => [v[0] + sn * 0 , v[1], v[2] + d];
    const scarfTip = add(j.neck, [Math.sin(t * 3) * 0.05, -0.12, -0.34]);
    const parts = [
      ["cap", j.hipL, j.hipR, 0.1, "shade"],
      ["cap", j.pelvis, j.chest, 0.13, "base"],
      ["cap", j.chest, j.neck, 0.085, "base"],
      ["cap", j.neck, scarfTip, 0.05, "accent"],
      ["cap", j.sL, j.eL, 0.052, "base"], ["cap", j.eL, j.hL, 0.045, "base"],
      ["cap", j.sR, j.eR, 0.052, "base"], ["cap", j.eR, j.hR, 0.045, "base"],
      ["ball", j.hL, null, 0.06, "accent"], ["ball", j.hR, null, 0.06, "accent"],
      ["cap", j.hipL, j.kL, 0.072, "base"], ["cap", j.kL, j.fL, 0.055, "base"],
      ["cap", j.hipR, j.kR, 0.072, "base"], ["cap", j.kR, j.fR, 0.055, "base"],
      ["cap", j.fL, forward(j.fL, 0.12), 0.062, "accent"],
      ["cap", j.fR, forward(j.fR, 0.12), 0.062, "accent"],
      ["ball", j.head, null, 0.14, "base"],
      ["visor", j.head, null, 0.14, "eye"],
    ];
    const items = parts.map((q) => {
      const a = proj(q[1]), b = q[2] ? proj(q[2]) : a;
      return { q, a, b, z: (a.z + b.z) / 2 + (q[0] === "visor" ? 0.01 : 0) };
    }).sort((m, n) => m.z - n.z);

    const L = [-0.62, -0.78];
    for (const { q, a, b, z } of items) {
      const [kind, , , r, key] = q;
      const base = p[key] || p.base;
      const fog = Math.max(0, Math.min(1, (0.25 - z) * 0.9));
      const lit = mix(base, "#fff8ea", 0.38), mid = mix(base, "#000000", fog * 0.35);
      const dark = mix(base, "#0b0a09", 0.62 + fog * 0.15), rim = mix(base, p.rim || "#f5c84b", 0.55);
      if (kind === "cap") {
        let dx = b.x - a.x, dy = b.y - a.y; const len = Math.hypot(dx, dy) || 1;
        let nx = -dy / len, ny = dx / len;
        if (nx * L[0] + ny * L[1] < 0) { nx = -nx; ny = -ny; }
        const R = r * (a.k + b.k) / 2, mx = (a.x + b.x) / 2, my = (a.y + b.y) / 2;
        const gr = ctx.createLinearGradient(mx + nx * R, my + ny * R, mx - nx * R, my - ny * R);
        gr.addColorStop(0, lit); gr.addColorStop(0.3, mid); gr.addColorStop(0.82, dark); gr.addColorStop(1, rim);
        ctx.strokeStyle = gr; ctx.lineWidth = R * 2; ctx.lineCap = "round";
        ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
      } else if (kind === "ball") {
        const R = r * a.k;
        const gr = ctx.createRadialGradient(a.x + L[0] * R * 0.45, a.y + L[1] * R * 0.45, R * 0.05, a.x, a.y, R);
        gr.addColorStop(0, lit); gr.addColorStop(0.45, mid); gr.addColorStop(0.9, dark); gr.addColorStop(1, rim);
        ctx.fillStyle = gr; ctx.beginPath(); ctx.arc(a.x, a.y, R, 0, 7); ctx.fill();
      } else if (kind === "visor") {
        const face = Math.cos(f.yaw);
        if (face > 0.05) {
          const R = r * a.k, w = R * 0.95 * face, side = Math.sin(f.yaw) * R * 0.55;
          ctx.save(); ctx.globalAlpha = Math.min(1, face * 2);
          ctx.fillStyle = base; ctx.shadowColor = base; ctx.shadowBlur = R * 0.6;
          ctx.beginPath(); ctx.ellipse(a.x + side, a.y - R * 0.08, w * 0.62, R * 0.17, 0, 0, 7); ctx.fill();
          ctx.restore();
        }
      }
    }

    if (shine > 0) {
      const c = proj(j.pelvis), R = 0.42 * c.k * (0.96 + shine * 0.04);
      ctx.save(); ctx.globalCompositeOperation = "lighter";
      ctx.translate(c.x, c.y);
      for (let pass = 0; pass < 2; pass++) {
        ctx.beginPath();
        for (let i = 0; i < 6; i++) { const an = Math.PI / 6 + i * Math.PI / 3; ctx[i ? "lineTo" : "moveTo"](Math.cos(an) * R, Math.sin(an) * R); }
        ctx.closePath();
        ctx.fillStyle = pass ? "rgba(0,0,0,0)" : `rgba(120,200,255,${0.16 * shine})`;
        ctx.strokeStyle = `rgba(170,225,255,${0.85 * shine})`;
        ctx.lineWidth = pass ? 2 : 6; ctx.shadowColor = "#7fd0ff"; ctx.shadowBlur = 24;
        ctx.fill(); ctx.stroke();
      }
      ctx.restore();
    }
  }

  let last = performance.now();
  function loop(now) {
    const dt = Math.min(0.05, (now - last) / 1000); last = now;
    for (const f of figures) {
      if (!f.canvas.isConnected) { figures.delete(f); continue; }
      if (f.canvas.offsetParent === null) continue;
      f.dt = dt;
      if (f.to) {
        f.tween = Math.min(1, f.tween + dt / 0.3);
        const e = 1 - Math.pow(1 - f.tween, 3);
        const p = {};
        for (const k in f.to) p[k] = lerpHex(f.from[k] || f.to[k], f.to[k], e);
        f.o.palette = p;
        if (f.tween >= 1) { f.o.palette = f.to; f.to = null; }
      }
      draw(f, now);
    }
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);

  function fit(f) {
    const r = f.canvas.getBoundingClientRect(), d = window.devicePixelRatio || 1;
    f.canvas.width = Math.max(1, Math.round(r.width * d));
    f.canvas.height = Math.max(1, Math.round(r.height * d));
  }

  window.Figure = {
    mount(canvas, opts) {
      const f = { canvas, ctx: canvas.getContext("2d"), o: Object.assign({ move: "idle" }, opts), yaw: opts.yaw ?? 0.5, vel: 0, dt: 0, seed: Math.random() * 10 };
      new ResizeObserver(() => fit(f)).observe(canvas);
      fit(f);
      if (opts.interactive) {
        let dragging = false, lx = 0;
        canvas.addEventListener("pointerdown", (e) => { dragging = true; lx = e.clientX; canvas.setPointerCapture(e.pointerId); canvas.style.cursor = "grabbing"; });
        canvas.addEventListener("pointermove", (e) => { if (!dragging) return; const d = (e.clientX - lx) * 0.01; f.yaw += d; f.vel = d / Math.max(f.dt, 0.016); lx = e.clientX; });
        canvas.addEventListener("pointerup", () => { dragging = false; canvas.style.cursor = "grab"; });
        canvas.style.cursor = "grab";
      }
      figures.add(f);
      return {
        set(o) {
          // A new palette eases in over 300 ms instead of snapping.
          if (o.palette && f.o.palette && o.palette !== f.o.palette) {
            f.from = f.o.palette; f.tween = 0;
            const to = o.palette;
            o = Object.assign({}, o); delete o.palette; f.to = to;
          }
          Object.assign(f.o, o);
        },
        get yaw() { return f.yaw; },
      };
    },
  };
})();
