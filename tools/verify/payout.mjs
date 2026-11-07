// The payout rule again, in JavaScript, so the checker can recompute it without trusting the Rust build.
// Same whole-number maths: gains round down, losses round up.
const BPS = 10000n;

export function net(stake, chanceBps, outcome) {
  const target = outcome === 'T' ? BPS : 0n;
  let d = target - BigInt(chanceBps);
  if (d < 0n) d = -d;
  const score = 25_000_000n - d * d;
  const num = stake * score;
  const den = 100_000_000n;
  return num >= 0n ? num / den : -((-num + den - 1n) / den);
}

export function settle(entries, outcome, reserve) {
  const total = entries.reduce((a, e) => a + e.stake, 0n);
  if (!outcome) return { payouts: entries.map((e) => e.stake), reserveAfter: reserve };
  const nets = entries.map((e) => net(e.stake, e.chance, outcome));
  const losses = nets.filter((n) => n < 0n).reduce((a, n) => a - n, 0n);
  const gains = nets.filter((n) => n > 0n).reduce((a, n) => a + n, 0n);
  const pool = losses + reserve;
  const payouts = entries.map((e, i) => {
    const n = nets[i];
    if (n >= 0n) return e.stake + (gains <= pool ? n : (n * pool) / gains);
    return e.stake + n;
  });
  const paid = payouts.reduce((a, p) => a + p, 0n);
  return { payouts, reserveAfter: reserve + total - paid };
}

// Quick self-check the checker runs every time: money is conserved over many random settlements.
export function settleBatchCheck() {
  let seed = 12345n;
  const rnd = (n) => { seed = (seed * 6364136223846793005n + 1442695040888963407n) & ((1n << 64n) - 1n); return Number((seed >> 11n) % BigInt(n)); };
  const chances = [9500, 8000, 6500, 5000];
  for (let i = 0; i < 2000; i++) {
    const entries = Array.from({ length: 1 + rnd(8) }, () => ({ stake: 10n ** 22n * BigInt(1 + rnd(5)), chance: chances[rnd(4)] }));
    const outcome = [null, 'T', 'T', 'F'][rnd(4)];
    const reserve = BigInt(rnd(3)) * 10n ** 22n;
    const s = settle(entries, outcome, reserve);
    const stakes = entries.reduce((a, e) => a + e.stake, 0n);
    const paid = s.payouts.reduce((a, p) => a + p, 0n);
    if (paid + s.reserveAfter !== stakes + reserve) return { ok: false, detail: `money not conserved in settlement ${i}` };
    if (s.payouts.some((p, k) => p < 0n || p > entries[k].stake * 5n / 4n + 1n)) return { ok: false, detail: `payout out of range in settlement ${i}` };
  }
  return { ok: true, detail: '2000 random settlements: every unit accounted for' };
}
