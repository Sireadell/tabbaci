// Re-record test/fixture.json from the live contract: node scripts/record-fixture.mjs
import { writeFileSync } from 'node:fs';
import { createClient, makeTransport } from '../src/index.mjs';

const real = makeTransport();
const store = {};
const rpc = async (method, params) => {
  const res = await real(method, params);
  if (method === 'query') store[`${params.method_name}|${Buffer.from(params.args_base64, 'base64').toString()}`] = res;
  return res;
};
const c = createClient({ rpc });
await c.getContractInfo();
await c.listRounds();
writeFileSync(new URL('../test/fixture.json', import.meta.url), JSON.stringify(store));
console.log('saved', Object.keys(store).length, 'responses');
