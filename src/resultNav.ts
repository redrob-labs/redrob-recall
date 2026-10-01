// Which result the arrow keys land on. Stops at either end rather than wrapping: a list that jumps
// from the last result back to the first looks, mid-scroll, like the results changed.
export function stepIndex(
  count: number,
  current: number,
  key: string,
): number | null {
  if (count === 0) return null;
  if (key === "ArrowDown")
    return current < 0 ? 0 : Math.min(current + 1, count - 1);
  if (key === "ArrowUp") return current < 0 ? 0 : Math.max(current - 1, 0);
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}
