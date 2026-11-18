import { useNavigate } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Plus, Search } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useInvalidateClient, useSecurities } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DataTable } from "@/components/ui/data-table";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import { Badge, ErrorBox, PageHeader, Segmented, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, percent, quote, shares, tone } from "@/lib/format";

type Row = Schemas["SecurityDto"];

function NewSecurityDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const clientId = useClientId();
  const invalidate = useInvalidateClient(clientId);
  const [form, setForm] = React.useState({ name: "", isin: "", wkn: "", ticker: "", currency: "EUR" });
  const [error, setError] = React.useState<unknown>(null);
  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm({ ...form, [k]: e.target.value });
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await unwrap(
        api.POST("/clients/{id}/securities", {
          params: { path: { id: clientId } },
          body: {
            name: form.name,
            isin: form.isin || null,
            wkn: form.wkn || null,
            ticker: form.ticker || null,
            currency: form.currency || null,
          },
        }),
      );
      await invalidate();
      onOpenChange(false);
    } catch (err) {
      setError(err);
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Neues Wertpapier">
      <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <div className="sm:col-span-2">
          <Field label="Name">
            <Input value={form.name} onChange={set("name")} required autoFocus />
          </Field>
        </div>
        <Field label="ISIN">
          <Input value={form.isin} onChange={set("isin")} maxLength={12} />
        </Field>
        <Field label="WKN">
          <Input value={form.wkn} onChange={set("wkn")} />
        </Field>
        <Field label="Ticker">
          <Input value={form.ticker} onChange={set("ticker")} />
        </Field>
        <Field label="Währung">
          <Input value={form.currency} onChange={set("currency")} maxLength={3} />
        </Field>
        {error != null && (
          <div className="sm:col-span-2">
            <ErrorBox error={error} />
          </div>
        )}
        <div className="flex justify-end sm:col-span-2">
          <Button type="submit">Anlegen</Button>
        </div>
      </form>
    </Dialog>
  );
}

export function SecuritiesPage() {
  const clientId = useClientId();
  const { data, error, isLoading } = useSecurities(clientId);
  const navigate = useNavigate();
  const [q, setQ] = React.useState("");
  const [filter, setFilter] = React.useState<"held" | "all" | "retired">("held");
  const [open, setOpen] = React.useState(false);

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
        cell: (c) => <span className="text-xs text-muted">{c.getValue<string>() ?? "–"}</span>,
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
          <Button onClick={() => setOpen(true)}>
            <Plus />
            Wertpapier
          </Button>
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
      <NewSecurityDialog open={open} onOpenChange={setOpen} />
    </>
  );
}
