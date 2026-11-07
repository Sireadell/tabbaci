import test from 'node:test';
import assert from 'node:assert/strict';
import { keccak256, recoverAddress } from './crypto.mjs';

test('keccak256 known answers', () => {
  assert.equal(keccak256(Buffer.alloc(0)).toString('hex'), 'c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470');
  assert.equal(keccak256(Buffer.from('abc')).toString('hex'), '4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45');
});

test('recovers the address the Rust stand-in signer produced', () => {
  const sig = Buffer.from('2ac48f1a092483c278fa3cefb840d3b8e18d621a048e2bb466586495ef07d22d60ff1b651df2294f9d29a45f666b72888ec262444758133e92bf962a1d2e0a521b', 'hex');
  assert.equal(recoverAddress('hello', sig), '0x17c5185167401ed00cf5f5b2fc97d9bbfdb7d025');
  assert.notEqual(recoverAddress('hellO', sig), '0x17c5185167401ed00cf5f5b2fc97d9bbfdb7d025');
});

test('rejects bad lengths and bad recovery ids', () => {
  assert.equal(recoverAddress('x', Buffer.alloc(64)), null);
  const bad = Buffer.alloc(65, 1);
  bad[64] = 9;
  assert.equal(recoverAddress('x', bad), null);
});
