// Asks the NEAR Intents 1Click API for a quote: 5 USDC on Base into wNEAR for sireadell.near.
// Quote only. dry:true means no deposit address is reserved and nothing is sent or swapped.
// Run: node tools/intents-quote/quote.mjs [amountUsdc] [outFile]
import { writeFileSync } from "node:fs";

const BASE = "https://1click.chaindefuser.com";
const amountUsdc = Number(process.argv[2] ?? 5);
const outFile = process.argv[3] ?? new URL("./quote-output.json", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

const ORIGIN = "nep141:base-0x833589fcd6edb6e08f4c7c32d4f71b54bda02913.omft.near"; // USDC on Base
const DEST = "nep141:wrap.near"; // wNEAR on NEAR
const RECIPIENT = "sireadell.near";
// Placeholder Base address, only used as the refund target of a dry quote. No funds ever go there.
const REFUND_TO = "0x0000000000000000000000000000000000000001";

const get = async (path) => {
  const r = await fetch(BASE + path);
  if (!r.ok) throw new Error(`${path} -> HTTP ${r.status} ${await r.text()}`);
  return r.json();
};

const tokens = await get("/v0/tokens");
const o = tokens.find((t) => t.assetId === ORIGIN);
const d = tokens.find((t) => t.assetId === DEST);
if (!o || !d) throw new Error("asset not listed by 1Click any more");

const amount = BigInt(Math.round(amountUsdc * 10 ** o.decimals)).toString();
const body = {
  dry: true,
  swapType: "EXACT_INPUT",
  slippageTolerance: 100,
  originAsset: ORIGIN,
  depositType: "ORIGIN_CHAIN",
  destinationAsset: DEST,
  amount,
  refundTo: REFUND_TO,
  refundType: "ORIGIN_CHAIN",
  recipient: RECIPIENT,
  recipientType: "DESTINATION_CHAIN",
  deadline: new Date(Date.now() + 30 * 60 * 1000).toISOString(),
};

const res = await fetch(BASE + "/v0/quote", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});
const text = await res.text();
let json;
try { json = JSON.parse(text); } catch { json = { raw: text }; }
if (!res.ok) {
  console.error(`quote failed: HTTP ${res.status}`, text);
  process.exit(1);
}

const out = {
  note: "Quote only (dry:true). No deposit was made and no swap happened.",
  fetchedAt: new Date().toISOString(),
  request: body,
  response: json,
};
writeFileSync(outFile, JSON.stringify(out, null, 2) + "\n");

const q = json.quote ?? {};
const outHuman = q.amountOut ? Number(BigInt(q.amountOut)) / 10 ** d.decimals : null;
console.log(`Sell ${amountUsdc} USDC on Base, receive about ${outHuman} wNEAR to ${RECIPIENT}`);
console.log(`In (USD): ${q.amountInUsd}  Out (USD): ${q.amountOutUsd}  Min out: ${q.minAmountOut}`);
console.log(`Saved ${outFile}`);
