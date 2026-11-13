// Pure rules for the write function, kept apart so they can be tested without the network.

export const WORDS = ['is', 'very likely', 'likely', 'disputed'];
export const CAP_PER_DAY = 30;
export const PER_VISITOR_PER_HOUR = 3;
export const DAY_BLOCKS = 150_000; // testnet makes a block about every 0.59 s, so this is a little over 24 h
export const WIKI = 'https://en.wikipedia.org/api/rest_v1/page/summary/';
export const WIKIDATA = 'https://www.wikidata.org/w/rest.php/wikibase/v1/entities/items/';
export const SOURCE_WIKI = 4;
export const SOURCE_WIKIDATA = 9;

// Short list of words that would put abuse on a public ledger forever. Not a full filter, a floor.
const BLOCK = ['nigger', 'nigga', 'faggot', 'retard', 'kike', 'spic', 'chink', 'tranny', 'cunt', 'rape', 'kill yourself', 'kys'];

export function cleanClaim(raw) {
  const claim = String(raw ?? '').replace(/\s+/g, ' ').trim();
  if (claim.length < 8) return { error: 'Write a sentence of at least 8 characters.' };
  if (claim.length > 280) return { error: 'Keep the sentence under 280 characters.' };
  const low = claim.toLowerCase();
  if (BLOCK.some((w) => new RegExp(`(^|[^a-z])${w}([^a-z]|$)`).test(low))) return { error: 'That sentence has words this site does not publish.' };
  if (/https?:\/\//i.test(claim)) return { error: 'Leave links out of the sentence. Name the topic in the topic box instead.' };
  return { claim };
}

export function cleanTopic(raw) {
  const t = String(raw ?? '').replace(/\s+/g, ' ').trim();
  if (!t || t.length > 120) return { error: 'Name a Wikipedia topic, for example Moon.' };
  return { topic: t };
}

export function cleanWord(raw) {
  const w = String(raw ?? '').trim().toLowerCase();
  return WORDS.includes(w) ? { word: w } : { error: 'Pick one word: is, very likely, likely or disputed.' };
}

const STOP = new Set('about above after again against also among another around because been before being below between both came come could does doing down during each every from further have having here into itself just like made make many more most much must never only other over same should since some such than that their them then there these they this those through under until upon very what when where which while whom whose will with within without would your judges ignore answer pages true false'.split(' '));

// Where the summary text sits inside the raw page, so a key word lands on the article text and not on its links.
export function extractSpan(body) {
  const k = body.indexOf('"extract":"');
  if (k < 0) return null;
  const start = k + 11;
  let end = body.indexOf('","extract_html"', start);
  if (end < 0) end = body.indexOf('"', start + 1);
  return [start, end];
}

// The court shows the judges about 300 characters around the first key word it finds on each page.
// Pick words from the sentence whose first appearance in the raw page is inside the article text.
export function pickTerms(claim, title, body) {
  const span = extractSpan(body);
  if (!span) return [];
  const low = body.toLowerCase();
  const [s, e] = span;
  const extract = low.slice(s, e);
  const titleWords = new Set(title.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean));
  const words = [...new Set(claim.toLowerCase().match(/[a-z][a-z0-9-]{3,}/g) || [])].filter((w) => !STOP.has(w));
  const scored = [];
  for (const w of words) {
    const first = low.indexOf(w);
    if (first < s || first >= e) continue;
    const count = extract.split(w).length - 1;
    scored.push({ w, title: titleWords.has(w), count, len: w.length });
  }
  scored.sort((a, b) => (a.title - b.title) || (a.count - b.count) || (b.len - a.len));
  const terms = scored.slice(0, 3).map((x) => x.w);
  // Fallback: a phrase about 140 characters into the article. The court's window of about 300 characters
  // around it then covers the opening of the article instead of the links before it.
  const fb = openingAnchor(body, s, e);
  if (fb) terms.push(fb);
  return terms;
}

export function openingAnchor(body, s, e) {
  const low = body.toLowerCase();
  for (let off = Math.min(140, e - s - 1); off >= 0; off -= 10) {
    let p = s + off;
    while (p > s && body[p - 1] !== ' ') p--;
    const phrase = body.slice(p, Math.min(e, p + 40)).split(' ').slice(0, 3).join(' ');
    if (phrase.length >= 6 && !/["\\]/.test(phrase) && low.indexOf(phrase.toLowerCase()) === p) return phrase;
  }
  return null;
}

export function wikiTitleForUrl(canonical) {
  return encodeURIComponent(canonical).replace(/%2F/g, '/');
}

export function buildRound({ claim, canonical, qid, terms }) {
  return {
    claim,
    kind: 'prose',
    cv: null,
    picks: [
      { id: SOURCE_WIKI, url: WIKI + wikiTitleForUrl(canonical), x: '', tol: 0 },
      { id: SOURCE_WIKIDATA, url: `${WIKIDATA}${qid}/descriptions/en`, x: '', tol: 0 },
    ],
    terms,
    tag: 'visitor',
  };
}

// Rounds this account opened since `since` (a block height), from a list of rounds newest first.
export function countSince(rounds, account, since) {
  return rounds.filter((r) => r && r.opener === account && r.opened_block >= since).length;
}

// A per-visitor limit kept in memory. It resets when the function restarts, the daily cap on the chain does not.
export function makeLimiter(max, windowMs) {
  const hits = new Map();
  return (who, now = Date.now()) => {
    const list = (hits.get(who) || []).filter((t) => now - t < windowMs);
    if (list.length >= max) { hits.set(who, list); return false; }
    list.push(now); hits.set(who, list);
    if (hits.size > 5000) hits.clear();
    return true;
  };
}
