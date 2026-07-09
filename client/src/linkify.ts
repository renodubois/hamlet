/// Tokenize free-form text into interleaved plain runs and http(s) URLs so
/// messages can be rendered with clickable `<a>` tags.
///
/// We deliberately match the server's URL-extraction scope
/// (`server/src/embeds.rs:extract_urls`): http/https only, trailing prose
/// punctuation stripped from the URL. The trailing-punctuation set mirrors
/// `trim_url` in that file. Keeping the two in sync upholds the invariant
/// "every URL that produces an embed is also clickable."
///
/// `new URL()` validates each candidate before we emit a link token — guards
/// against `javascript:` or other odd schemes slipping through if the regex
/// ever drifts.

export type LinkifyToken = { type: "text"; value: string } | { type: "link"; url: string };

const URL_RE = /https?:\/\/[^\s<>"']+/gi;

// Casees the server's `trim_url` trailing set.
const TRAILING = `.,;:!?)]}'"\`>`;

function stripTrailing(candidate: string): { url: string; tail: string } {
  let i = candidate.length;
  while (i > 0 && TRAILING.includes(candidate[i - 1])) {
    i -= 1;
  }
  return { url: candidate.slice(0, i), tail: candidate.slice(i) };
}

function isHttpUrl(candidate: string): boolean {
  try {
    const u = new URL(candidate);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

export function linkifyText(text: string): LinkifyToken[] {
  const tokens: LinkifyToken[] = [];
  let cursor = 0;

  // Regex `exec` walks the string; we slice the gaps between matches as text
  // tokens and push valid URLs as link tokens. Trailing punctuation is
  // stripped off the URL and rewound into the regex's lastIndex so it flows
  // naturally into the next text run.
  URL_RE.lastIndex = 0;
  let m: RegExpExecArray | null = URL_RE.exec(text);
  while (m !== null) {
    const start = m.index;
    const raw = m[0];
    const { url } = stripTrailing(raw);

    if (url.length > 0 && isHttpUrl(url)) {
      if (start > cursor) {
        tokens.push({ type: "text", value: text.slice(cursor, start) });
      }
      tokens.push({ type: "link", url });
      cursor = start + url.length;
      // Rewind so trailing punctuation (still inside the raw match) is seen
      // by the next iteration — or falls into the final text flush.
      URL_RE.lastIndex = cursor;
    }
    // If the match wasn't a valid URL we leave cursor alone; the whole run
    // will be emitted as text by the final flush.
    m = URL_RE.exec(text);
  }

  if (cursor < text.length) {
    tokens.push({ type: "text", value: text.slice(cursor) });
  }
  return tokens;
}
