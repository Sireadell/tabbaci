// Reads and cleans what the "Check on Tabbaci" Wikipedia bookmark puts in the address.
// Everything here is plain text handling: the values come from a web address, so they are never trusted as markup.
(function (root) {
  const STOP = new Set('about above after again against also among another around because been before being below between both came come could does doing down during each every from further have having here into itself just like made make many more most much must never only other over same should since some such than that their them then there these they this those through under until upon very what when where which while whom whose will with within without would your judges ignore answer pages true false'.split(' '));
  const MAX_TOPIC = 120;
  const MAX_CLAIM = 280;
  const MAX_CLAIM_IN = 400;

  // Control characters, zero-width marks and the characters that flip text direction.
  const HIDDEN = /[\u0000-\u0008\u000B-\u001F\u007F-\u009F​-‏‪-‮⁠-⁤⁦-⁩﻿]/g;
  // Footnote marks Wikipedia leaves in a highlighted sentence: [12], [a], [citation needed], [note 3], [nb 1].
  const FOOTNOTE = /\[(?:\d{1,3}|[a-z]|(?:citation needed|clarification needed|when\?|who\?|note \d{1,3}|nb \d{1,3}))\]/gi;
  const LINK = /\bhttps?:\/\/\S*/gi;

  function cleanText(s) {
    return String(s == null ? '' : s).replace(HIDDEN, ' ').replace(FOOTNOTE, '').replace(/\s+/g, ' ').trim();
  }

  function cleanTopicParam(s) {
    let t = cleanText(s).replace(/_/g, ' ');
    const h = t.indexOf('#');
    if (h >= 0) t = t.slice(0, h);
    return t.replace(/\s+/g, ' ').trim().slice(0, MAX_TOPIC).trim();
  }

  // Returns {text, notes}. Notes say what was changed so the page can tell the person.
  function cleanClaimParam(s) {
    const notes = [];
    let t = cleanText(String(s == null ? '' : s).slice(0, MAX_CLAIM_IN * 4));
    if (LINK.test(t)) { t = t.replace(LINK, '').replace(/\s+/g, ' ').trim(); notes.push('links-removed'); }
    LINK.lastIndex = 0;
    if (t.length > MAX_CLAIM) {
      const cut = t.slice(0, MAX_CLAIM + 1);
      const sp = cut.lastIndexOf(' ');
      t = (sp > MAX_CLAIM * 0.6 ? cut.slice(0, sp) : cut.slice(0, MAX_CLAIM)).trim();
      notes.push('cut');
    }
    return { text: t, notes };
  }

  function readParams(search) {
    let p;
    try { p = new URLSearchParams(search || ''); } catch (e) { p = new URLSearchParams(''); }
    const get = (k) => { try { return p.get(k) || ''; } catch (e) { return ''; } };
    const fromWiki = get('from') === 'wiki';
    const topic = cleanTopicParam(get('topic'));
    const c = cleanClaimParam(get('claim'));
    const notes = c.notes.slice();
    if (fromWiki && !c.text) notes.push('empty');
    return { fromWiki, topic, claim: c.text, notes, present: p.has('topic') || p.has('claim') || p.has('from') };
  }

  function keyWords(claim, title) {
    const titleWords = new Set(String(title || '').toLowerCase().split(/[^a-z0-9]+/).filter(Boolean));
    const words = String(claim || '').toLowerCase().match(/[a-z][a-z0-9-]{3,}|\d[\d,.]*\d|\d/g) || [];
    return [...new Set(words)].filter((w) => !STOP.has(w) && !titleWords.has(w));
  }

  // Are the sentence's key words in the page summary? ok means the court will probably see what decides it.
  function summaryCheck(claim, title, extract) {
    const words = keyWords(claim, title);
    const low = String(extract || '').toLowerCase();
    const found = words.filter((w) => low.includes(w));
    const missing = words.filter((w) => !low.includes(w));
    if (!words.length) return { found, missing, ok: true };
    const need = Math.min(2, words.length);
    return { found, missing, ok: found.length >= need && found.length * 2 >= words.length };
  }

  function introSentences(extract, n) {
    const t = String(extract || '').replace(/\s+/g, ' ').trim();
    const parts = t.match(/[^.!?]+(?:[.!?]+(?=\s+[A-Z"(]|$)|$)/g) || [];
    return parts.map((x) => x.trim()).filter((x) => x.length >= 8 && x.length <= MAX_CLAIM).slice(0, n || 3);
  }

  // The one-line bookmark. notEn and notArticle are the short notes shown by alert().
  function bookmarkHref(base, notEn, notArticle) {
    const code = '(()=>{const B=' + JSON.stringify(String(base).replace(/\/+$/, '') + '/try.html') + ';' +
      'if(!/^en\\.(m\\.)?wikipedia\\.org$/.test(location.hostname)){alert(' + JSON.stringify(notEn) + ');return;}' +
      'let t="";try{const m=location.pathname.match(/^\\/wiki\\/(.+)$/);t=m?decodeURIComponent(m[1]):(new URLSearchParams(location.search).get("title")||"");}catch(e){}' +
      't=t.replace(/_/g," ").trim();' +
      'if(!t||t==="Main Page"||/^(Special|Talk|User|Wikipedia|File|Template|Help|Category|Portal|Draft|MediaWiki|Module|TimedText|Book)( talk)?:/i.test(t)){alert(' + JSON.stringify(notArticle) + ');return;}' +
      'const s=String(getSelection()).replace(/\\s+/g," ").trim().slice(0,' + MAX_CLAIM_IN + ');' +
      'const u=B+"?from=wiki&topic="+encodeURIComponent(t.slice(0,' + MAX_TOPIC + '))+"&claim="+encodeURIComponent(s);' +
      'const w=open(u,"_blank");if(w)w.opener=null;else location.href=u;})()';
    return 'javascript:' + code.replace(/[\r\n]/g, '');
  }

  const api = { cleanText, cleanTopicParam, cleanClaimParam, readParams, keyWords, summaryCheck, introSentences, bookmarkHref, MAX_CLAIM, MAX_TOPIC };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.WikiLink = api;
})(typeof window !== 'undefined' ? window : this);
