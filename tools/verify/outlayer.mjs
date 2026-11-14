// Finds the sealed-box checker (outlayer-verify) without asking the visitor to install Rust.
// Order: a copy already on the PATH, then a copy in .tools/, then a download from this repo's release,
// accepted only when its sha256 equals the fingerprint written in outlayer-verify.sums.json.
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const here = dirname(fileURLToPath(import.meta.url));
export const SUMS_FILE = join(here, 'outlayer-verify.sums.json');
const TOOLS_DIR = resolve(here, '..', '..', '.tools');

export const platformKey = (platform = process.platform, arch = process.arch) => {
  const os = { linux: 'linux', darwin: 'macos', win32: 'windows' }[platform];
  const cpu = { x64: 'x64', arm64: 'arm64' }[arch];
  return os && cpu ? `${os}-${cpu}` : null;
};

export const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

// sums.json looks like { "release": "https://github.com/.../releases/download/<tag>", "files": { "linux-x64": { "name": "...", "sha256": "..." } } }
export function readSums(file = SUMS_FILE) {
  if (!existsSync(file)) return null;
  try { return JSON.parse(readFileSync(file, 'utf8')); } catch { return null; }
}

// Returns the command to run, or { missing: 'reason' }. Never runs a download whose fingerprint is not written in the repo.
export async function findOutlayerVerify({ fetchImpl = fetch, sums = readSums(), key = platformKey(), toolsDir = TOOLS_DIR } = {}) {
  const onPath = spawnSync('outlayer-verify', ['--version'], { encoding: 'utf8' });
  if (!onPath.error) return { cmd: 'outlayer-verify', source: 'installed on this machine' };
  if (!key) return { missing: `no prebuilt copy for ${process.platform}/${process.arch}` };
  const entry = sums?.files?.[key];
  if (!entry || !entry.sha256) return { missing: 'no fingerprint for a prebuilt copy is written in the repo yet' };
  const local = join(toolsDir, entry.name);
  if (existsSync(local) && sha256(readFileSync(local)) === entry.sha256) return { cmd: local, source: `downloaded copy ${entry.name} (fingerprint ${entry.sha256.slice(0, 12)}...)` };
  try {
    const r = await fetchImpl(`${sums.release}/${entry.name}`);
    if (!r.ok) return { missing: `could not download ${entry.name} (HTTP ${r.status})` };
    const buf = Buffer.from(await r.arrayBuffer());
    const got = sha256(buf);
    if (got !== entry.sha256) return { mismatch: `downloaded ${entry.name} has fingerprint ${got.slice(0, 12)}..., the repo says ${entry.sha256.slice(0, 12)}...` };
    mkdirSync(toolsDir, { recursive: true });
    writeFileSync(local, buf);
    if (process.platform !== 'win32') chmodSync(local, 0o755);
    return { cmd: local, source: `downloaded copy ${entry.name} (fingerprint ${entry.sha256.slice(0, 12)}...)` };
  } catch (e) { return { missing: `could not download ${entry.name}: ${String(e.message).slice(0, 80)}` }; }
}
