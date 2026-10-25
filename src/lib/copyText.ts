/**
 * Copying text that a program wrapped itself.
 *
 * xterm knows about its own soft wraps (`isWrapped`) and copies those rows as
 * one line. TUIs like Claude Code instead wrap text themselves — a hard line
 * break, the continuation usually indented — so a long command copies as
 * several lines. A break is taken as such a wrap when the next row's first
 * word could not have fitted on the row above it: nobody breaks a line by hand
 * right before a word that still had room.
 */

export type CopyRow = {
  // The whole row as displayed (trailing spaces trimmed).
  full: string;
  // The selected part of the row.
  selected: string;
  // xterm's own soft wrap: the row continues the previous one.
  wrapped: boolean;
};

// Columns a wrapping program commonly keeps free at the right edge.
const RIGHT_MARGIN = 2;

export function wrappedByApp(prevFull: string, nextFull: string, cols: number): boolean {
  const prev = prevFull.trimEnd();
  const next = nextFull.trimStart();
  if (!prev || !next) return false;
  // A shell line continuation is a break someone typed on purpose.
  if (/\s\\$/.test(prev)) return false;
  const indent = nextFull.length - next.length;
  const firstWord = next.split(/\s/, 1)[0] ?? "";
  return prev.length + 1 + firstWord.length > cols - RIGHT_MARGIN - indent;
}

/**
 * The width the text was wrapped at. Rows keep the width they were printed
 * with — widening the window doesn't reflow a program's hard line breaks — so
 * the current column count overstates it after a resize. Estimate it from the
 * longest row nearby, ignoring box-drawing rules (Claude Code redraws its
 * input box at the current width).
 */
export function wrapWidth(nearbyRows: string[], cols: number): number {
  let longest = 0;
  for (const row of nearbyRows) {
    const t = row.trimEnd();
    if (/^\s*[─━═╭╰│┃]/.test(t)) continue;
    longest = Math.max(longest, t.length);
  }
  return longest > 0 ? Math.min(cols, longest + RIGHT_MARGIN) : cols;
}

/**
 * Whether a wrap cut through a single token (a long path, URL, ARN or JSON
 * blob) rather than at a space: the token's two halves together are longer
 * than a row, so it could never have been moved down whole — the program had
 * to split it, and the halves join with no space.
 */
export function splitMidToken(prevFull: string, nextFull: string, cols: number): boolean {
  const lastToken = prevFull.trimEnd().split(/\s/).pop() ?? "";
  const firstToken = nextFull.trimStart().split(/\s/, 1)[0] ?? "";
  return lastToken.length + firstToken.length > cols - RIGHT_MARGIN;
}

/**
 * Joins selected rows into the text to copy: soft-wrapped rows and rows a
 * program wrapped become one line (the continuation's indent dropped, a space
 * put back where the program broke at one); real line breaks stay.
 */
export function joinRows(rows: CopyRow[], cols: number): string {
  if (rows.length === 0) return "";
  let out = rows[0].selected;
  for (let i = 1; i < rows.length; i++) {
    const prev = rows[i - 1];
    const row = rows[i];
    if (row.wrapped) {
      out += row.selected;
    } else if (wrappedByApp(prev.full, row.full, cols)) {
      out = out.trimEnd() + (splitMidToken(prev.full, row.full, cols) ? "" : " ") + row.selected.trimStart();
    } else {
      out += "\n" + row.selected;
    }
  }
  return out;
}
