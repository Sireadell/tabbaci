// Copies site/ into dist/ as a self-contained static folder.
// Static pages plus one small server function for Vercel. Relative links only.
// Usage: node tools/host/build.mjs
import { cp, rm, readFile, readdir, stat, writeFile } from 'node:fs/promises';
import { join, relative, dirname, resolve, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { RAW_STATUS, readLive, statusFor } from '../recheck/status.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const src = join(root, 'site');
const dist = join(root, 'dist');

async function walk(dir) {
  const out = [];
  for (const name of await readdir(dir)) {
    const p = join(dir, name);
    if ((await stat(p)).isDirectory()) out.push(...(await walk(p)));
    else out.push(p);
  }
  return out;
}

// Refuse root-relative links ("/x.css"): they break on a host that serves the folder from a sub-path.
async function checkRelativeLinks(files) {
  const bad = [];
  const pattern = /(href|src)=["'](\/(?!\/)[^"']*)["']/g;
  for (const f of files.filter((f) => extname(f) === '.html')) {
    const text = await readFile(f, 'utf8');
    for (const m of text.matchAll(pattern)) bad.push(`${relative(src, f)}: ${m[1]}="${m[2]}"`);
  }
  return bad;
}

await rm(dist, { recursive: true, force: true });
await cp(src, dist, { recursive: true });

// The one server function: lets a visitor send a sentence with no wallet (api/write.js). Tests stay out of the upload.
await cp(join(root, 'api'), join(dist, 'api'), { recursive: true, filter: (f) => !f.endsWith('.test.js') });
const json = (o) => JSON.stringify(o, null, 2) + String.fromCharCode(10);
await writeFile(join(dist, 'package.json'), json({ private: true, type: 'module', engines: { node: '22.x' } }));
await writeFile(join(dist, 'vercel.json'), json({ functions: { 'api/write.js': { maxDuration: 60 } } }));

// /status.json: live numbers from the chain plus the last nightly re-check result (published on the "status" branch).
let last = null;
try { const r = await fetch(RAW_STATUS); if (r.ok) last = (await r.json()).last_recheck; } catch { /* the site still builds with no result yet */ }
const status = statusFor({ report: null, live: await readLive() });
status.last_recheck = last;
await writeFile(join(dist, 'status.json'), json(status));

const files = await walk(dist);
const bad = await checkRelativeLinks(files);
if (bad.length) {
  console.error('Root-relative links found, fix them in site/:');
  for (const b of bad) console.error('  ' + b);
  process.exit(1);
}
console.log(`Built dist/ with ${files.length} files from site/.`);
for (const f of files) console.log('  ' + relative(dist, f).replace(/\\/g, '/'));
