import { test } from 'node:test';
import assert from 'node:assert';
import { requestBody, SYSTEM } from './frames.mjs';

test('judge question bytes match the courtroom program exactly', () => {
  // programs/court/src/frames.rs exact_bytes_match_the_checker pins the same string.
  assert.equal(requestBody('m', 'u', 7), `{"model":"m","messages":[{"role":"system","content":"${SYSTEM}"},{"role":"user","content":"u"}],"temperature":0,"seed":7,"max_tokens":4,"stream":false,"reasoning_effort":"none"}`);
});
