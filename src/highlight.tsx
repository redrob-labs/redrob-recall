import { Fragment, type ReactElement } from "react";

// Words are matched the way the keyword index matches them (`unicode61 remove_diacritics 2` in
// storage.rs): a word is a run of letters and digits, compared without case or accents, and only a
// whole word counts. So "café" marks "Cafe" but "saff" does not mark "saffron" -- marking a part
// would claim a match the index never made.
const WORD = /[\p{L}\p{N}]+/gu;

function fold(word: string): string {
  return word.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** [start, end) offsets of every word in `text` that is also a word of `query`. */
export function matchRanges(
  text: string,
  query: string,
): Array<[number, number]> {
  const wanted = new Set(Array.from(query.matchAll(WORD), (m) => fold(m[0])));
  if (wanted.size === 0) return [];
  const ranges: Array<[number, number]> = [];
  for (const m of text.matchAll(WORD)) {
    if (wanted.has(fold(m[0]))) ranges.push([m.index, m.index + m[0].length]);
  }
  return ranges;
}

export function Highlighted({ text, query }: { text: string; query: string }) {
  const ranges = matchRanges(text, query);
  if (ranges.length === 0) return <>{text}</>;
  const parts: ReactElement[] = [];
  let at = 0;
  ranges.forEach(([start, end], i) => {
    if (start > at)
      parts.push(<Fragment key={`t${i}`}>{text.slice(at, start)}</Fragment>);
    parts.push(<mark key={`m${i}`}>{text.slice(start, end)}</mark>);
    at = end;
  });
  if (at < text.length)
    parts.push(<Fragment key="tail">{text.slice(at)}</Fragment>);
  return <>{parts}</>;
}
