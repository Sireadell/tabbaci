// Where saved sealed-box proofs live, and which outlayer-verify command checks them.
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
export const PROOF_ROOT = resolve(here, '..', '..', 'proofs');

export const proofPath = (contract, round, root = PROOF_ROOT) => join(root, contract.split('.')[0], `round-${round}.bundle.json`);

const TESTNET_API = ['--api', 'https://testnet-api.outlayer.ai'];

// A saved bundle wins: it checks with no network and no OutLayer. Otherwise use the transaction hash.
// With --save-proof and a transaction hash, the bundle is written to the saved path for next time.
export function sealedBoxCommand({ tx, bundle, network = 'testnet', save = false, savePath }) {
  if (bundle && existsSync(bundle) && !(save && tx)) return { args: ['bundle', bundle, '--offline', '--short', '--no-color'], source: `saved proof ${bundle}` };
  if (tx) {
    const args = ['tx', tx, '--network', network, '--short', '--no-color', ...(network === 'testnet' ? TESTNET_API : [])];
    if (save && savePath) args.push('--bundle', savePath);
    return { args, source: `live check of transaction ${tx}` };
  }
  return null;
}
