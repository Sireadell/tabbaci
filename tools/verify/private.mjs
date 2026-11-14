// Private court checks (program version 3, kind "private"): a sentence about a page only the writer's key can read.
// The page never leaves the sealed chip, so there is nothing to compare it with. What a stranger can check:
//   the verdict carries only the page fingerprint and one bit, never page text or the number on the page,
//   the contract recorded that same fingerprint,
//   each judge signed an answer to the exact question the court asks (sentence, public address, comparison),
//   and the contract checked those same signatures,
//   the ruling follows from the signed answers and the bit.
// The question text is byte for byte programs/court-private/src/frames.rs private_user.
import { sha256hex } from './crypto.mjs';
import { SYSTEM, canon, dataOutcome } from './frames.mjs';
import { requestBodyV2, ed25519Ok, contentOf } from './v2.mjs';

export const OP_WORDS = { gt: 'more than', ge: 'at least', lt: 'fewer than', le: 'at most', eq: 'exactly' };
export const privateUser = (claim, url, words, cv) =>
  `Sentence: <untrusted>${claim}</untrusted>\nSource: <untrusted>${url}</untrusted>\nQuestion: does this sentence say that the number read from this source is ${words} ${cv}? Reply TRUE, FALSE or UNSURE.`;

const letter = (w) => (w === 'TRUE' ? 'T' : w === 'FALSE' ? 'F' : 'U');
const SRC_KEYS = ['i', 'c', 'h', 'y'];

/** True when a private verdict holds nothing of the page beyond its fingerprint and the one bit. */
export function privateShapeOk(v) {
  if ('ex' in v) return false;
  const s = v.s || [];
  if (v.o === 'X') return s.length === 0;
  if (s.length !== 1) return false;
  if (!Object.keys(s[0]).every((k) => SRC_KEYS.includes(k))) return false;
  if (s[0].c !== 200 || typeof s[0].y !== 'boolean') return false;
  return (v.j || []).every((j) => !('q' in j));
}

const fpBytes = (h) => { try { const b = Buffer.from(h || '', 'base64url'); return b.length === 32 && b.toString('base64url') === h ? b : null; } catch { return null; } };

/**
 * Rows for a private ruling.
 *  r         the round as the contract stores it (fingerprint and checked judges included)
 *  input     the parsed stored request
 *  v         the parsed verdict
 *  page      the page bytes, only for the person who holds them (Buffer or null)
 *  attested  { model: [hex ed25519 keys Intel vouches for] } or null
 */
export function checkPrivate({ r, input, v, page, attested }) {
  const M = 'MATCH', X = 'MISMATCH', C = 'CANNOT VERIFY', I = 'INFO';
  const rows = [];
  const add = (status, name, detail = '') => rows.push({ status, name, detail });
  const src = (input.src || [])[0] || {};

  add(input.src?.length === 1 && /^PRIVATE_[A-Z0-9_]{1,40}$/.test(src.sk || '') && src.h === 'api.github.com' && String(src.u || '').startsWith(`https://${src.h}/`) ? M : X,
    'private request names one source, its host and its secret by name only', `${src.h} via ${src.sk}`);
  add(privateShapeOk(v) ? M : X, 'verdict carries no page text, value or quote', `source fields: ${Object.keys((v.s || [])[0] || {}).join(', ') || 'none'}`);
  if (v.o === 'X') { add(I, 'private ruling', `the sealed program stopped with code ${v.rc}; nothing was ruled`); return rows; }

  const s = (v.s || [])[0] || {};
  const fp = fpBytes(s.h);
  add(fp ? M : X, 'fingerprint recorded in the verdict', fp ? `sha256 ${fp.toString('hex')}` : 'not a sha256 fingerprint');
  if (r.fingerprint == null) add(C, 'contract record matches the fingerprint', 'this round has no fingerprint record');
  else add(r.fingerprint === s.h ? M : X, 'contract record matches the fingerprint', r.fingerprint === s.h ? 'same fingerprint on chain' : `contract has ${r.fingerprint}`);
  if (page) add(fp && sha256hex(page) === fp.toString('hex') ? M : X, 'the page you hold matches the fingerprint', `${page.length} bytes`);
  else add(I, 'the page itself', 'stays private; whoever holds it can rerun this with --archive to compare it with the fingerprint');

  const words = OP_WORDS[input.op];
  if (!words) { add(X, 'comparison in the request', `unknown op ${input.op}`); return rows; }
  const user = privateUser(input.claim, src.u, words, canon(Number(input.cv)));
  const stored = r.checked || [];
  if (!stored.length) add(C, 'signatures checked by the contract', 'this round has no contract check record');

  const counted = [];
  for (const j of v.j || []) {
    const model = input.m[j.m];
    const tag = `judge ${j.m} (${model})`;
    const rec = stored[j.m];
    if (j.a === 'X') {
      counted.push('U');
      add(M, `${tag} was not counted`, 'no signed reply, treated as UNSURE');
      if (rec) add(rec.answer === 'X' && !rec.sig_ok ? M : X, `${tag} contract agrees it was not counted`);
      continue;
    }
    const resp = Buffer.from(j.r, 'base64url');
    const key = Buffer.from(j.k, 'base64url');
    const rh = sha256hex(Buffer.from(requestBodyV2(model, SYSTEM, user, input.seed, 4)));
    const ok = ed25519Ok(`${model}:${rh}:${sha256hex(resp)}`, Buffer.from(j.g, 'base64url'), key);
    add(ok ? M : X, `${tag} signature verifies`, ok ? 'over the exact question the court asks, which holds no page text' : 'the signature does not verify for this question');
    if (rec) {
      const same = rec.sig_ok && rec.sig === j.g && rec.key === j.k;
      add(ok && same ? M : X, `${tag} contract checked the same signature`, same ? 'signature and key recorded on chain' : 'the contract record differs');
    } else add(C, `${tag} contract checked the same signature`, 'no contract record to compare');
    const keyHex = key.toString('hex');
    const want = attested && attested[model.toLowerCase()];
    if (want) add(want.map((x) => x.toLowerCase()).includes(keyHex) ? M : X, `${tag} signed by a key Intel vouches for`, keyHex.slice(0, 16) + '...');
    else add(C, `${tag} signed by a key Intel vouches for`, `key ${keyHex.slice(0, 16)}...; run with --attested-judges to compare`);
    const said = letter((contentOf(resp) ?? '').trim());
    counted.push(said);
    if (j.a !== said) add(X, `${tag} answer matches its signed reply`, `output says ${j.a}, signed reply gives ${said}`);
  }
  const [o, w] = dataOutcome(counted, s.y === true);
  add(o === v.o && !!w === !!v.w ? M : X, 'outcome follows from the signed answers and the page bit', `answers ${counted.join('')}, page ${s.y ? 'meets' : 'does not meet'} "${words} ${input.cv}" give ${o}`);
  if (stored.length) add(I, 'contract recorded', stored.map((c) => `${c.model}: ${c.answer}${c.sig_ok ? ' sig ok' : ''}`).join('; '));
  return rows;
}
