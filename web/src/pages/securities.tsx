import { useNavigate } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { AlertTriangle, Plus, RefreshCw, Search } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useInvalidateClient, useSecurities } from "@/api/hooks";
import { SecurityDialog } from "@/components/security-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DataTable } from "@/components/ui/data-table";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Badge, ErrorBox, PageHeader, Segmented, Skeleton, Spinner } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, percent, quote, shares, tone } from "@/lib/format";

type Row = Schemas["SecurityDto"];

export function SecuritiesPage() {
  const clientId = useClientId();
  const { data, error, isLoading } = useSecurities(clientId);
  const navigate = useNavigate();
  const [q, setQ] = React.useState("");
  const [filter, setFilter] = React.useState<"held" | "all" | "retired">("held");
  const [open, setOpen] = React.useState(false);
  const [updating, setUpdating] = React.useState(false);
  const [results, setResults] = React.useState<Schemas["QuoteUpdateResult"][] | null>(null);
  const invalidate = useInvalidateClient(clientId);
  const updateAll = async () => {
    setUpdating(true);
    try {
      setResults(
        await unwrap(api.POST("/clients/{id}/quotes/update", { params: { path: { id: clientId } }, body: {} })),
      );
      invalidate();
    } finally {
      setUpdating(false);
    }
  };

  const rows = React.useMemo(() => {
    const ql = q.toLowerCase();
    return (data ?? []).filter((s) => {
      if (filter === "held" && s.shares === 0) return false;
      if (filter === "retired" && !s.retired) return false;
      if (filter === "all" && s.retired) return false;
      return !ql || [s.name, s.isin, s.wkn, s.ticker].some((x) => x?.toLowerCase().includes(ql));
    });
  }, [data, q, filter]);

  const columns = React.useMemo<ColumnDef<Row>[]>(
    () => [
      {
        accessorKey: "name",
        header: "Name",
        cell: ({ row }) => (
          <div className="max-w-80 min-w-40">
            <div className="truncate font-medium">
              {row.original.name} {row.original.retired && <Badge>inaktiv</Badge>}
            </div>
            <div className="truncate text-xs text-muted">
              {[row.original.isin, row.original.wkn, row.original.ticker].filter(Boolean).join(" · ")}
            </div>
          </div>
        ),
      },
      { accessorKey: "currency", header: "Währung", meta: { hideMobile: true } },
      {
        accessorKey: "latest_price",
        header: "Letzter Kurs",
        meta: { align: "right" },
        cell: ({ row }) => (
          <div>
            <div>{quote(row.original.latest_price, row.original.currency)}</div>
            <div className="text-xs text-muted">{date(row.original.latest_price_date)}</div>
          </div>
        ),
      },
      {
        accessorKey: "latest_change",
        header: "Änderung",
        meta: { align: "right" },
        cell: (c) => (
          <span className={tone(c.getValue<number>())}>{percent(c.getValue<number>(), { sign: true })}</span>
        ),
      },
      {
        accessorKey: "shares",
        header: "Bestand",
        meta: { align: "right", hideMobile: true },
        cell: (c) => (c.getValue<number>() ? shares(c.getValue<number>()) : "–"),
      },
      {
        accessorKey: "market_value",
        header: "Marktwert",
        meta: { align: "right" },
        cell: (c) => (c.getValue<number>() ? money(c.getValue<number>()) : "–"),
      },
      {
        accessorKey: "prices",
        header: "Kurse",
        meta: { align: "right", hideMobile: true },
        cell: (c) => c.getValue<number>().toLocaleString("de-DE"),
      },
      {
        accessorKey: "feed",
        header: "Quelle",
        meta: { hideMobile: true },
        cell: ({ row }) => (
          <span
            className="inline-flex items-center gap-1 text-xs text-muted"
            title={row.original.feed_status?.message ?? undefined}
          >
            {row.original.feed_status && !row.original.feed_status.ok && (
              <AlertTriangle className="size-3.5 text-warn" />
            )}
            {row.original.feed ?? "–"}
          </span>
        ),
      },
    ],
    [],
  );

  return (
    <>
      <PageHeader
        title="Wertpapiere"
        subtitle={data ? `${data.length} Wertpapiere` : undefined}
        actions={
          <>
            <Button variant="outline" onClick={updateAll} disabled={updating}>
              {updating ? <Spinner className="size-4" /> : <RefreshCw />}
              Kurse aktualisieren
            </Button>
            <Button onClick={() => setOpen(true)}>
              <Plus />
              Wertpapier
            </Button>
          </>
        }
      />
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative w-full max-w-xs">
          <Search className="absolute top-2.5 left-2.5 size-4 text-muted" />
          <Input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Suche nach Name, ISIN, WKN …"
            className="pl-8"
          />
        </div>
        <Segmented
          value={filter}
          onChange={setFilter}
          options={[
            { key: "held", label: "Im Bestand" },
            { key: "all", label: "Aktiv" },
            { key: "retired", label: "Inaktiv" },
          ]}
        />
      </div>
      {error ? (
        <ErrorBox error={error} />
      ) : isLoading ? (
        <Skeleton className="h-96" />
      ) : (
        <Card>
          <DataTable
            data={rows}
            columns={columns}
            rowHeight={56}
            getRowId={(r) => r.id}
            onRowClick={(r) =>
              navigate({ to: "/c/$clientId/securities/$securityId", params: { clientId, securityId: r.id } })
            }
            renderCard={(s) => (
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{s.name}</div>
                  <div className="truncate text-xs text-muted">{s.isin ?? s.ticker}</div>
                </div>
                <div className="text-right">
                  <div className="tabular text-sm">{quote(s.latest_price, s.currency)}</div>
                  <div className={`tabular text-xs ${tone(s.latest_change)}`}>
                    {percent(s.latest_change, { sign: true })}
                  </div>
                </div>
              </div>
            )}
          />
        </Card>
      )}
      <SecurityDialog
        open={open}
        onOpenChange={setOpen}
        onCreated={(sid) =>
          navigate({ to: "/c/$clientId/securities/$securityId", params: { clientId, securityId: sid } })
        }
      />
      <Dialog open={!!results} onOpenChange={(o) => !o && setResults(null)} title="Kursaktualisierung">
        <ul className="max-h-[60dvh] divide-y divide-border overflow-y-auto text-sm">
          {results?.length === 0 && (
            <li className="py-2 text-muted">Keine Wertpapiere mit automatischer Kursquelle.</li>
          )}
          {results?.map((r) => (
            <li key={r.security_id} className="flex items-start justify-between gap-3 py-2">
              <div className="min-w-0">
                <div className="truncate font-medium">{r.name}</div>
                <div className={r.ok ? "text-xs text-muted" : "text-xs text-neg"}>{r.ok ? `${r.feed}` : r.message}</div>
              </div>
              <span className={r.ok ? "tabular text-pos" : "text-neg"}>{r.ok ? `+${r.new_prices}` : "Fehler"}</span>
            </li>
          ))}
        </ul>
      </Dialog>
    </>
  );
}
