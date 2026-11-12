// The cover box's art slot and its motion: a port of DeetsMusic's src/vinyl.ts
// (docs/features/VINYL.md; ACTIVITY.md §14.2).
//
//   Press   the record turns while the song plays. The angle follows the song, not the clock:
//           angle = 360° × position ÷ period, where the period is one turn at the host's Spin
//           speed, stretched so the song lasts a whole number of turns (each disc starts and
//           ends upright). A song change slides the old disc out and the new one in.
//   Metal   the zoetrope (--cover-swap: zoetrope): the old cover turns away on a drum while
//           the next one comes in.
//   Happy   the slide waits for Happy's push (--vinyl-swap-delay, set by pet.ts).
// Every other skin shows the plain square cover. CSS decides whether the slot is a record
// here (--vinyl on .np__art); this file only moves it.
//
// The page knows the song's position exactly (the state message's position and its time), so
// the app's whole-second estimate is not needed: the disc snaps only on a real seek and
// otherwise runs up to 5 % fast or slow to stay on the song. Both motions are Web Animations,
// so the compositor runs them.

import { motionReduced } from "./motion";
import { setting, onSettingsChange } from "./skin-settings";

const RPM: Record<string, number> = { "16": 50 / 3, "33": 100 / 3, "45": 45, "78": 78 };
const turnS = (): number => 60 / (RPM[setting("pressVinylSpeed")] ?? RPM["16"]);
const SEEK_S = 1.5;
const SNAP_MIN_DEG = 50;
const RATE_CAP = 0.05;
const CATCH_MS = 1500;
const DECODE_WAIT_MS = 300;

const root = document.documentElement;
const spinWanted = (): boolean => root.dataset.hookRecord === "on" && setting("pressVinyl") === "spin" && !motionReduced();

/** One turn at the chosen speed, stretched so the song lasts a whole number of turns. */
export const periodFor = (duration: number, turn = turnS()): number => (duration > 0 ? duration / Math.max(1, Math.round(duration / turn)) : turn);

const ms = (v: string): number => {
  const n = parseFloat(v);
  if (!isFinite(n)) return 0;
  return v.trim().endsWith("ms") ? n : n * 1000;
};

/** The zoetrope's drum (DeetsMusic's drum.ts): the perspective for a cover of this width. */
export function drumPerspective(base: number, perCover: number, coverW: number): number {
  return Math.max(base, perCover > 0 && coverW > 0 ? perCover * coverW : 0);
}
/** The depth that shows a cover of the drum at `scale`. */
export function drumBack(perspective: number, scale: number): number {
  return scale >= 1 ? 0 : perspective - perspective / scale;
}

export interface Vinyl {
  /** Show a song's cover. `song` is its identity (a new one turns); no `src` = the ♪ placeholder. */
  show(song: string, src: string | undefined): void;
  playing(on: boolean): void;
  /** The play position in seconds, and the song's length (0: no end). */
  position(sec: number, duration: number): void;
  /** The scrubber is dragged to `sec`; `done` when it is let go. */
  scrub(sec: number, done?: boolean): void;
  destroy(): void;
}

// Which way a song change turns: a return to the song before is Previous.
const recent: string[] = [];
let lastTurn: { song: string; back: boolean } | null = null;
function turnBack(next: string): boolean {
  if (lastTurn?.song === next) return lastTurn.back;
  const back = recent.length >= 2 && recent[recent.length - 2] === next;
  if (back) recent.pop();
  else recent.push(next);
  if (recent.length > 20) recent.shift();
  lastTurn = { song: next, back };
  return back;
}

export function mountVinyl(box: HTMLElement, glyph: string): Vinyl {
  let slot: HTMLElement | null = null;
  let song: string | null = null;
  let src: string | undefined;
  let gen = 0;
  let anim: Animation | null = null;
  let period = turnS();
  let dur = 0;
  let pos = 0;
  let posAt = 0;
  let isPlaying = false;
  let started = false;
  let scrubbing = false;

  const build = (s: string | undefined): HTMLElement => {
    const el = document.createElement("div");
    el.className = s ? "vinyl" : "vinyl vinyl--glyph";
    if (s) {
      el.innerHTML = '<div class="vinyl__plate"></div><div class="vinyl__spin"><img alt="" data-art /></div>';
      const img = el.querySelector("img")!;
      img.decoding = "async";
      img.src = s;
    } else {
      el.innerHTML = glyph;
    }
    return el;
  };
  const place = (el: HTMLElement): void => void box.appendChild(el);
  const spinEl = (): HTMLElement | null => slot?.querySelector<HTMLElement>(".vinyl__spin") ?? null;
  const held = (): boolean => root.dataset.ambient === "paused";
  const isRecordHere = (): boolean => getComputedStyle(box).getPropertyValue("--vinyl").trim() === "1";
  const coverSwap = (): string => getComputedStyle(box).getPropertyValue("--cover-swap").trim() || "none";

  const setDuration = (duration: number): void => {
    const d = duration > 0 && isFinite(duration) ? duration : 0;
    if (d === dur) return;
    dur = d;
    period = periodFor(d);
  };

  const errNow = (a: Animation, pMs: number): number => {
    const running = a.playState === "running" && isPlaying;
    const want = ((pos + (running ? (performance.now() - posAt) / 1000 : 0)) * 1000) % pMs;
    let err = want - (Number(a.currentTime ?? 0) % pMs);
    if (err > pMs / 2) err -= pMs;
    else if (err < -pMs / 2) err += pMs;
    return err;
  };

  /** `snap`: the disc may jump to the position (a seek); false = it only changes speed. */
  const sync = (snapAsked = false): void => {
    let snap = snapAsked;
    const el = spinEl();
    if (!el || !spinWanted()) {
      anim?.cancel();
      anim = null;
      return;
    }
    const pMs = period * 1000;
    const now = pos + (isPlaying ? (performance.now() - posAt) / 1000 : 0);
    const target = (now * 1000) % pMs;
    if (!anim || (anim.effect as KeyframeEffect | null)?.target !== el) {
      anim = el.animate([{ transform: "rotate(0turn)" }, { transform: "rotate(1turn)" }], { duration: pMs, iterations: Infinity, easing: "linear" });
      anim.pause();
      anim.currentTime = target;
    } else if (anim.effect?.getTiming().duration !== pMs) {
      anim.effect?.updateTiming({ duration: pMs });
    }
    if (scrubbing) return;
    if (snap && started && (Math.abs(errNow(anim, pMs)) / pMs) * 360 < SNAP_MIN_DEG) snap = false;
    const live = dur <= 0;
    const run = isPlaying && !held();
    if (!run) {
      if (anim.playState === "running") anim.pause();
      if (snap && !live) anim.currentTime = target;
      return;
    }
    if (anim.playState !== "running") {
      if (!started || snap) anim.currentTime = target;
      started = true;
      anim.playbackRate = 1;
      anim.play();
      return;
    }
    if (live) return;
    if (snap) {
      anim.currentTime = target;
      anim.updatePlaybackRate(1);
      return;
    }
    let err = target - (Number(anim.currentTime ?? 0) % pMs);
    if (err > pMs / 2) err -= pMs;
    else if (err < -pMs / 2) err += pMs;
    anim.updatePlaybackRate(1 + Math.max(-RATE_CAP, Math.min(RATE_CAP, err / CATCH_MS)));
  };

  const stackTimers = new WeakMap<HTMLElement, number>();
  const stack = (e: HTMLElement, z: number, wait = 0): void => {
    window.clearTimeout(stackTimers.get(e));
    if (wait <= 0) {
      e.style.zIndex = String(z);
      return;
    }
    stackTimers.set(e, window.setTimeout(() => void (e.style.zIndex = String(z)), wait));
  };

  /** Turn `old` out and `el` in, once `el`'s cover is decoded (or DECODE_WAIT_MS passed). */
  const slide = (old: HTMLElement, el: HTMLElement, back: boolean): void => {
    const mine = gen;
    old.classList.add("vinyl--out");
    el.style.opacity = "0";
    const img = el.querySelector("img");
    const decoded = img ? img.decode().catch(() => {}) : Promise.resolve();
    const wait = new Promise((r) => window.setTimeout(r, DECODE_WAIT_MS));
    const zoe = !isRecordHere() && coverSwap() === "zoetrope";
    void (zoe ? Promise.resolve() : Promise.race([decoded, wait])).then(() => {
      el.style.opacity = "";
      if (mine !== gen || !old.isConnected) return;
      const cs = getComputedStyle(box);
      const x = parseFloat(cs.getPropertyValue("--vinyl-x")) || 0;
      const y = parseFloat(cs.getPropertyValue("--vinyl-y")) || 0;
      const travel = (parseFloat(cs.getPropertyValue("--vinyl-swap-travel")) || 110) / 100;
      const fade = parseFloat(cs.getPropertyValue("--vinyl-swap-fade")) || 0;
      const scale = parseFloat(cs.getPropertyValue("--motion-scale")) || 1;
      const duration = ms(cs.getPropertyValue("--vinyl-swap-dur")) * scale;
      const easing = cs.getPropertyValue("--vinyl-swap-ease").trim() || "ease-in-out";
      const d = back ? -1 : 1;
      if (zoe) {
        const pic = (e: HTMLElement): string => e.querySelector("img")?.src ?? "";
        if (pic(old) && pic(old) === pic(el)) {
          old.remove();
          return;
        }
        const num = (p: string, dflt: number): number => {
          const v = parseFloat(cs.getPropertyValue(p));
          return Number.isFinite(v) ? v : dflt;
        };
        const run = old.offsetWidth * num("--zoetrope-run", 0.5) * d;
        const turn = num("--zoetrope-turn", 58) * d;
        const small = num("--zoetrope-scale", 0.6);
        const away = num("--zoetrope-fade", 0);
        const dim = `brightness(${num("--zoetrope-dim", 1)})`;
        const P = drumPerspective(num("--zoetrope-perspective", 900), num("--zoetrope-perspective-per-cover", 0), old.offsetWidth);
        const drum = (dx: number, deg: number, s: number): string => `perspective(${P}px) translateX(${dx}px) translateZ(${drumBack(P, s)}px) rotateY(${deg}deg)`;
        const stagger = Math.min(0.8, Math.max(0, num("--zoetrope-stagger", 0)));
        const leaveAt = Math.min(1, Math.max(0.1, num("--zoetrope-leave", stagger + 0.35)));
        const front = { transform: drum(0, 0, 1), opacity: 1, filter: "brightness(1)" };
        const behind = (dx: number, deg: number): Keyframe => ({ transform: drum(dx, deg, small), opacity: away, filter: dim });
        const leave = cs.getPropertyValue("--zoetrope-leave-ease").trim() || easing;
        const lead = Math.min(0.3, Math.max(0, num("--zoetrope-lead", 0)));
        const rise = Math.min(0.9, Math.max(0, num("--zoetrope-rise", 0.35)));
        const at = getComputedStyle(old);
        const from = { transform: at.transform === "none" ? front.transform : at.transform, opacity: Number(at.opacity), filter: at.filter === "none" ? front.filter : at.filter };
        old.getAnimations().forEach((a) => a.cancel());
        old.style.opacity = "";
        stack(old, 2);
        old.animate([{ ...from, easing: leave }, { ...behind(-run, -turn), offset: leaveAt }, behind(-run, -turn)], { duration, fill: "forwards" })
          .finished.then(() => old.remove(), () => old.remove());
        el.style.opacity = "0";
        const t0 = performance.now();
        void Promise.race([decoded, wait]).then(() => {
          if (mine !== gen || !el.isConnected) return;
          el.style.opacity = "";
          const late = performance.now() - t0;
          const hold = Math.max(0, duration * stagger - late);
          const move = Math.max(duration * (1 - stagger), duration * 0.4);
          stack(el, 1);
          stack(el, 3, hold + move * lead);
          window.setTimeout(() => {
            if (mine === gen && el.isConnected) box.dispatchEvent(new CustomEvent("vinyl:covered", { bubbles: true }));
          }, hold + move * Math.max(rise, lead));
          el.animate(
            rise > 0 ? [{ ...behind(run, turn), opacity: 0, easing }, { opacity: 1, offset: Math.max(rise, lead) }, front] : [{ ...behind(run, turn), easing }, front],
            { duration: move, delay: hold, fill: "backwards" },
          );
        });
        return;
      }
      const w = old.offsetWidth * travel * x * d;
      const h = old.offsetHeight * travel * y * d;
      const delay = ms(cs.getPropertyValue("--vinyl-swap-delay")) * scale;
      const inTravel = parseFloat(cs.getPropertyValue("--vinyl-swap-in-travel"));
      const inK = Number.isFinite(inTravel) ? inTravel : 1;
      old
        .animate([{ transform: "translate(0px, 0px)", opacity: 1 }, { transform: `translate(${w}px, ${h}px)`, opacity: fade }], { duration, delay, easing, fill: "both" })
        .finished.then(() => old.remove(), () => old.remove());
      el.animate([{ transform: `translate(${-w * inK}px, ${-h * inK}px)`, opacity: inK ? fade : 0 }, { transform: "translate(0px, 0px)", opacity: 1 }], { duration, delay, easing, fill: "backwards" });
    });
  };

  // The skin, the frame's visibility and the motion setting can each start or stop the spin.
  const observer = new MutationObserver(() => sync());
  observer.observe(root, { attributes: true, attributeFilter: ["data-skin", "data-ambient", "data-motion", "data-hook-record"] });
  const unsub = onSettingsChange((k) => {
    if (k === "pressVinyl") sync();
    if (k === "pressVinylSpeed") {
      period = periodFor(dur);
      sync(true);
    }
  });

  box.replaceChildren();
  slot = build(undefined);
  place(slot);

  return {
    show(next, s) {
      if (next === song) {
        if (s === src) return;
        // The same song at another size: swap the file once it is decoded, so it never blanks.
        src = s;
        const img = slot?.querySelector("img");
        if (img && s) {
          const pre = new Image();
          pre.src = s;
          void pre.decode().catch(() => {}).then(() => {
            if (src === s && img.isConnected) img.src = s;
          });
        } else {
          const el = build(s);
          slot?.remove();
          slot = el;
          place(el);
          sync();
        }
        return;
      }
      const back = turnBack(next);
      gen++;
      const old = slot;
      const first = song === null;
      song = next;
      src = s;
      pos = 0;
      posAt = performance.now();
      started = false;
      anim = null;
      const zoe = !isRecordHere() && coverSwap() === "zoetrope" && !motionReduced();
      box.querySelectorAll<HTMLElement>(".vinyl--out").forEach((e) => {
        const runs = e.getAnimations();
        if (!zoe || !runs.length) return e.remove();
        runs.forEach((a) => a.updatePlaybackRate(4));
        window.setTimeout(() => e.remove(), 400);
      });
      const el = build(s);
      slot = el;
      place(el);
      const samePic = !isRecordHere() && coverSwap() === "zoetrope" && !!s && old?.querySelector("img")?.src === s;
      const turned = !first && !samePic && !!(old && old.querySelector("img") && s && !held() && !motionReduced() && (isRecordHere() || coverSwap() !== "none"));
      if (turned) slide(old!, el, back);
      else old?.remove();
      box.dataset.turn = turned ? (back ? "back" : "fwd") : "none";
      box.dataset.turnSong = next;
      box.dataset.turnAt = String(performance.now());
      box.dispatchEvent(new CustomEvent("vinyl:turn", { bubbles: true, detail: { turned, back } }));
      sync();
    },
    playing(on) {
      if (on === isPlaying) return;
      // Keep the position where the clock had it when the play state changes.
      pos += isPlaying ? (performance.now() - posAt) / 1000 : 0;
      posAt = performance.now();
      isPlaying = on;
      sync();
    },
    position(sec, duration) {
      if (!isFinite(sec) || scrubbing) return;
      setDuration(duration);
      const expected = pos + (isPlaying ? (performance.now() - posAt) / 1000 : 0);
      pos = sec;
      posAt = performance.now();
      const step = Math.abs(sec - expected);
      sync(started && step >= SEEK_S);
    },
    scrub(sec, done = false) {
      pos = Math.max(0, sec);
      posAt = performance.now();
      if (done) {
        scrubbing = false;
        sync();
        return;
      }
      scrubbing = true;
      started = true;
      sync();
      if (anim) {
        if (anim.playState === "running") anim.pause();
        anim.currentTime = (pos * 1000) % (period * 1000);
      }
    },
    destroy() {
      anim?.cancel();
      observer.disconnect();
      unsub();
    },
  };
}
