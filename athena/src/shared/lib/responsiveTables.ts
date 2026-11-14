/**
 * Wide data tables (Personnel roster, inventory, logs…) can't fit a phone screen. This marks
 * every table with a header row of STACK_MIN_COLUMNS or more columns as `data-stack="auto"` and
 * copies each column's header text onto its cells as `data-label`. responsive.css then shows
 * those tables as one card per row on phones, with the label in front of each value.
 *
 * Opt out per table with `data-stack="off"`. Cells that already have `data-label` keep it.
 */

export const STACK_MIN_COLUMNS = 4;

export function labelTable(table: HTMLTableElement): void {
  if (table.dataset.stack === "off") return;
  const headRow = table.tHead?.rows[table.tHead.rows.length - 1];
  if (!headRow) return;
  const labels: string[] = [];
  for (const th of Array.from(headRow.cells)) {
    const text = (th.textContent ?? "").replace(/\s+/g, " ").trim();
    const span = Math.max(1, th.colSpan || 1);
    for (let i = 0; i < span; i++) labels.push(text);
  }
  if (labels.length < STACK_MIN_COLUMNS) return;
  table.dataset.stack = "auto";
  for (const body of Array.from(table.tBodies)) {
    for (const row of Array.from(body.rows)) {
      let col = 0;
      for (const cell of Array.from(row.cells)) {
        const label = labels[col] ?? "";
        if (!cell.hasAttribute("data-label") && label) cell.setAttribute("data-label", label);
        col += Math.max(1, cell.colSpan || 1);
      }
    }
  }
}

let installed = false;

/** Call once at startup; keeps labels current as tables render and re-render. */
export function installResponsiveTables(root: ParentNode = document): void {
  if (installed || typeof document === "undefined" || typeof MutationObserver === "undefined") return;
  installed = true;
  let raf = 0;
  const scan = () => {
    raf = 0;
    root.querySelectorAll("table").forEach((t) => labelTable(t as HTMLTableElement));
  };
  const schedule = () => {
    if (!raf) raf = window.requestAnimationFrame(scan);
  };
  new MutationObserver((records) => {
    // Only rescan when table rows or cells could have changed.
    for (const r of records) {
      const target = r.target as Element;
      if (target.nodeType !== 1) continue;
      if (target.closest?.("table") || Array.from(r.addedNodes).some((n) => n.nodeType === 1 && ((n as Element).tagName === "TABLE" || (n as Element).querySelector?.("table")))) {
        schedule();
        return;
      }
    }
  }).observe(document.body, { childList: true, subtree: true });
  schedule();
}
