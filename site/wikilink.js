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


  // Reader-facing wording, one place so every page matches.
  const TEXT = {
    notEn: 'This button works on English Wikipedia only. Open an English article and try again.',
    notArticle: 'Open a Wikipedia article first, then highlight one sentence and click again.',
    sentBarTitle: 'Sent from Wikipedia',
    sentBarBody: 'The topic and sentence came from the page you were reading. Change either one before you press Send.',
    whyLine: "The court reads only two things: the article's summary at the top and its one-line Wikidata description. Nothing further down the page counts.",
    warnTitle: 'The summary may not mention this',
    warnBody: "Some key words in your sentence aren't in the article's summary, and the court reads nothing past it. So it will probably rule UNSURE, and UNSURE gives the 0.05 test NEAR bond back in full.",
    btnAnyway: 'Send it anyway',
    btnIntro: 'Use a sentence from the summary',
    introHelp: 'These sentences are true as written. To try a lie, pick one and change it.',
    emptyNote: 'Nothing was highlighted, so only the topic came across. Write your sentence below.',
    cutNote: 'Your sentence was longer than 280 characters, so it was cut. Check the end still reads right.',
    linksNote: 'Links were taken out, because the court refuses any sentence with a web link in it.',
    summaryDownNote: "We couldn't load the summary just now to check your sentence. The court still decides when you send it.",
    installTitle: 'Check Wikipedia sentences on Tabbaci',
    installLead: 'Highlight a sentence on English Wikipedia, click one bookmark, and Tabbaci opens with it filled in. You read it over and press Send.',
    installStep1: "Show your browser's bookmarks bar (Ctrl+Shift+B, or Cmd+Shift+B on a Mac).",
    installStep2: 'Drag the button below onto that bar.',
    installStep3: 'On any English Wikipedia article, highlight one sentence and click the bookmark.',
    installButtonLabel: 'Check on Tabbaci',
    installDragHint: 'Drag this onto your bookmarks bar. Clicking it here does nothing.',
    installPhoneNote: "Phones can't drag bookmarks, so this needs a computer. On a phone, use the try page instead.",
    installFine: 'No wallet needed. Test money only, up to 30 free sentences a day, and every sentence stays on the public ledger for good.',
    ledgerLabel: 'Sent from Wikipedia',
    ledgerChip: 'From Wikipedia',
    caseKind: 'Sent from Wikipedia (came in through the Wikipedia button)',
    latestMeta: 'via Wikipedia',
    homeThreeWays: 'Three ways in: the try page on this site, the tabbaci-mcp add-on for your AI, or the Wikipedia button.',
  };

  // The looping picture of the three steps. Text nodes only.
  function guide() {
    const mk = (tag, cls, txt) => { const n = document.createElement(tag); if (cls) n.className = cls; if (txt) n.textContent = txt; return n; };
    const fig = mk('figure', 'wg'); fig.style.margin = '14px 0';
    fig.setAttribute('role', 'img'); fig.setAttribute('aria-label', TEXT.installLead);
    const win = mk('div', 'wg-win');
    const addr = mk('div', 'wg-addr'); addr.appendChild(mk('span', '', 'tabbaci.vercel.app')); win.appendChild(addr);
    const bm = mk('div', 'wg-bm'); bm.appendChild(mk('span', 'wg-bmi', TEXT.installButtonLabel)); bm.appendChild(mk('span', 'wg-key', 'Ctrl+Shift+B')); win.appendChild(bm);
    const p1 = mk('div', 'wg-pg wg-p1'); p1.appendChild(mk('h4', '', TEXT.installTitle)); p1.appendChild(mk('span', 'wg-btn', TEXT.installButtonLabel)); win.appendChild(p1);
    const p2 = mk('div', 'wg-pg wg-p2'); p2.appendChild(mk('h4', '', 'Lagos'));
    const para = mk('p'); para.appendChild(mk('span', 'wg-hl', 'Lagos, or Lagos City, is a large metropolitan port city in southwestern Nigeria.')); p2.appendChild(para); win.appendChild(p2);
    const p3 = mk('div', 'wg-pg wg-p3'); const sent = mk('div', 'wg-sent'); sent.appendChild(mk('strong', '', TEXT.sentBarTitle)); p3.appendChild(sent);
    p3.appendChild(mk('div', 'wg-in', 'Lagos')); p3.appendChild(mk('div', 'wg-in', 'Lagos is a large metropolitan port city in southwestern Nigeria.')); win.appendChild(p3);
    win.appendChild(mk('span', 'wg-gh', TEXT.installButtonLabel)); win.appendChild(mk('span', 'wg-cur'));
    const fit = mk('div', 'wg-fit'); fit.appendChild(win); fig.appendChild(fit);
    const scale = () => { const w = fig.clientWidth || 520; const k = Math.min(1, w / 520); win.style.transform = 'scale(' + k + ')'; fit.style.height = Math.round(325 * k) + 'px'; };
    setTimeout(scale, 0); window.addEventListener('resize', scale);
    return fig;
  }

  // Fills an empty element with the install box. Built from text nodes only.
  function installBox(el, base) {
    if (!el || typeof document === 'undefined') return;
    const mk = (tag, txt, cls) => { const n = document.createElement(tag); if (txt) n.textContent = txt; if (cls) n.className = cls; return n; };
    el.textContent = '';
    el.appendChild(mk('h2', TEXT.installTitle, 'step'));
    el.appendChild(guide());
    const a = mk('a', TEXT.installButtonLabel, 'btn');
    a.href = bookmarkHref(base || location.origin, TEXT.notEn, TEXT.notArticle);
    a.title = TEXT.installDragHint;
    a.addEventListener('click', (e) => { e.preventDefault(); hint.hidden = false; });
    const row = mk('p'); row.appendChild(a); el.appendChild(row);
    const hint = mk('p', TEXT.installDragHint, 'mute'); hint.hidden = true; el.appendChild(hint);
  }

  const api = { TEXT, installBox,  cleanText, cleanTopicParam, cleanClaimParam, readParams, keyWords, summaryCheck, introSentences, bookmarkHref, MAX_CLAIM, MAX_TOPIC };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.WikiLink = api;
})(typeof window !== 'undefined' ? window : this);
