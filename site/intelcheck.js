// Checks Intel's report on a sealed run inside the browser, from the proof saved in this repo.
// The checker is @phala/dcap-qvl-web 0.3.3 (Phala's open Rust checker built for the browser). Its two files are served
// from this site (site/vendor/dcap-qvl-web-0.3.3/) and refused unless their sha-256 equals the fingerprints below.
// Intel's certificates and revocation lists come from inside the saved bundle, so nothing is fetched from Intel.
// Four lines come out, each MATCH, MISMATCH or NEEDS (not checked). MATCH is only ever given for something that was computed here.
(function (root) {
  const CHECKER_DIR = 'vendor/dcap-qvl-web-0.3.3/';
  const CHECKER_SUMS = {
    'dcap-qvl-web.js': '3a61f9a0a8af2d07f1b69f884d747f14942dbf27670801e4f57ac272ee8aed63',
    'dcap-qvl-web_bg.wasm': '0583364c30741259992dd12055e2439e006ca234f7d13be4c77b436f242a9dc1',
  };
  const enc = (s) => new TextEncoder().encode(s);
  const hex = (u8) => Array.from(u8, (b) => b.toString(16).padStart(2, '0')).join('');
  const fromB64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
  const sha256Hex = async (bytes) => hex(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)));
  const u64le = (n) => { const b = new Uint8Array(8); new DataView(b.buffer).setBigUint64(0, BigInt(n), true); return b; };
  const NAMES = {
    sig: "Intel's signature on the sealed box's report",
    bind: 'The report is tied to this exact run and its ruling',
    prog: 'The program in the report is the one the contract stores',
    build: 'The sealed machine build is on the approved list',
  };

  // The same bytes outlayer-verify hashes (verify-core/src/preimage.rs, task_hash). format V1 adds the caller, secret and time.
  async function taskHash(att, format) {
    const parts = [enc(att.task_type), u64le(att.task_id)];
    for (const k of ['repo_url', 'commit_hash', 'build_target', 'wasm_hash', 'input_hash']) if (att[k] != null) parts.push(enc(att[k]));
    parts.push(enc(att.output_hash));
    if (att.block_height != null) parts.push(u64le(att.block_height));
    if (format === 'V1') {
      for (const k of ['caller_account_id', 'project_id', 'secrets_ref']) if (att[k] != null) parts.push(enc(att[k]));
      parts.push(u64le(att.timestamp));
      if (att.attached_usd != null) parts.push(enc(String(att.attached_usd)));
    }
    const all = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0; for (const p of parts) { all.set(p, at); at += p.length; }
    return sha256Hex(all);
  }

  // The saved output is the ruling text wrapped once more as a JSON string, exactly as the transaction carried it.
  const signedRuling = (raw) => { try { const v = JSON.parse(raw); return typeof v === 'string' ? v : raw; } catch { return raw; } };

  // deps: { qvl: { js_verify }, view(contract, method, args), contract, round (the chain's get_round answer, or null) }
  async function checkBundle(bundle, deps) {
    const rows = []; const row = (key, status, detail) => rows.push({ key, name: NAMES[key], status, detail });
    const att = bundle && bundle.attestation;
    if (!att || !att.tdx_quote || !bundle.collateral) { row('sig', 'NEEDS', 'the saved proof has no Intel report in it'); return rows; }
    let collateral;
    try { collateral = typeof bundle.collateral === 'string' ? JSON.parse(bundle.collateral) : bundle.collateral; } catch { row('sig', 'NEEDS', 'the saved certificates are unreadable'); return rows; }

    // 1. Intel's signature chain, the revocation lists and the platform status, all from the saved certificates, judged at the run's own time.
    let rep = null;
    try {
      const out = deps.qvl.js_verify(fromB64(att.tdx_quote), collateral, BigInt(att.timestamp));
      if (out.status !== 'UpToDate') row('sig', 'MISMATCH', `the signature is valid but Intel rates the machine "${out.status}", not up to date`);
      else if (bundle.collateral_info && bundle.collateral_info.covers_execution_time === false) row('sig', 'NEEDS', 'the saved certificates do not cover the moment of the run, so a later revocation could be missed');
      else { rep = out.report.TD10; row('sig', 'MATCH', `Intel's certificate chain and revocation lists check out, machine status ${out.status}, checked in this browser`); }
      if (!rep && out.report && out.report.TD10) rep = out.report.TD10;
    } catch (e) { row('sig', 'MISMATCH', `the checker rejected the report: ${String(e && e.message || e).slice(0, 140)}`); }
    if (!rep) { for (const k of ['bind', 'prog', 'build']) row(k, 'NEEDS', 'not checked, because the report itself did not pass'); return rows; }

    // 2. The signed report commits to a hash of the run's fields. Recompute it, and check the input and output texts match their hashes.
    try {
      const fmt = (bundle.verification && bundle.verification.format) || 'V1';
      if (fmt !== 'V1' && fmt !== 'V2') row('bind', 'NEEDS', `report format ${fmt} is not supported by this page`);
      else {
        const want = await taskHash(att, fmt);
        const rd = String(rep.report_data || '');
        const inHash = bundle.input_raw != null ? await sha256Hex(enc(bundle.input_raw)) : null;
        const outHash = bundle.output_raw != null ? await sha256Hex(enc(bundle.output_raw)) : null;
        if (rd.slice(0, 64) !== want || !/^0*$/.test(rd.slice(64))) row('bind', 'MISMATCH', 'the signed report does not commit to these run fields, so the saved record is not the run that was signed');
        else if (inHash == null || outHash == null) row('bind', 'NEEDS', 'the saved proof has no copy of the input and output');
        else if (inHash !== att.input_hash || outHash !== att.output_hash) row('bind', 'MISMATCH', 'the saved input or output text does not match the hash the report signs');
        else if (deps.round && deps.round.verdict != null && deps.round.verdict !== signedRuling(bundle.output_raw)) row('bind', 'MISMATCH', 'the ruling the sealed run signed is not the ruling stored in the contract');
        else if (!deps.round) row('bind', 'NEEDS', 'the contract did not answer, so the signed ruling was not compared with the stored one');
        else row('bind', 'MATCH', 'the report commits to this run, and the signed ruling is the one stored in the contract');
      }
    } catch (e) { row('bind', 'NEEDS', 'this browser could not compute the hashes'); }

    // 3. The program that ran, as named inside the signed fields, against what the contract stored.
    try {
      const got = await deps.view(deps.contract, 'courtroom', {});
      const stored = Array.isArray(got) ? got[1] : null;
      if (!stored) row('prog', 'NEEDS', 'the contract did not return its program fingerprint');
      else if (stored === att.wasm_hash) row('prog', 'MATCH', `fingerprint ${att.wasm_hash.slice(0, 8)}... in the signed report equals the one stored on ${deps.contract}`);
      else row('prog', 'MISMATCH', `the signed report names program ${String(att.wasm_hash).slice(0, 8)}..., the contract stores ${stored.slice(0, 8)}...`);
    } catch { row('prog', 'NEEDS', 'no public node answered just now'); }

    // 4. Is the machine build inside the quote one the operator publishes? Asked live, with the numbers read out of the signed quote.
    try {
      const m = { mrtd: rep.mr_td, rtmr0: rep.rt_mr0, rtmr1: rep.rt_mr1, rtmr2: rep.rt_mr2, rtmr3: rep.rt_mr3 };
      const reg = bundle.register_contract || 'worker.outlayer.testnet';
      const ok = await deps.view(reg, 'is_measurements_approved', { measurements: m });
      if (ok === true) row('build', 'MATCH', `the build numbers inside the signed report are on the approved list of ${reg}`);
      else if (ok === false) row('build', 'MISMATCH', `the build numbers inside the signed report are NOT on the approved list of ${reg}`);
      else row('build', 'NEEDS', 'the approved list gave no clear answer');
    } catch { row('build', 'NEEDS', 'no public node answered just now'); }
    return rows;
  }

  // Loads the checker from this site, refusing it unless both files have the written fingerprints.
  let loading = null;
  function loadChecker(base = '') {
    if (loading) return loading;
    loading = (async () => {
      const get = async (name) => {
        const bytes = new Uint8Array(await (await fetch(base + CHECKER_DIR + name)).arrayBuffer());
        if ((await sha256Hex(bytes)) !== CHECKER_SUMS[name]) throw new Error(`${name} does not have the fingerprint written in this repo`);
        return bytes;
      };
      const [js, wasm] = await Promise.all([get('dcap-qvl-web.js'), get('dcap-qvl-web_bg.wasm')]);
      const mod = await import(URL.createObjectURL(new Blob([js], { type: 'text/javascript' })));
      mod.initSync({ module: wasm });
      return mod;
    })();
    loading.catch(() => { loading = null; });
    return loading;
  }

  // For the case page: loads the saved proof for this round and returns the four lines. never throws.
  async function runForRound(id, sealed, { view, contract, round, base = '' }) {
    if (!sealed) return [{ key: 'none', name: "Intel's report on the sealed box", status: 'NEEDS', detail: 'not available for this round: no proof was saved for it. The command below checks it live.' }];
    let bundle;
    try { const r = await fetch(`${base}proofs/${sealed.dir}/round-${id}.bundle.json`); if (!r.ok) throw new Error('missing'); bundle = await r.json(); }
    catch { return [{ key: 'none', name: "Intel's report on the sealed box", status: 'NEEDS', detail: 'not available for this round: the saved proof file could not be loaded here.' }]; }
    let qvl;
    try { qvl = await loadChecker(base); }
    catch (e) { return [{ key: 'none', name: "Intel's report on the sealed box", status: 'NEEDS', detail: `the in-browser checker did not load (${String(e.message || e).slice(0, 100)}). Use the command below.` }]; }
    return checkBundle(bundle, { qvl, view, contract, round });
  }

  const api = { checkBundle, loadChecker, runForRound, taskHash, NAMES, CHECKER_SUMS };
  if (typeof module !== 'undefined' && module.exports) module.exports = api; else root.IntelCheck = api;
})(typeof window !== 'undefined' ? window : globalThis);
