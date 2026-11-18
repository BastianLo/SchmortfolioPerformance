import {
  type ColumnDef,
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  type Row,
  type SortingState,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import * as React from "react";
import { cn } from "@/lib/utils";

declare module "@tanstack/react-table" {
  interface ColumnMeta<TData, TValue> {
    align?: "left" | "right";
    className?: string;
    /** hide on small screens */
    hideMobile?: boolean;
  }
}

export function DataTable<T>({
  data,
  columns,
  onRowClick,
  initialSorting = [],
  maxHeight = "70dvh",
  rowHeight = 44,
  footer,
  renderCard,
  getRowId,
}: {
  data: T[];
  columns: ColumnDef<T, any>[];
  onRowClick?: (row: T) => void;
  initialSorting?: SortingState;
  maxHeight?: string;
  rowHeight?: number;
  footer?: React.ReactNode;
  /** compact card rendering on phones */
  renderCard?: (row: T) => React.ReactNode;
  getRowId?: (row: T, index: number) => string;
}) {
  const [sorting, setSorting] = React.useState<SortingState>(initialSorting);
  const table = useReactTable({
    data,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getRowId,
  });
  const rows = table.getRowModel().rows;
  const parentRef = React.useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });
  const items = virtualizer.getVirtualItems();
  const paddingTop = items.length ? items[0].start : 0;
  const paddingBottom = items.length ? virtualizer.getTotalSize() - items[items.length - 1].end : 0;

  const cardList = renderCard && (
    <div className="divide-y divide-border sm:hidden">
      {rows.slice(0, 300).map((r) => (
        <div
          key={r.id}
          className={cn("px-4 py-3", onRowClick && "cursor-pointer active:bg-surface-2")}
          onClick={() => onRowClick?.(r.original)}
        >
          {renderCard(r.original)}
        </div>
      ))}
      {rows.length > 300 && (
        <div className="px-4 py-3 text-center text-xs text-muted">
          {rows.length - 300} weitere Einträge – bitte filtern
        </div>
      )}
    </div>
  );

  return (
    <>
      {cardList}
      <div ref={parentRef} className={cn("overflow-auto", renderCard && "hidden sm:block")} style={{ maxHeight }}>
        <table className="w-full border-collapse text-sm">
          <thead className="sticky top-0 z-10 bg-surface">
            {table.getHeaderGroups().map((hg) => (
              <tr key={hg.id} className="border-b border-border">
                {hg.headers.map((h) => {
                  const meta = h.column.columnDef.meta;
                  const sorted = h.column.getIsSorted();
                  return (
                    <th
                      key={h.id}
                      className={cn(
                        "h-9 select-none whitespace-nowrap px-3 text-xs font-medium text-muted first:pl-4 last:pr-4 sm:first:pl-5 sm:last:pr-5",
                        meta?.align === "right" ? "text-right" : "text-left",
                        h.column.getCanSort() && "cursor-pointer hover:text-fg",
                        meta?.hideMobile && "hidden md:table-cell",
                      )}
                      onClick={h.column.getToggleSortingHandler()}
                    >
                      <span
                        className={cn("inline-flex items-center gap-1", meta?.align === "right" && "flex-row-reverse")}
                      >
                        {flexRender(h.column.columnDef.header, h.getContext())}
                        {sorted === "asc" && <ArrowUp className="size-3" />}
                        {sorted === "desc" && <ArrowDown className="size-3" />}
                      </span>
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {paddingTop > 0 && (
              <tr>
                <td style={{ height: paddingTop }} colSpan={columns.length} />
              </tr>
            )}
            {items.map((vi) => {
              const row = rows[vi.index] as Row<T>;
              return (
                <tr
                  key={row.id}
                  style={{ height: rowHeight }}
                  className={cn(
                    "border-b border-border/60 last:border-0 hover:bg-surface-2/60",
                    onRowClick && "cursor-pointer",
                  )}
                  onClick={() => onRowClick?.(row.original)}
                >
                  {row.getVisibleCells().map((cell) => {
                    const meta = cell.column.columnDef.meta;
                    return (
                      <td
                        key={cell.id}
                        className={cn(
                          "tabular whitespace-nowrap px-3 first:pl-4 last:pr-4 sm:first:pl-5 sm:last:pr-5",
                          meta?.align === "right" && "text-right",
                          meta?.hideMobile && "hidden md:table-cell",
                          meta?.className,
                        )}
                      >
                        {flexRender(cell.column.columnDef.cell, cell.getContext())}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
            {paddingBottom > 0 && (
              <tr>
                <td style={{ height: paddingBottom }} colSpan={columns.length} />
              </tr>
            )}
          </tbody>
          {footer && <tfoot className="sticky bottom-0 border-t border-border bg-surface">{footer}</tfoot>}
        </table>
      </div>
    </>
  );
}
