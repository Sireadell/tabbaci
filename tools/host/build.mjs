// Copies site/ into dist/ as a self-contained static folder.
// Works on any free static host: no server code, no build step, relative links only.
// Usage: node tools/host/build.mjs
import { cp, rm, readFile, readdir, stat } from 'node:fs/promises';
import { join, relative, dirname, resolve, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

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

const files = await walk(dist);
const bad = await checkRelativeLinks(files);
if (bad.length) {
  console.error('Root-relative links found, fix them in site/:');
  for (const b of bad) console.error('  ' + b);
  process.exit(1);
}
console.log(`Built dist/ with ${files.length} files from site/.`);
for (const f of files) console.log('  ' + relative(dist, f).replace(/\\/g, '/'));
