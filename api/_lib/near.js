// Minimal NEAR testnet client for the write function: reads, and signed function calls.
// No outside libraries: borsh by hand, ed25519 from Node's own crypto.
import { createHash, createPrivateKey, sign as edSign } from 'node:crypto';

const NODES = ['https://test.rpc.fastnear.com', 'https://rpc.testnet.near.org'];
const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function b58decode(s) {
  let n = 0n;
  for (const c of s) {
    const i = B58.indexOf(c);
    if (i < 0) throw new Error('bad base58');
    n = n * 58n + BigInt(i);
  }
  const out = [];
  while (n > 0n) { out.unshift(Number(n % 256n)); n /= 256n; }
  for (const c of s) { if (c !== '1') break; out.unshift(0); }
  return Uint8Array.from(out);
}
export function b58encode(bytes) {
  let n = 0n;
  for (const b of bytes) n = n * 256n + BigInt(b);
  let s = '';
  while (n > 0n) { s = B58[Number(n % 58n)] + s; n /= 58n; }
  for (const b of bytes) { if (b !== 0) break; s = '1' + s; }
  return s;
}

// "ed25519:<base58 of 64 bytes>" holds the 32-byte seed followed by the 32-byte public key.
export function loadKey(secret) {
  const raw = b58decode(secret.replace(/^ed25519:/, ''));
  if (raw.length !== 64) throw new Error('key must be 64 bytes');
  const seed = raw.subarray(0, 32), pub = raw.subarray(32);
  const der = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.from(seed)]);
  return { pub: Uint8Array.from(pub), pubText: 'ed25519:' + b58encode(pub), priv: createPrivateKey({ key: der, format: 'der', type: 'pkcs8' }) };
}

class Writer {
  constructor() { this.parts = []; }
  u8(n) { this.parts.push(Buffer.from([n])); }
  u32(n) { const b = Buffer.alloc(4); b.writeUInt32LE(n); this.parts.push(b); }
  u64(n) { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); this.parts.push(b); }
  u128(n) { const v = BigInt(n); const b = Buffer.alloc(16); b.writeBigUInt64LE(v & 0xffffffffffffffffn); b.writeBigUInt64LE(v >> 64n, 8); this.parts.push(b); }
  bytes(u8) { this.parts.push(Buffer.from(u8)); }
  str(s) { const b = Buffer.from(s, 'utf8'); this.u32(b.length); this.parts.push(b); }
  vec(u8) { this.u32(u8.length); this.bytes(u8); }
  done() { return Buffer.concat(this.parts); }
}

// Transaction with one FunctionCall action, laid out as NEAR's borsh schema.
export function encodeTx({ signer, pub, nonce, receiver, blockHash, method, args, gas, deposit }) {
  const w = new Writer();
  w.str(signer); w.u8(0); w.bytes(pub); w.u64(nonce); w.str(receiver); w.bytes(blockHash);
  w.u32(1); w.u8(2); w.str(method); w.vec(Buffer.from(JSON.stringify(args))); w.u64(gas); w.u128(deposit);
  return w.done();
}
export function signTx(txBytes, priv) {
  const hash = createHash('sha256').update(txBytes).digest();
  const sig = edSign(null, hash, priv);
  const w = new Writer(); w.bytes(txBytes); w.u8(0); w.bytes(sig);
  return { signed: w.done(), hash: b58encode(hash) };
}

export async function rpc(method, params, timeoutMs = 20000) {
  let last;
  for (const url of [...NODES, ...NODES]) {
    if (last) await new Promise((ok) => setTimeout(ok, 300));
    try {
      const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }), signal: AbortSignal.timeout(timeoutMs) });
      const j = await r.json();
      if (j.error) { last = new Error(JSON.stringify(j.error).slice(0, 400)); if (/InvalidNonce|InvalidTxError|ActionError|FunctionCallError/.test(last.message)) throw last; continue; }
      return j.result;
    } catch (e) { last = e; if (/InvalidNonce|InvalidTxError|ActionError|FunctionCallError/.test(e.message)) throw e; }
  }
  throw last || new Error('no node answered');
}

export async function view(contract, method, args = {}) {
  const res = await rpc('query', { request_type: 'call_function', finality: 'final', account_id: contract, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64') });
  return JSON.parse(Buffer.from(res.result).toString());
}
export async function head() {
  const b = await rpc('block', { finality: 'final' });
  return b.header.height;
}

const TGAS = 1_000_000_000_000n;
export const yocto = (near) => { const [a, b = ''] = String(near).split('.'); return BigInt(a) * 10n ** 24n + BigInt((b + '0'.repeat(24)).slice(0, 24)); };

// Sign and send one call. wait: 'EXECUTED_OPTIMISTIC' to get the return value, 'INCLUDED' to come back at once.
export async function call({ account, key, contract, method, args, tgas = 30, deposit = '0', wait = 'EXECUTED_OPTIMISTIC' }) {
  let lastErr;
  for (let tryNo = 0; tryNo < 4; tryNo++) {
    const ak = await rpc('query', { request_type: 'view_access_key', finality: 'optimistic', account_id: account, public_key: key.pubText });
    const blockHash = b58decode(ak.block_hash);
    const nonce = BigInt(ak.nonce) + 1n + BigInt(tryNo);
    const tx = encodeTx({ signer: account, pub: key.pub, nonce, receiver: contract, blockHash, method, args, gas: BigInt(tgas) * TGAS, deposit: yocto(deposit) });
    const { signed, hash } = signTx(tx, key.priv);
    try {
      const res = await rpc('send_tx', { signed_tx_base64: signed.toString('base64'), wait_until: wait }, 60000);
      const st = res && res.status;
      if (st && st.Failure) throw new Error('call failed: ' + JSON.stringify(st.Failure).slice(0, 300));
      const value = st && typeof st.SuccessValue === 'string' && st.SuccessValue ? JSON.parse(Buffer.from(st.SuccessValue, 'base64').toString()) : null;
      return { hash, value };
    } catch (e) {
      lastErr = e;
      if (!/InvalidNonce/.test(e.message)) throw e;
      await new Promise((ok) => setTimeout(ok, 400 + Math.random() * 600));
    }
  }
  throw lastErr;
}
