#!/usr/bin/env node
// Checks that an account has no access keys (so nobody can sign for it or move its funds).
// Usage: node tools/verify-secret-frozen.mjs <owner_account> [--rpc URL]
const args = process.argv.slice(2);
let rpc = "https://rpc.testnet.near.org";
let owner = null;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--rpc") rpc = args[++i];
  else if (!owner) owner = args[i];
}
if (!owner || !rpc) {
  console.error("usage: node tools/verify-secret-frozen.mjs <owner_account> [--rpc URL]");
  process.exit(2);
}
function cannot(reason) {
  console.log(`CANNOT VERIFY: ${reason}`);
  process.exit(2);
}
async function query(request_type) {
  let json;
  try {
    const res = await fetch(rpc, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0", id: "v", method: "query",
        params: { request_type, finality: "final", account_id: owner },
      }),
      signal: AbortSignal.timeout(20000),
    });
    if (!res.ok) cannot(`RPC HTTP ${res.status}`);
    json = await res.json();
  } catch (e) {
    cannot(`RPC request failed (${e.message})`);
  }
  if (json.error) {
    const cause = json.error.cause?.name || json.error.name || "error";
    cannot(`RPC error ${cause}${cause === "UNKNOWN_ACCOUNT" ? " (account does not exist)" : ""}`);
  }
  if (json.result?.error) {
    const e = String(json.result.error);
    cannot(/does not exist/i.test(e) ? `account ${owner} does not exist` : `RPC result error: ${e.slice(0, 120)}`);
  }
  return json.result;
}
// The key list comes back empty for accounts that do not exist, so check the account first.
await query("view_account");
const result = await query("view_access_key_list");
const keys = result?.keys;
if (!Array.isArray(keys)) cannot("unexpected RPC response");
if (keys.length === 0) {
  console.log(`MATCH: ${owner} has 0 access keys`);
  process.exit(0);
}
console.log(`MISMATCH: ${owner} has ${keys.length} keys`);
process.exit(1);
