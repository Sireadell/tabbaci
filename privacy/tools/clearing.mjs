// One-price clearing, the same rules as contract/src/clearing.rs, in exact BigInt maths.
// Used by the checker to re-clear a batch from chain data and compare with what the contract stored.
const side = (s, p) => (s === 0 ? BigInt(p) : 10000n - BigInt(p));
const eligible = (r, p) => BigInt(r.limit) >= side(r.side, p);
const demand = (r, p) => (r.amount * 10000n) / side(r.side, p);

function asked(orders, p, s) {
  let t = 0n;
  for (const r of orders) if (r.side === s && eligible(r, p)) t += demand(r, p);
  return t;
}
const matchedAt = (o, p) => { const y = asked(o, p, 0), n = asked(o, p, 1); return y < n ? y : n; };

function firstTrue(f) {
  let lo = 1, hi = 10000;
  while (lo < hi) { const mid = (lo + hi) >> 1; if (f(mid)) hi = mid; else lo = mid + 1; }
  return lo;
}

function findPrice(o) {
  const p0 = firstTrue((p) => asked(o, p, 1) >= asked(o, p, 0));
  let peak = 0n;
  if (p0 <= 9999) peak = matchedAt(o, p0);
  if (p0 >= 2) { const m = matchedAt(o, p0 - 1); if (m > peak) peak = m; }
  if (peak === 0n) return [0, 0n];
  const a = firstTrue((p) => asked(o, p, 0) < peak) - 1;
  const b = firstTrue((p) => asked(o, p, 1) >= peak);
  return [Math.min(Math.max(5000, b), a), peak];
}

// orders: [{side, limit, amount: BigInt}]; returns {price_bp, matched, fills:[{shares, cost}]}
export function clear(orders) {
  const fills = orders.map(() => ({ shares: 0n, cost: 0n }));
  const [p, matched] = findPrice(orders);
  if (matched === 0n) return { price_bp: 0, matched: 0n, fills };
  const ask = [0n, 0n];
  const dem = orders.map((r) => (eligible(r, p) ? demand(r, p) : 0n));
  orders.forEach((r, i) => { ask[r.side] += dem[i]; });
  for (const s of [0, 1]) {
    const total = ask[s];
    if (total === 0n) continue;
    const target = matched < total ? matched : total;
    let given = 0n;
    const rem = [];
    orders.forEach((r, i) => {
      if (r.side !== s || dem[i] === 0n) return;
      const num = dem[i] * target;
      fills[i].shares = num / total;
      given += fills[i].shares;
      rem.push([num % total, i]);
    });
    rem.sort((x, y) => (x[0] === y[0] ? x[1] - y[1] : x[0] > y[0] ? -1 : 1));
    let left = target - given;
    for (const [, i] of rem) {
      if (left === 0n) break;
      if (fills[i].shares < dem[i]) { fills[i].shares += 1n; left -= 1n; }
    }
  }
  orders.forEach((r, i) => { fills[i].cost = (fills[i].shares * side(r.side, p) + 9999n) / 10000n; });
  return { price_bp: p, matched, fills };
}
