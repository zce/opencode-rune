// Workspace architecture guardrails for anomalyco/opencode-rune.
// Runs on Bun or Node >= 18 (no Python dependency). Usage: scripts/arch-check.sh
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  countLines,
  extractBraceStructs,
  gitLsFiles,
  parseStructFields,
  stripRust,
} from "./rust-scan.mjs";

const root = join(fileURLToPath(new URL("../..", import.meta.url)));

// Ratchet ceiling for effective fields on `pub struct App` in apps/desktop/src/app.rs,
// expanding plain field-cluster wrappers in apps/desktop/src/shell/app_state.rs (and any
// other apps/desktop/src struct whose fields hold `Tracked<...>`).
// Ceilings only go down; new feature state belongs in opencode_kernel::StateSlots or Luau.
const APP_FIELD_CEILING = 94;
const DESKTOP_ROOT_MAX_FILES = 10;
const HARD_LINE_CAP = 800;

const CORE_FORBID_UNSAFE = [
  "crates/opencode-kernel/src/lib.rs",
  "crates/opencode-platform/src/lib.rs",
  "crates/opencode-runtime/src/lib.rs",
  "crates/opencode-search/src/lib.rs",
  "crates/opencode-ui/src/lib.rs",
];

const FEATURE_UNSAFE_ALLOWLIST = new Set(["egg", "office", "open-in", "updater"]);

const CORE_ISOLATION = {
  "opencode-kernel": new Set([
    "session-view",
    "terminal",
    "browser-pane",
    "office-preview",
    "opencode-storage",
  ]),
  "opencode-runtime": new Set([
    "session-view",
    "terminal",
    "browser-pane",
    "office-preview",
    "opencode-storage",
  ]),
  "opencode-ui": new Set([
    "session-view",
    "opencode-storage",
    "terminal",
    "browser-pane",
    "office-preview",
    "opencode-runtime",
  ]),
  "opencode-code-view": new Set([
    "session-view",
    "opencode-storage",
    "terminal",
    "browser-pane",
    "office-preview",
    "opencode-runtime",
  ]),
  "opencode-search": new Set([
    "session-view",
    "opencode-ui",
    "opencode-code-view",
    "opencode-storage",
    "terminal",
    "browser-pane",
    "office-preview",
    "opencode-runtime",
  ]),
  "opencode-plugin-runtime": new Set([
    "session-view",
    "opencode-storage",
    "terminal",
    "browser-pane",
    "office-preview",
  ]),
};

const EXCLUDED_MANIFESTS = new Set(["apps/browser-engine/Cargo.toml"]);
const EXCEPTIONS_FILE = "scripts/arch-check-exceptions.txt";

const errors = [];
const warnings = [];
const notes = [];

function readText(relPath) {
  return readFileSync(join(root, relPath), "utf8");
}

function tierOf(relDir) {
  if (relDir.startsWith("crates/")) return { tier: "crate", rank: 1 };
  if (relDir.startsWith("features/")) return { tier: "feature", rank: 2 };
  if (relDir.startsWith("apps/")) return { tier: "app", rank: 3 };
  return { tier: "other", rank: 0 };
}

function loadExceptions() {
  const map = new Map(); // key: `${rule}|${src}|${dst}` -> { line, raw, used }
  const full = join(root, EXCEPTIONS_FILE);
  if (!existsSync(full)) return map;
  const lines = readFileSync(full, "utf8").split(/\r?\n/);
  lines.forEach((raw, idx) => {
    const clean = raw.split("#")[0].trim();
    if (!clean) return;
    const m = clean.match(/^([a-z0-9_-]+)\s+([a-z0-9_-]+)\s*->\s*([a-z0-9_-]+)$/);
    if (!m) {
      errors.push(`${EXCEPTIONS_FILE}:${idx + 1}: invalid exception syntax (${clean})`);
      return;
    }
    const key = `${m[1]}|${m[2]}|${m[3]}`;
    map.set(key, { line: idx + 1, raw: clean, used: false });
  });
  return map;
}

function checkException(exceptions, rule, src, dst, detail) {
  const key = `${rule}|${src}|${dst}`;
  const entry = exceptions.get(key);
  if (entry) {
    entry.used = true;
    notes.push(`temporary exception (${EXCEPTIONS_FILE}:${entry.line}): ${detail}`);
    return true;
  }
  return false;
}

// ── Rule 11c (pre-flight): single Rune rev pin in root Cargo.toml ──
// Check this before `cargo metadata` so a mismatched rev fails fast without a git fetch.
{
  const rootToml = readText("Cargo.toml");
  const runeRevs = new Map(); // rev -> line numbers
  rootToml.split(/\r?\n/).forEach((line, idx) => {
    const clean = line.split("#")[0];
    const m = clean.match(/github\.com\/(?:anomalyco|zce)\/rune"\s*,[^}]*\brev\s*=\s*"([^"]+)"/);
    if (m) {
      const list = runeRevs.get(m[1]) ?? [];
      list.push(idx + 1);
      runeRevs.set(m[1], list);
    }
  });
  if (runeRevs.size > 1) {
    const summary = [...runeRevs.entries()].map(([rev, ls]) => `${rev} (lines ${ls.join(",")})`).join("; ");
    console.error(`  ERROR  Cargo.toml: mismatched Rune revs across workspace deps/patches: ${summary}`);
    console.error("arch check: 1 error(s)");
    process.exit(1);
  }
}

// ── Resolved dependency graph via `cargo metadata` ────────────────────────────

const exceptions = loadExceptions();

let meta;
try {
  const raw = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--all-features", "--manifest-path", join(root, "Cargo.toml")],
    { cwd: root, maxBuffer: 64 * 1024 * 1024 },
  ).toString("utf8");
  meta = JSON.parse(raw);
} catch (err) {
  console.error(`  ERROR  failed to run \`cargo metadata --format-version 1 --all-features\`: ${err.message}`);
  process.exit(1);
}

const memberSet = new Set(meta.workspace_members);
const pkgById = new Map(meta.packages.map((p) => [p.id, p]));
const nodeById = new Map(meta.resolve.nodes.map((n) => [n.id, n]));

const wsMeta = new Map(); // pkgId -> { name, relDir, manifestRel, tier, rank }
const wsByName = new Map();
for (const id of memberSet) {
  const pkg = pkgById.get(id);
  const manifestRel = relative(root, pkg.manifest_path).replace(/\\/g, "/");
  const relDir = dirname(manifestRel).replace(/\\/g, "/");
  const { tier, rank } = tierOf(relDir);
  const info = { id, name: pkg.name, relDir, manifestRel, tier, rank };
  wsMeta.set(id, info);
  wsByName.set(pkg.name, info);
}

/**
 * BFS over resolved dependencies, following only edges whose `dep_kinds` match `allowedKinds`
 * (`"normal"` for `kind === null`, `"build"` for `kind === "build"`).
 * Returns Map<pkgId, { path: string[], viaBuild: boolean }>.
 */
function resolveReachable(startId, allowedKinds) {
  const visited = new Map();
  const startPkg = pkgById.get(startId);
  const queue = [{ id: startId, path: [startPkg.name], viaBuild: false }];
  while (queue.length > 0) {
    const { id, path, viaBuild } = queue.shift();
    const node = nodeById.get(id);
    if (!node) continue;
    for (const dep of node.deps) {
      const kinds = dep.dep_kinds.map((k) => k.kind ?? "normal");
      const matched = kinds.filter((k) => allowedKinds.includes(k));
      if (matched.length === 0) continue;
      if (visited.has(dep.pkg)) continue;
      const depPkg = pkgById.get(dep.pkg);
      const edgeIsBuildOnly = !matched.includes("normal");
      const nextViaBuild = viaBuild || edgeIsBuildOnly;
      const label = edgeIsBuildOnly ? `[build] ${depPkg.name}` : depPkg.name;
      const nextPath = [...path, label];
      visited.set(dep.pkg, { path: nextPath, viaBuild: nextViaBuild });
      // Inside a build-dependency, its own normal deps are needed at build time too.
      queue.push({ id: dep.pkg, path: nextPath, viaBuild: nextViaBuild });
    }
  }
  return visited;
}

// Rules 1, 2, 2b: Transitive downward layers, zero cross-feature deps, and core-crate isolation.
for (const src of [...wsMeta.values()].sort((a, b) => a.name.localeCompare(b.name))) {
  const reachable = resolveReachable(src.id, ["normal", "build"]);

  for (const [depId, info] of reachable) {
    const depPkg = pkgById.get(depId);
    const chain = info.path.join(" -> ");

    if (wsMeta.has(depId)) {
      const dst = wsMeta.get(depId);

      // Rule 1: Strict downward layer dependencies
      if (dst.rank > src.rank) {
        const msg = `${src.manifestRel}: upward dependency ${src.name} (${src.tier}) -> ${dst.name} (${dst.tier}) via ${chain}`;
        if (!checkException(exceptions, "layer", src.name, dst.name, msg)) {
          errors.push(msg);
        }
      }

      // Rule 2: Zero cross-feature crate dependencies
      if (src.tier === "feature" && dst.tier === "feature" && dst.id !== src.id) {
        const msg = `${src.manifestRel}: cross-feature dependency ${src.name} -> ${dst.name} via ${chain}; decouple via opencode-kernel`;
        if (!checkException(exceptions, "cross-feature", src.name, dst.name, msg)) {
          errors.push(msg);
        }
      }

      // Rule 2b: Core-crate isolation (transitive)
      const forbidden = CORE_ISOLATION[src.name];
      if (forbidden && forbidden.has(dst.name)) {
        const msg = `${src.manifestRel}: ${src.name} must not depend on ${dst.name} (resolved via ${chain})`;
        if (!checkException(exceptions, "isolation", src.name, dst.name, msg)) {
          errors.push(msg);
        }
      }
    }

    // Rule 2b (external): opencode-search must stay strictly free of rune / rune-* crates
    if (src.name === "opencode-search" && /^rune(?:-|$)/.test(depPkg.name)) {
      const msg = `${src.manifestRel}: opencode-search must stay free of rune/UI dependencies (found ${depPkg.name} via ${chain})`;
      if (!checkException(exceptions, "isolation", src.name, depPkg.name, msg)) {
        errors.push(msg);
      }
    }
  }
}

// Rule 1b: Full workspace DFS cycle check (normal + build edges between workspace members)
{
  const WHITE = 0, GRAY = 1, BLACK = 2;
  const color = new Map([...wsMeta.keys()].map((id) => [id, WHITE]));
  const stack = [];

  function dfs(u) {
    color.set(u, GRAY);
    stack.push(u);
    const node = nodeById.get(u);
    for (const dep of node?.deps ?? []) {
      if (!wsMeta.has(dep.pkg)) continue;
      if (!dep.dep_kinds.some((k) => k.kind === null || k.kind === "build")) continue;
      const v = dep.pkg;
      if (color.get(v) === GRAY) {
        const cycleIds = [...stack.slice(stack.indexOf(v)), v];
        const cycle = cycleIds.map((id) => wsMeta.get(id).name).join(" -> ");
        errors.push(`circular workspace crate dependency: ${cycle}`);
      } else if (color.get(v) === WHITE) {
        dfs(v);
      }
    }
    stack.pop();
    color.set(u, BLACK);
  }

  for (const id of [...wsMeta.keys()].sort()) {
    if (color.get(id) === WHITE) dfs(id);
  }
}

// Warn on stale exceptions whose edge no longer exists
for (const entry of exceptions.values()) {
  if (!entry.used) {
    warnings.push(`${EXCEPTIONS_FILE}:${entry.line}: stale exception "${entry.raw}" (edge no longer exists; delete this entry)`);
  }
}

// ── Rule 3: Core crates forbid unsafe_code ────────────────────────────────────

const FORBID_UNSAFE_RE = /^\s*#!\[forbid\(unsafe_code\)\]/m;

for (const coreLib of CORE_FORBID_UNSAFE) {
  if (!existsSync(join(root, coreLib))) {
    errors.push(`${coreLib}: missing core crate`);
    continue;
  }
  const clean = stripRust(readText(coreLib), { blankStrings: true });
  if (!FORBID_UNSAFE_RE.test(clean)) {
    errors.push(`${coreLib}: must declare #![forbid(unsafe_code)]`);
  }
}

// Collect tracked files once for Rules 4, 5, 7, 8, 9, 10, 11
const allTracked = gitLsFiles(root);
const trackedRs = allTracked.filter((f) => /^(apps|crates|features)\//.test(f) && f.endsWith(".rs"));

// ── Rule 4: apps/desktop/src/ root breadth ────────────────────────────────────

const desktopRootRs = trackedRs.filter((f) => /^apps\/desktop\/src\/[^/]+\.rs$/.test(f));
if (desktopRootRs.length > DESKTOP_ROOT_MAX_FILES) {
  errors.push(
    `apps/desktop/src/: ${desktopRootRs.length} root .rs files (max ${DESKTOP_ROOT_MAX_FILES}); ` +
      "group into domain folders (shell, session, workspace, surfaces, dialogs, plugin_host, stories)",
  );
}

// ── Rule 5: struct App effective field-count ratchet (counting through wrappers) ──

let appFieldStats = { direct: 0, wrappers: 0, wrapperSubfields: 0, effective: 0 };
{
  const appRs = "apps/desktop/src/app.rs";
  const appStateRs = "apps/desktop/src/shell/app_state.rs";
  if (!existsSync(join(root, appRs))) {
    errors.push(`${appRs}: missing App definition for field-count ratchet`);
  } else {
    // Gather plain field-cluster wrapper structs:
    //   1. Every struct defined in apps/desktop/src/shell/app_state.rs
    //   2. Any other struct in apps/desktop/src/**/*.rs (other than App itself) whose fields
    //      contain `Tracked<` — preventing bypassing Rule 5 by placing a cluster in another file.
    const wrapperStructs = new Map(); // structName -> fields[]

    if (existsSync(join(root, appStateRs))) {
      const clean = stripRust(readText(appStateRs), { blankStrings: true });
      for (const [name, body] of extractBraceStructs(clean)) {
        wrapperStructs.set(name, parseStructFields(body));
      }
    }

    for (const f of trackedRs) {
      if (!f.startsWith("apps/desktop/src/") || f === appRs || f === appStateRs) continue;
      const raw = readText(f);
      if (!raw.includes("Tracked<")) continue;
      const clean = stripRust(raw, { blankStrings: true });
      for (const [name, body] of extractBraceStructs(clean)) {
        const fields = parseStructFields(body);
        if (fields.some((fld) => /\bTracked</.test(fld.ty))) {
          wrapperStructs.set(name, fields);
        }
      }
    }

    const appClean = stripRust(readText(appRs), { blankStrings: true });
    const appStructs = extractBraceStructs(appClean);
    const appBody = appStructs.get("App");
    if (!appBody) {
      errors.push(`${appRs}: failed to parse \`pub struct App\` for field-count ratchet`);
    } else {
      const directFields = parseStructFields(appBody);

      function unwrapOuterBoxOption(ty) {
        let cur = ty.trim();
        for (;;) {
          const m = cur.match(/^(?:(?:std::boxed::)?Box|(?:std::option::)?Option)\s*<([\s\S]+)>$/);
          if (!m) break;
          cur = m[1].trim();
        }
        return cur;
      }

      function wrapperNameFor(ty) {
        // A field wrapped in `Tracked<...>` is a single reactive cell, not a plain cluster wrapper.
        if (/\bTracked</.test(ty)) return null;
        const inner = unwrapOuterBoxOption(ty);
        // Strip generic args if any, then take final path segment
        const base = inner.replace(/<[\s\S]*>$/, "").trim();
        const seg = base.split("::").pop();
        return seg && wrapperStructs.has(seg) ? seg : null;
      }

      function countExpanded(fields, seen) {
        let total = 0;
        for (const fld of fields) {
          const w = wrapperNameFor(fld.ty);
          if (w && !seen.has(w)) {
            const nextSeen = new Set(seen);
            nextSeen.add(w);
            total += countExpanded(wrapperStructs.get(w), nextSeen);
          } else {
            total += 1;
          }
        }
        return total;
      }

      let wrappers = 0;
      let wrapperSubfields = 0;
      let nonWrapperDirect = 0;
      for (const fld of directFields) {
        const w = wrapperNameFor(fld.ty);
        if (w) {
          wrappers++;
          wrapperSubfields += countExpanded(wrapperStructs.get(w), new Set([w]));
        } else {
          nonWrapperDirect++;
        }
      }
      const effective = nonWrapperDirect + wrapperSubfields;
      appFieldStats = { direct: directFields.length, wrappers, wrapperSubfields, effective };

      if (effective > APP_FIELD_CEILING) {
        errors.push(
          `${appRs}: struct App has ${effective} effective fields ` +
            `(${directFields.length} direct, ${wrappers} wrapper(s) expanding to ${wrapperSubfields} subfields; ` +
            `ceiling ${APP_FIELD_CEILING}); store new feature state in opencode_kernel::StateSlots or build a Luau plugin`,
        );
      }
    }
  }
}

// ── Rule 6: Unified web library (resolved from cargo metadata) ────────────────

{
  const web = wsByName.get("opencode-web");
  if (!web) {
    errors.push("apps/web/Cargo.toml: opencode-web missing from workspace members");
  } else {
    const webNode = nodeById.get(web.id);
    const hasDesktopNormal = webNode.deps.some(
      (d) =>
        wsMeta.get(d.pkg)?.name === "opencode-desktop" &&
        d.dep_kinds.some((k) => k.kind === null),
    );
    if (!hasDesktopNormal) {
      errors.push(`${web.manifestRel}: must have a normal dependency on opencode-desktop`);
    }
    const dupFeatures = webNode.deps
      .filter((d) => wsMeta.get(d.pkg)?.tier === "feature" && d.dep_kinds.some((k) => k.kind === null || k.kind === "build"))
      .map((d) => wsMeta.get(d.pkg).name)
      .sort();
    if (dupFeatures.length > 0) {
      errors.push(
        `${web.manifestRel}: duplicates features/* dependencies (${dupFeatures.join(", ")}); depend on opencode-desktop instead`,
      );
    }
  }
}

// ── Rules 7, 8, 10: Tracked Rust source scans ─────────────────────────────────

const TEST_PATH_RE = /(^|\/)(tests\.rs|[^/]+_tests?\.rs|tests\/)/;
const TEST_ATTR_RE = /#\[\s*(?:test\b|cfg\s*\([^)]*\btest\b)/;
const CRATE_WILDCARD_RE = /(?:^|[;\n{}])\s*(?:pub(?:\([^)]*\))?\s+)?use\s+crate\s*::\s*\*\s*;/;

for (const f of trackedRs) {
  if (!existsSync(join(root, f))) continue;
  const raw = readText(f);

  // Rule 7: Zero automated tests (attributes outside comments/strings AND test filenames)
  if (TEST_PATH_RE.test(f)) {
    errors.push(`${f}: test file path forbidden by AGENTS.md (no automated tests)`);
  }
  const clean = stripRust(raw, { blankStrings: true });
  if (TEST_ATTR_RE.test(clean)) {
    errors.push(`${f}: contains #[test] or #[cfg(...test...)] (forbidden by AGENTS.md)`);
  }

  // Rule 8: Hard <= 800 line ceiling in apps/desktop/, features/, and hand-written workspace crates/
  const inHardCapScope =
    f.startsWith("apps/desktop/") ||
    f.startsWith("features/") ||
    (f.startsWith("crates/") &&
      !f.startsWith("crates/opencode-plugin-spec/src/generated/") &&
      !f.startsWith("crates/browser-pane/"));
  if (inHardCapScope) {
    const lines = countLines(raw);
    if (lines > HARD_LINE_CAP) {
      errors.push(`${f}: ${lines} lines exceeds hard ceiling of ${HARD_LINE_CAP}`);
    }
  }

  // Rule 10: Zero `use crate::*;` wildcards in apps/desktop/src/
  if (f.startsWith("apps/desktop/src/") && CRATE_WILDCARD_RE.test(clean)) {
    errors.push(`${f}: contains \`use crate::*;\` wildcard import; use explicit imports instead`);
  }
}

// ── Rule 9: Pure feature crates forbid unsafe_code ────────────────────────────

const featureLibs = trackedRs.filter((f) => /^features\/[^/]+\/src\/lib\.rs$/.test(f)).sort();
const seenAllowlistedFeatures = new Set();
for (const lib of featureLibs) {
  const feat = lib.split("/")[1];
  const clean = stripRust(readText(lib), { blankStrings: true });
  const forbids = FORBID_UNSAFE_RE.test(clean);
  if (FEATURE_UNSAFE_ALLOWLIST.has(feat)) {
    seenAllowlistedFeatures.add(feat);
    if (forbids) {
      warnings.push(`${lib}: listed in FEATURE_UNSAFE_ALLOWLIST but already declares #![forbid(unsafe_code)]; remove from allowlist`);
    }
    continue;
  }
  if (!forbids) {
    errors.push(`${lib}: pure feature crate must declare #![forbid(unsafe_code)]`);
  }
}
for (const feat of FEATURE_UNSAFE_ALLOWLIST) {
  if (!seenAllowlistedFeatures.has(feat)) {
    errors.push(`FEATURE_UNSAFE_ALLOWLIST contains nonexistent feature "${feat}"`);
  }
}

// ── Rule 11: Structural workspace conventions ─────────────────────────────────

// 11a: Feature crates follow `opencode-feature-<dir>` naming
for (const info of wsMeta.values()) {
  if (info.tier === "feature") {
    const dirName = info.relDir.split("/")[1];
    const expected = `opencode-feature-${dirName}`;
    if (info.name !== expected) {
      errors.push(`${info.manifestRel}: feature crate name "${info.name}" must match "${expected}"`);
    }
  }
}

// 11b: Every tracked Cargo.toml under apps/, crates/, features/ (except excluded sidecars) is in workspace.members
const wsManifestSet = new Set([...wsMeta.values()].map((m) => m.manifestRel));
for (const m of allTracked.filter((f) => /^(apps|crates|features)\/[^/]+\/Cargo\.toml$/.test(f))) {
  if (EXCLUDED_MANIFESTS.has(m)) continue;
  if (!wsManifestSet.has(m)) {
    errors.push(`${m}: crate manifest is missing from root Cargo.toml [workspace.members]`);
  }
}

// 11d: Built-in Luau plugins (directories with plugin.toml) use `--!strict`
{
  const pluginDirs = new Set(
    allTracked
      .filter((f) => /^plugins\/[^/]+\/plugin\.toml$/.test(f))
      .map((f) => f.split("/")[1]),
  );
  for (const f of allTracked.filter((p) => /^plugins\/[^/]+\/.*\.luau$/.test(p) && !p.endsWith(".d.luau"))) {
    const dir = f.split("/")[1];
    if (!pluginDirs.has(dir)) continue;
    const text = readText(f).replace(/^\uFEFF/, "");
    const firstLine = text.split(/\r?\n/, 1)[0]?.trim() ?? "";
    if (firstLine !== "--!strict") {
      errors.push(`${f}: built-in Luau plugin source must start with \`--!strict\``);
    }
  }
}

// ── Report ────────────────────────────────────────────────────────────────────

for (const n of notes) console.log(`  NOTE   ${n}`);
for (const w of warnings) console.log(`  WARN   ${w}`);
if (errors.length > 0) {
  for (const e of errors) console.error(`  ERROR  ${e}`);
  console.error(`arch check: ${errors.length} error(s)`);
  process.exit(1);
}

const featCount = [...wsMeta.values()].filter((m) => m.tier === "feature").length;
const crateCount = [...wsMeta.values()].filter((m) => m.tier === "crate").length;
console.log(
  `arch check: ok ` +
    `(${featCount} features, ${crateCount} crates, ${desktopRootRs.length} apps/desktop/src root files, ` +
    `${appFieldStats.effective}/${APP_FIELD_CEILING} effective App fields ` +
    `[${appFieldStats.direct} direct, ${appFieldStats.wrappers} wrappers -> ${appFieldStats.wrapperSubfields} subfields], ` +
    `0 cycles, 0 files >${HARD_LINE_CAP} in desktop/features/crates, 0 crate::* wildcards)`,
);
