/**
 * Frontend mirror of `src-tauri/src/document/text_match.rs`.
 *
 * Used to find every occurrence of an approved string in a page's PDF.js text
 * so each one gets a box. The Rust exporter counts occurrences with the same
 * rules and refuses to export if we covered fewer than it finds, so keep the
 * two in sync: case-insensitive, whitespace (and soft hyphen / zero-width)
 * insensitive, and a match whose first/last needle character is a letter or
 * digit must not touch another letter or digit outside the match.
 */

const IGNORABLE = /^[\s­​‌‍⁠﻿]$/u;
const ALNUM = /^[\p{Alphabetic}\p{N}]$/u;
const LIGATURES: Record<string, string> = {
  "ﬀ": "ff",
  "ﬁ": "fi",
  "ﬂ": "fl",
  "ﬃ": "ffi",
  "ﬄ": "ffl",
  "ﬅ": "st",
  "ﬆ": "st",
};

function fold(c: string, out: (f: string) => void) {
  if (IGNORABLE.test(c)) return;
  const expanded = LIGATURES[c] ?? c.toLowerCase();
  for (const f of expanded) out(f);
}

const isAlnum = (c: string | undefined) => c !== undefined && ALNUM.test(c);

export interface Needle {
  chars: string[];
  key: string;
  startsAlnum: boolean;
  endsAlnum: boolean;
}

export function makeNeedle(s: string): Needle | null {
  const chars: string[] = [];
  for (const c of s) fold(c, (f) => chars.push(f));
  if (chars.length === 0) return null;
  return {
    chars,
    key: chars.join(""),
    startsAlnum: isAlnum(chars[0]),
    endsAlnum: isAlnum(chars[chars.length - 1]),
  };
}

/** Code point that ends just before UTF-16 index `i`. */
function charBefore(s: string, i: number): string | undefined {
  if (i <= 0) return undefined;
  const lo = s.charCodeAt(i - 1);
  if (lo >= 0xdc00 && lo <= 0xdfff && i >= 2) return s.slice(i - 2, i);
  return s[i - 1];
}

/** Code point that starts at UTF-16 index `i`. */
function charAt(s: string, i: number): string | undefined {
  if (i >= s.length) return undefined;
  return String.fromCodePoint(s.codePointAt(i)!);
}

export class FoldedText {
  private chars: string[] = [];
  /** [start, end) UTF-16 range in the source of each folded char */
  private pos: [number, number][] = [];

  constructor(private src: string) {
    let i = 0;
    for (const c of src) {
      const start = i;
      i += c.length;
      fold(c, (f) => {
        this.chars.push(f);
        this.pos.push([start, i]);
      });
    }
  }

  /** Non-overlapping matches as [start, end) UTF-16 ranges into the source. */
  findAll(needle: Needle): [number, number][] {
    const out: [number, number][] = [];
    const m = needle.chars.length;
    let i = 0;
    while (i + m <= this.chars.length) {
      let ok = true;
      for (let k = 0; k < m; k++) {
        if (this.chars[i + k] !== needle.chars[k]) {
          ok = false;
          break;
        }
      }
      if (ok) {
        const start = this.pos[i][0];
        const end = this.pos[i + m - 1][1];
        const beforeOk = !needle.startsAlnum || !isAlnum(charBefore(this.src, start));
        const afterOk = !needle.endsAlnum || !isAlnum(charAt(this.src, end));
        if (beforeOk && afterOk) {
          out.push([start, end]);
          i += m;
          continue;
        }
      }
      i++;
    }
    return out;
  }
}
