import { useNavigate } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";
import * as React from "react";
import type { Schemas } from "@/api/client";
import { useHoldings } from "@/api/hooks";
import { Card, CardHeader } from "@/components/ui/card";
import { DataTable } from "@/components/ui/data-table";
import { Input } from "@/components/ui/input";
import { ErrorBox, PageHeader, Skeleton, Stat } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { money, percent, quote, shares, tone } from "@/lib/format";
import { cn } from "@/lib/utils";

type Row = Schemas["HoldingRow"];

export function HoldingsPage() {
  const clientId = useClientId();
  const [date, setDate] = React.useState<string>("");
  const { data, error, isLoading } = useHoldings(clientId, date || undefined);
  const navigate = useNavigate();
  const cur = data?.currency ?? "EUR";

  const columns = React.useMemo<ColumnDef<Row>[]>(
    () => [
      {
        accessorKey: "name",
        header: "Wertpapier",
        cell: ({ row }) => (
          <div className="max-w-72 min-w-40">
            <div className="truncate font-medium">{row.original.name}</div>
            <div className="truncate text-xs text-muted">{row.original.isin ?? row.original.ticker}</div>
          </div>
        ),
      },
      {
        accessorKey: "shares",
        header: "Stück",
        cell: (c) => shares(c.getValue<number>()),
        meta: { align: "right", hideMobile: true },
      },
      {
        accessorKey: "quote",
        header: "Kurs",
        meta: { align: "right" },
        cell: ({ row }) => (
          <div>
            <div>{quote(row.original.quote, row.original.currency)}</div>
            <div className={cn("text-xs", tone(row.original.quote_change))}>
              {percent(row.original.quote_change, { sign: true })}
            </div>
          </div>
        ),
      },
      {
        accessorKey: "market_value",
        header: "Marktwert",
        cell: (c) => <span className="font-medium">{money(c.getValue<number>(), cur)}</span>,
        meta: { align: "right" },
      },
      {
        accessorKey: "share",
        header: "Anteil",
        cell: (c) => percent(c.getValue<number>(), { digits: 1 }),
        meta: { align: "right", hideMobile: true },
      },
      {
        accessorKey: "fifo_cost",
        header: "Einstand",
        cell: (c) => money(c.getValue<number>(), cur),
        meta: { align: "right", hideMobile: true },
      },
      {
        accessorKey: "gain",
        header: "Gewinn/Verlust",
        meta: { align: "right" },
        cell: ({ row }) => (
          <div className={tone(row.original.gain)}>
            <div>{money(row.original.gain, cur, { sign: true })}</div>
            <div className="text-xs">{percent(row.original.gain_percent, { sign: true })}</div>
          </div>
        ),
      },
      {
        accessorKey: "irr",
        header: "IZF p.a.",
        cell: (c) => (
          <span className={tone(c.getValue<number>())}>{percent(c.getValue<number>(), { sign: true })}</span>
        ),
        meta: { align: "right", hideMobile: true },
      },
      {
        accessorKey: "dividends",
        header: "Erträge",
        cell: (c) => money(c.getValue<number>(), cur),
        meta: { align: "right", hideMobile: true },
      },
    ],
    [cur],
  );

  const totalCost = data?.positions.reduce((s, p) => s + p.fifo_cost, 0) ?? 0;
  const totalGain = data?.positions.reduce((s, p) => s + p.gain, 0) ?? 0;

  return (
    <>
      <PageHeader
        title="Vermögensaufstellung"
        subtitle="Bestand, Einstand nach FIFO und Gewinne je Wertpapier"
        actions={
          <Input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="w-40"
            aria-label="Stichtag"
          />
        }
      />
      {error ? (
        <ErrorBox error={error} />
      ) : isLoading || !data ? (
        <Skeleton className="h-96" />
      ) : (
        <div className="space-y-4">
          <Card>
            <div className="grid grid-cols-2 gap-5 p-4 sm:grid-cols-4 sm:p-5">
              <Stat label="Gesamtvermögen" value={money(data.total, cur)} />
              <Stat label="Wertpapiere" value={money(data.securities_value, cur)} />
              <Stat label="Barvermögen" value={money(data.cash_value, cur)} />
              <Stat
                label="Nicht realisierte G/V"
                value={money(totalGain, cur, { sign: true })}
                valueClass={tone(totalGain)}
                sub={
                  <span className={tone(totalGain)}>
                    {percent(totalCost ? totalGain / totalCost : 0, { sign: true })}
                  </span>
                }
              />
            </div>
          </Card>
          <Card>
            <CardHeader title={`Wertpapiere (${data.positions.length})`} />
            <div className="mt-2">
              <DataTable
                data={data.positions}
                columns={columns}
                initialSorting={[{ id: "market_value", desc: true }]}
                getRowId={(r) => r.security_id}
                onRowClick={(r) =>
                  navigate({
                    to: "/c/$clientId/securities/$securityId",
                    params: { clientId, securityId: r.security_id },
                  })
                }
                rowHeight={56}
                renderCard={(p) => (
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">{p.name}</div>
                      <div className="text-xs text-muted">
                        {shares(p.shares)} Stk. · {quote(p.quote, p.currency)}
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="tabular text-sm font-medium">{money(p.market_value, cur)}</div>
                      <div className={cn("tabular text-xs", tone(p.gain))}>
                        {percent(p.gain_percent, { sign: true })}
                      </div>
                    </div>
                  </div>
                )}
              />
            </div>
          </Card>
          <Card>
            <CardHeader title="Barvermögen" />
            <ul className="mt-2 divide-y divide-border">
              {data.cash.map((c) => (
                <li key={c.account_id} className="flex items-center justify-between px-4 py-3 text-sm sm:px-5">
                  <span>{c.name}</span>
                  <span className="tabular font-medium">
                    {money(c.balance, c.currency)}
                    {c.currency !== cur && (
                      <span className="ml-2 text-xs text-muted">({money(c.balance_converted, cur)})</span>
                    )}
                  </span>
                </li>
              ))}
              {data.cash.length === 0 && <li className="px-5 py-4 text-sm text-muted">Keine Guthaben.</li>}
            </ul>
          </Card>
        </div>
      )}
    </>
  );
}
