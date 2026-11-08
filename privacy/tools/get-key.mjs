// usage: node tools/get-key.mjs <release_tx_hash> [sender]   prints the public batch key from the batch_opened event
const [hash, sender] = process.argv.slice(2);
const rpc = process.env.RPC || 'https://rpc.testnet.near.org';
const r = await fetch(rpc, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'EXPERIMENTAL_tx_status', params: { tx_hash: hash, sender_account_id: sender || 'privacy.sireadell.testnet', wait_until: 'FINAL' } }) });
const j = await r.json();
if (!j.result) { console.log('CANNOT VERIFY: ' + JSON.stringify(j.error).slice(0, 200)); process.exit(2); }
for (const o of j.result.receipts_outcome) for (const l of o.outcome.logs) {
  if (!l.startsWith('EVENT_JSON:')) continue;
  const e = JSON.parse(l.slice(11));
  if (e.event === 'batch_opened') { console.log(e.data.key); process.exit(0); }
}
console.log('CANNOT VERIFY: no batch_opened event in that transaction'); process.exit(2);
