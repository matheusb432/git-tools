export type RowPageRequest = { readonly start: number; readonly count: number };

export type RowPagePlanInput = {
  readonly expanded: boolean;
  readonly intersects: boolean;
  readonly pageSize: number;
  readonly rowWindow: { readonly start: number; readonly end: number } | null;
  readonly totalRows: number | null;
};

export function planRowPages(input: RowPagePlanInput): readonly RowPageRequest[] {
  if (!input.expanded || !input.intersects) return [];

  const pageSize = Math.max(1, Math.trunc(input.pageSize));
  const requests: RowPageRequest[] = [{ start: 0, count: pageSize }];
  if (input.rowWindow === null || input.totalRows === null || input.totalRows <= 0) return requests;

  const visibleStart = Math.max(0, Math.trunc(input.rowWindow.start));
  const visibleEnd = Math.min(
    Math.max(0, Math.trunc(input.totalRows) - 1),
    Math.max(visibleStart, input.rowWindow.end),
  );
  const firstVisiblePage = Math.floor(visibleStart / pageSize) * pageSize;
  const lastVisiblePage = Math.floor(visibleEnd / pageSize) * pageSize;
  const nonzeroVisiblePage = firstVisiblePage === 0 ? Math.min(pageSize, lastVisiblePage) : firstVisiblePage;
  if (nonzeroVisiblePage > 0) requests.push({ start: nonzeroVisiblePage, count: pageSize });
  return requests;
}
