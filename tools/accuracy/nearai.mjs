// NEAR AI Cloud calls shared by the timing run and the accuracy run, with a spend meter.
// The meter asks NEAR AI the exact cost of every call it made (POST /v1/billing/costs, by inference id)
// and refuses to send another call once the total passes the stop line. The key is never printed.
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';

export const BASE = 'https://cloud-api.near.ai';
const LEDGER = new URL('results/spend.json', import.meta.url);
const sha = (b) => createHash('sha256').update(b).digest('hex');

export function loadKey(file) {
  if (process.env.NEARAI_API_KEY) return process.env.NEARAI_API_KEY.trim();
  if (!file) throw new Error('needs --key-file or NEARAI_API_KEY');
  return readFileSync(file, 'utf8').trim();
}

export class Spend {
  // stopUsd: the most this ledger may ever spend (start credit minus the money that must stay).
  constructor(key, stopUsd) {
    this.key = key;
    this.stop = stopUsd;
    this.l = existsSync(LEDGER) ? JSON.parse(readFileSync(LEDGER, 'utf8')) : { nano: 0, calls: 0, priced: [] };
    this.pending = [];
  }
  get usd() { return this.l.nano / 1e9; }
  async settle() {
    while (this.pending.length) {
      const ids = this.pending.splice(0, 50);
      const r = await fetch(`${BASE}/v1/billing/costs`, {
        method: 'POST', headers: { authorization: `Bearer ${this.key}`, 'content-type': 'application/json' },
        body: JSON.stringify({ requestIds: ids }),
      });
      if (!r.ok) throw new Error(`cost lookup failed: HTTP ${r.status}`);
      const j = await r.json();
      for (const x of j.requests) this.l.nano += x.costNanoUsd;
      this.l.calls += ids.length;
    }
    mkdirSync(new URL('results/', import.meta.url), { recursive: true });
    writeFileSync(LEDGER, JSON.stringify({ nano: this.l.nano, calls: this.l.calls }, null, 1));
  }
  check() {
    if (this.usd >= this.stop) throw new Error(`spend stop: $${this.usd.toFixed(4)} spent, stop line $${this.stop}`);
  }
  add(id) { if (id) this.pending.push(id); }
}

// One judge question, the way the courtroom asks it: chat call, then the signed receipt for that reply.
export async function ask(key, spend, body, { sigAlgo = 'ecdsa', sigTries = 4, timeoutMs = 60000 } = {}) {
  spend.check();
  const t0 = Date.now();
  const r = await fetch(`${BASE}/v1/chat/completions`, {
    method: 'POST',
    headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json', 'x-no-aliasing': 'true', 'accept-encoding': 'identity' },
    body, signal: AbortSignal.timeout(timeoutMs),
  });
  const resBytes = Buffer.from(await r.arrayBuffer());
  spend.add(r.headers.get('inference-id'));
  const chatMs = Date.now() - t0;
  if (!r.ok) return { ok: false, status: r.status, chatMs, totalMs: chatMs };
  const j = JSON.parse(resBytes.toString());
  const content = (j.choices?.[0]?.message?.content || '').trim();
  let sig = null;
  if (sigTries > 0) {
    for (let i = 0; i < sigTries && !sig; i++) {
      const s = await fetch(`${BASE}/v1/signature/${encodeURIComponent(j.id)}?${new URLSearchParams({ signing_algo: sigAlgo })}`, {
        headers: { authorization: `Bearer ${key}`, accept: 'application/json' }, signal: AbortSignal.timeout(10000),
      }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
      if (s && !s.error_code && s.signature) sig = s;
    }
  }
  const model = JSON.parse(body).model;
  const want = `${model}:${sha(Buffer.from(body))}:${sha(resBytes)}`;
  return {
    ok: true, content, chatMs, totalMs: Date.now() - t0,
    signed: !!sig, signedTextMatches: sig ? sig.text === want || sig.text === want.slice(model.length + 1) : false,
    sigKind: sig?.signature_kind,
  };
}
