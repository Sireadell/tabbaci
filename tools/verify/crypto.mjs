// Small, dependency-free crypto for the checker: keccak256, sha256, and Ethereum signature recovery.
import { createHash } from 'node:crypto';

export const sha256 = (buf) => createHash('sha256').update(buf).digest();
export const sha256hex = (buf) => sha256(buf).toString('hex');
export const b64url = (buf) => Buffer.from(buf).toString('base64url');

// ---- keccak256 (the Ethereum one, not NIST SHA3) ----
const MASK = (1n << 64n) - 1n;
const RC = [
  0x0000000000000001n, 0x0000000000008082n, 0x800000000000808an, 0x8000000080008000n,
  0x000000000000808bn, 0x0000000080000001n, 0x8000000080008081n, 0x8000000000008009n,
  0x000000000000008an, 0x0000000000000088n, 0x0000000080008009n, 0x000000008000000an,
  0x000000008000808bn, 0x800000000000008bn, 0x8000000000008089n, 0x8000000000008003n,
  0x8000000000008002n, 0x8000000000000080n, 0x000000000000800an, 0x800000008000000an,
  0x8000000080008081n, 0x8000000000008080n, 0x0000000080000001n, 0x8000000080008008n,
];
const ROT = [
  [0, 36, 3, 41, 18], [1, 44, 10, 45, 2], [62, 6, 43, 15, 61], [28, 55, 25, 21, 56], [27, 20, 39, 8, 14],
];
const rotl = (x, n) => (n === 0 ? x : ((x << BigInt(n)) | (x >> BigInt(64 - n))) & MASK);

function keccakF(s) {
  for (let round = 0; round < 24; round++) {
    const c = [];
    for (let x = 0; x < 5; x++) c[x] = s[x] ^ s[x + 5] ^ s[x + 10] ^ s[x + 15] ^ s[x + 20];
    for (let x = 0; x < 5; x++) {
      const d = c[(x + 4) % 5] ^ rotl(c[(x + 1) % 5], 1);
      for (let y = 0; y < 5; y++) s[x + 5 * y] ^= d;
    }
    const b = new Array(25).fill(0n);
    for (let x = 0; x < 5; x++) {
      for (let y = 0; y < 5; y++) b[y + 5 * ((2 * x + 3 * y) % 5)] = rotl(s[x + 5 * y], ROT[x][y]);
    }
    for (let x = 0; x < 5; x++) {
      for (let y = 0; y < 5; y++) s[x + 5 * y] = b[x + 5 * y] ^ (~b[((x + 1) % 5) + 5 * y] & MASK & b[((x + 2) % 5) + 5 * y]);
    }
    s[0] ^= RC[round];
  }
}

export function keccak256(data) {
  const rate = 136;
  const msg = Buffer.from(data);
  const padLen = rate - (msg.length % rate);
  const padded = Buffer.alloc(msg.length + padLen);
  msg.copy(padded);
  padded[msg.length] ^= 0x01;
  padded[padded.length - 1] ^= 0x80;
  const s = new Array(25).fill(0n);
  for (let off = 0; off < padded.length; off += rate) {
    for (let i = 0; i < rate / 8; i++) s[i] ^= padded.readBigUInt64LE(off + i * 8);
    keccakF(s);
  }
  const out = Buffer.alloc(32);
  for (let i = 0; i < 4; i++) out.writeBigUInt64LE(s[i], i * 8);
  return out;
}

// ---- secp256k1 recovery ----
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
const N = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141n;
const GX = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
const GY = 0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n;

const mod = (a, m = P) => ((a % m) + m) % m;
function inv(a, m = P) {
  let [r0, r1] = [mod(a, m), m];
  let [s0, s1] = [1n, 0n];
  while (r1 !== 0n) {
    const q = r0 / r1;
    [r0, r1] = [r1, r0 - q * r1];
    [s0, s1] = [s1, s0 - q * s1];
  }
  return mod(s0, m);
}
function pow(b, e, m) {
  let r = 1n;
  b = mod(b, m);
  while (e > 0n) {
    if (e & 1n) r = (r * b) % m;
    b = (b * b) % m;
    e >>= 1n;
  }
  return r;
}
function add(p, q) {
  if (!p) return q;
  if (!q) return p;
  if (p[0] === q[0]) {
    if (mod(p[1] + q[1]) === 0n) return null;
    const l = mod(3n * p[0] * p[0] * inv(2n * p[1]));
    const x = mod(l * l - 2n * p[0]);
    return [x, mod(l * (p[0] - x) - p[1])];
  }
  const l = mod((q[1] - p[1]) * inv(q[0] - p[0]));
  const x = mod(l * l - p[0] - q[0]);
  return [x, mod(l * (p[0] - x) - p[1])];
}
function mul(k, p) {
  let r = null;
  while (k > 0n) {
    if (k & 1n) r = add(r, p);
    p = add(p, p);
    k >>= 1n;
  }
  return r;
}
const bigFromBuf = (b) => BigInt('0x' + Buffer.from(b).toString('hex'));

export function eip191Hash(text) {
  const t = Buffer.from(text, 'utf8');
  return keccak256(Buffer.concat([Buffer.from(`\x19Ethereum Signed Message:\n${t.length}`), t]));
}

/** Recover the Ethereum address that signed `text` (EIP-191). `sig` is 65 bytes r|s|v. Returns '0x..' or null. */
export function recoverAddress(text, sig) {
  sig = Buffer.from(sig);
  if (sig.length !== 65) return null;
  const v = sig[64];
  const recid = v === 0 || v === 27 ? 0 : v === 1 || v === 28 ? 1 : -1;
  if (recid < 0) return null;
  const r = bigFromBuf(sig.subarray(0, 32));
  const s = bigFromBuf(sig.subarray(32, 64));
  if (r <= 0n || r >= N || s <= 0n || s >= N) return null;
  const y2 = mod(r * r * r + 7n);
  let y = pow(y2, (P + 1n) / 4n, P);
  if (mod(y * y) !== y2) return null;
  if (Number(y & 1n) !== recid) y = P - y;
  const R = [r, y];
  const z = bigFromBuf(eip191Hash(text));
  const rinv = inv(r, N);
  const sR = mul(s, R);
  const zG = mul(mod(N - (z % N), N), [GX, GY]);
  const Q = mul(rinv, add(sR, zG));
  if (!Q) return null;
  const xy = Buffer.concat([Buffer.from(Q[0].toString(16).padStart(64, '0'), 'hex'), Buffer.from(Q[1].toString(16).padStart(64, '0'), 'hex')]);
  return '0x' + keccak256(xy).subarray(12).toString('hex');
}
