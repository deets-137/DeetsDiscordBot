// Copies DeetsMusic's look into deetsmusic-ui/look/ (docs/features/ACTIVITY.md §7).
//
//   node scripts/look-sync.mjs           copy, and write look/MANIFEST.json
//   node scripts/look-sync.mjs --check   fail when the copy differs from DeetsMusic's files
//
// It READS ../../DeetsMusic and never writes there. The copy keeps DeetsMusic's own tree
// (src/styles.css, src/styles/…, src/assets/…), so every url() in the stylesheets still
// resolves. Never hand-edit a file under look/: change it in DeetsMusic, then run this.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SOURCE = resolve(HERE, "../../../DeetsMusic");
const LOOK = resolve(HERE, "../deetsmusic-ui/look");
const MANIFEST = join(LOOK, "MANIFEST.json");
const check = process.argv.includes("--check");

const sha = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const posix = (p) => p.split(sep).join("/");

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

/** Every file the look needs: the stylesheets, the fonts, and each asset a url() names. */
function sourceFiles() {
  const files = new Set([join(SOURCE, "src/styles.css"), ...walk(join(SOURCE, "src/styles"))]);
  for (const css of [...files].filter((f) => f.endsWith(".css"))) {
    for (const m of readFileSync(css, "utf8").matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)) {
      const ref = m[1];
      if (!ref || ref.startsWith("data:") || ref.startsWith("#") || /^[a-z]+:/i.test(ref)) continue;
      const target = resolve(dirname(css), ref.split(/[?#]/)[0]);
      if (existsSync(target) && statSync(target).isFile()) files.add(target);
    }
  }
  return [...files].sort();
}

if (!existsSync(join(SOURCE, "src/styles.css"))) {
  console.error(`[look] DeetsMusic is not at ${SOURCE}. The look cannot be ${check ? "checked" : "copied"}.`);
  process.exit(check ? 0 : 1); // a machine without DeetsMusic still builds from the committed copy
}

const want = Object.fromEntries(sourceFiles().map((f) => [posix(relative(SOURCE, f)), sha(f)]));

if (check) {
  const have = existsSync(MANIFEST) ? JSON.parse(readFileSync(MANIFEST, "utf8")).files : {};
  const stale = [];
  for (const [rel, hash] of Object.entries(want)) {
    const copy = join(LOOK, rel);
    if (have[rel] !== hash || !existsSync(copy) || sha(copy) !== hash) stale.push(rel);
  }
  const gone = Object.keys(have).filter((rel) => !(rel in want));
  if (stale.length || gone.length) {
    console.error(`[look] the copy is out of date: ${stale.length} changed or new, ${gone.length} removed in DeetsMusic.`);
    for (const rel of [...stale, ...gone].slice(0, 20)) console.error(`  ${rel}`);
    console.error("Run: npm run look:sync");
    process.exit(1);
  }
  console.log(`[look] ${Object.keys(want).length} files match DeetsMusic.`);
} else {
  rmSync(LOOK, { recursive: true, force: true });
  for (const rel of Object.keys(want)) {
    const to = join(LOOK, rel);
    mkdirSync(dirname(to), { recursive: true });
    copyFileSync(join(SOURCE, rel), to);
  }
  const pkg = JSON.parse(readFileSync(join(SOURCE, "package.json"), "utf8"));
  writeFileSync(MANIFEST, JSON.stringify({ from: "DeetsMusic", version: pkg.version, files: want }, null, 2) + "\n");
  console.log(`[look] copied ${Object.keys(want).length} files from DeetsMusic ${pkg.version}.`);
}
