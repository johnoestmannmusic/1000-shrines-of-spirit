#!/usr/bin/env node
/**
 * Dependency-provenance audit (slopsquatting guard).
 *
 * Slopsquatting is the supply-chain attack where an AI-hallucinated or
 * typosquatted package name is registered by an attacker. This script:
 *   1. Fails on any direct dependency not present in the reviewed allowlist.
 *   2. Verifies every allowlisted direct dependency exists on the npm registry
 *      and resolves to the expected upstream repository.
 *   3. Checks the installed version is actually published and that the
 *      package-lock integrity matches the registry's published integrity.
 *   4. Checks a minimum package age and weekly download count.
 *   5. Audits every transitive entry in package-lock.json for a registry.npmjs.org
 *      resolved URL and a sha512 integrity hash (lockfile-tamper guard).
 *
 * Usage: node scripts/audit-dependencies.mjs [--offline]
 */
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const projectRoot = fileURLToPath(new URL("../", import.meta.url));
const offline = process.argv.includes("--offline");

const MIN_AGE_DAYS = 30;
const MIN_WEEKLY_DOWNLOADS = 10_000;

/** Reviewed direct dependencies and the upstream repo each must resolve to. */
const ALLOWED = {
  react: /github\.com\/(facebook|react)\/react/i,
  "react-dom": /github\.com\/(facebook|react)\/react/i,
  "@vitejs/plugin-react": /github\.com\/vitejs\/vite-plugin-react/i,
  electron: /github\.com\/electron\/electron/i,
  esbuild: /github\.com\/evanw\/esbuild/i,
  jsdom: /github\.com\/jsdom\/jsdom/i,
  typescript: /github\.com\/microsoft\/TypeScript/i,
  vite: /github\.com\/vitejs\/vite/i,
  vitest: /github\.com\/vitest-dev\/vitest/i,
  "@playwright/test": /github\.com\/microsoft\/playwright/i,
  "@types/node": /github\.com\/DefinitelyTyped\/DefinitelyTyped/i,
  "@types/react": /github\.com\/DefinitelyTyped\/DefinitelyTyped/i,
  "@types/react-dom": /github\.com\/DefinitelyTyped\/DefinitelyTyped/i,
};

const failures = [];
const warnings = [];
const checks = [];

function fail(message) {
  failures.push(message);
  console.error(`  FAIL  ${message}`);
}
function warn(message) {
  warnings.push(message);
  console.warn(`  WARN  ${message}`);
}
function ok(message) {
  checks.push(message);
  console.log(`  ok    ${message}`);
}

async function fetchJson(url) {
  let lastError;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const res = await fetch(url, { headers: { accept: "application/json" } });
      if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
      return await res.json();
    } catch (e) {
      lastError = e;
      await new Promise((resolve) => setTimeout(resolve, 400 * (attempt + 1)));
    }
  }
  throw lastError;
}

function repoUrl(metadata) {
  const repo = metadata.repository;
  if (!repo) return null;
  return typeof repo === "string" ? repo : repo.url ?? null;
}

function daysSince(dateString) {
  if (!dateString) return 0;
  return (Date.now() - new Date(dateString).getTime()) / 86_400_000;
}

const pkg = JSON.parse(readFileSync(path.join(projectRoot, "package.json"), "utf8"));
const lockPath = path.join(projectRoot, "package-lock.json");
const lock = existsSync(lockPath)
  ? JSON.parse(readFileSync(lockPath, "utf8"))
  : { packages: {} };

const direct = {
  ...(pkg.dependencies ?? {}),
  ...(pkg.devDependencies ?? {}),
};

console.log("\nDirect dependency allowlist check");
for (const name of Object.keys(direct)) {
  if (!ALLOWED[name]) {
    fail(`unreviewed direct dependency "${name}" (possible slopsquat/hallucination)`);
  }
}

if (!offline) {
  console.log("\nRegistry provenance checks (direct dependencies)");
  for (const [name, expectedRepo] of Object.entries(ALLOWED)) {
    if (!(name in direct)) continue; // allowlist entry not currently a dependency
    try {
      const meta = await fetchJson(`https://registry.npmjs.org/${encodeURIComponent(name)}`);
      const latest = meta["dist-tags"]?.latest;
      const versionMeta = latest ? meta.versions?.[latest] : undefined;
      if (!versionMeta) {
        fail(`${name}: no published latest version`);
        continue;
      }
      const repo = repoUrl(versionMeta) ?? repoUrl(meta);
      if (!repo || !expectedRepo.test(repo)) {
        fail(`${name}: repository "${repo}" does not match expected ${expectedRepo}`);
      } else {
        ok(`${name}: repository ${repo}`);
      }

      const installedRange = direct[name];
      const lockEntry = lock.packages?.[`node_modules/${name}`];
      const installedVersion = lockEntry?.version;
      if (installedVersion && !meta.versions?.[installedVersion]) {
        fail(`${name}: installed version ${installedVersion} is not published`);
      }
      if (installedVersion && lockEntry?.integrity) {
        const published = meta.versions[installedVersion]?.dist?.integrity;
        if (published && published !== lockEntry.integrity) {
          fail(`${name}: lock integrity does not match registry for ${installedVersion}`);
        } else {
          ok(`${name}: lock integrity matches registry (${installedVersion})`);
        }
      }

      const created = meta.time?.created;
      if (daysSince(created) < MIN_AGE_DAYS) {
        fail(`${name}: package is only ${daysSince(created).toFixed(1)} days old`);
      }

      const downloads = await fetchJson(
        `https://api.npmjs.org/downloads/point/last-week/${name}`,
      ).catch(() => null);
      if (downloads && typeof downloads.downloads === "number") {
        if (downloads.downloads < MIN_WEEKLY_DOWNLOADS) {
          warn(
            `${name}: only ${downloads.downloads.toLocaleString()} weekly downloads (< ${MIN_WEEKLY_DOWNLOADS.toLocaleString()})`,
          );
        } else {
          ok(`${name}: ${downloads.downloads.toLocaleString()} weekly downloads`);
        }
      }
      void installedRange;
    } catch (e) {
      fail(`${name}: registry check failed: ${e.message}`);
    }
  }
} else {
  console.log("\n(offline) skipping registry provenance checks");
}

console.log("\nLockfile integrity check (all transitive dependencies)");
let locked = 0;
let badResolved = 0;
let badIntegrity = 0;
for (const [key, entry] of Object.entries(lock.packages ?? {})) {
  if (!key || key === "") continue;
  locked += 1;
  if (entry.resolved && !entry.resolved.startsWith("https://registry.npmjs.org/")) {
    badResolved += 1;
    fail(`${key}: resolved URL is not the npm registry (${entry.resolved})`);
  }
  if (entry.integrity && !/^sha512-/.test(entry.integrity)) {
    badIntegrity += 1;
    fail(`${key}: integrity is not sha512`);
  }
  if (!entry.integrity && entry.resolved) {
    badIntegrity += 1;
    fail(`${key}: missing integrity hash`);
  }
}
ok(`${locked} locked packages inspected (${badResolved} bad resolved URLs, ${badIntegrity} bad integrity)`);

console.log("\nSummary");
console.log(`  checks passed: ${checks.length}`);
console.log(`  warnings:      ${warnings.length}`);
console.log(`  failures:      ${failures.length}`);

if (failures.length > 0) {
  console.error("\nDependency audit FAILED");
  process.exit(1);
}
console.log("\nDependency audit passed");
