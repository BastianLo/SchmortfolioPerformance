import { useNavigate, useSearch } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Paperclip, Plus, Search, Trash2 } from "lucide-react";
import * as React from "react";
import type { Schemas } from "@/api/client";
import { useAccounts, useDeleteTransaction, usePortfolios, useTransactions } from "@/api/hooks";
import { BookingDialog } from "@/components/booking-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DataTable } from "@/components/ui/data-table";
import { Input, Select } from "@/components/ui/input";
import { ErrorBox, PageHeader, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, shares, TX_LABELS } from "@/lib/format";
import { cn } from "@/lib/utils";

type Row = Schemas["TransactionRow"];

export function TransactionsPage() {
  const clientId = useClientId();
  const search = useSearch({ strict: false }) as { owner?: string };
  const navigate = useNavigate();
  const [q, setQ] = React.useState("");
  const [deferredQ, setDeferredQ] = React.useState("");
  const [type, setType] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const [editId, setEditId] = React.useState<string | undefined>();
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const owner = search.owner ?? "";
  React.useEffect(() => {
    const t = setTimeout(() => setDeferredQ(q), 250);
    return () => clearTimeout(t);
  }, [q]);
  const { data, error, isLoading } = useTransactions(clientId, {
    owner: owner || undefined,
    types: type || undefined,
    q: deferredQ || undefined,
  });
  const del = useDeleteTransaction(clientId);

  const columns = React.useMemo<ColumnDef<Row>[]>(
    () => [
      { accessorKey: "date_time", header: "Datum", cell: (c) => date(c.getValue<string>()) },
      {
        accessorKey: "kind",
        header: "Typ",
        cell: ({ row }) => (
          <span
            className={cn(
              "rounded-md px-1.5 py-0.5 text-xs font-medium",
              row.original.sign > 0 ? "bg-pos/10 text-pos" : "bg-surface-2 text-muted",
            )}
          >
            {TX_LABELS[row.original.kind] ?? row.original.kind}
          </span>
        ),
      },
      {
        accessorKey: "security_name",
        header: "Wertpapier / Notiz",
        cell: ({ row }) => (
          <div className="flex max-w-72 items-center gap-1 truncate">
            {row.original.attachments.map((a) => (
              <a
                key={a.file_hash}
                href={`/api/v1/clients/${clientId}/files/${a.file_hash}`}
                target="_blank"
                rel="noreferrer"
                title={a.file_name}
                onClick={(e) => e.stopPropagation()}
                className="shrink-0 text-muted hover:text-accent"
              >
                <Paperclip className="size-3.5" />
              </a>
            ))}
            {row.original.security_name ?? (
              <span className="text-muted">{row.original.note ?? row.original.other_owner_name ?? ""}</span>
            )}
          </div>
        ),
      },
      {
        accessorKey: "owner_name",
        header: "Konto/Depot",
        meta: { hideMobile: true },
        cell: (c) => <span className="text-muted">{c.getValue<string>()}</span>,
      },
      {
        accessorKey: "shares",
        header: "Stück",
        meta: { align: "right", hideMobile: true },
        cell: (c) => (c.getValue<number>() ? shares(c.getValue<number>()) : ""),
      },
      {
        accessorKey: "fees",
        header: "Gebühren",
        meta: { align: "right", hideMobile: true },
        cell: ({ row }) => (row.original.fees ? money(row.original.fees, row.original.currency) : ""),
      },
      {
        accessorKey: "taxes",
        header: "Steuern",
        meta: { align: "right", hideMobile: true },
        cell: ({ row }) => (row.original.taxes ? money(row.original.taxes, row.original.currency) : ""),
      },
      {
        accessorKey: "amount",
        header: "Betrag",
        meta: { align: "right" },
        cell: ({ row }) => <span className="font-medium">{money(row.original.amount, row.original.currency)}</span>,
      },
      {
        id: "actions",
        header: "",
        enableSorting: false,
        cell: ({ row }) => (
          <button
            type="button"
            aria-label="Buchung löschen"
            className="cursor-pointer rounded p-1 text-muted hover:text-neg"
            onClick={(e) => {
              e.stopPropagation();
              if (confirm("Buchung (inkl. Gegenbuchung) löschen?")) del.mutate(row.original.id);
            }}
          >
            <Trash2 className="size-4" />
          </button>
        ),
      },
    ],
    [del, clientId],
  );

  const items = React.useMemo(() => {
    // show each buy/sell once (portfolio side) when no owner filter is set
    if (owner || !data) return data?.items ?? [];
    return data.items.filter((t) => !(t.owner_kind === "account" && (t.kind === "BUY" || t.kind === "SELL")));
  }, [data, owner]);

  return (
    <>
      <PageHeader
        title="Buchungen"
        subtitle={data ? `${items.length.toLocaleString("de-DE")} Einträge` : undefined}
        actions={
          <Button onClick={() => setOpen(true)}>
            <Plus /> Buchung
          </Button>
        }
      />
      <div className="mb-3 grid gap-2 sm:flex sm:flex-wrap">
        <div className="relative sm:w-64">
          <Search className="absolute top-2.5 left-2.5 size-4 text-muted" />
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Suchen …" className="pl-8" />
        </div>
        <Select
          value={owner}
          onChange={(e) =>
            navigate({
              to: "/c/$clientId/transactions",
              params: { clientId },
              search: (e.target.value ? { owner: e.target.value } : {}) as never,
            })
          }
          className="sm:w-56"
        >
          <option value="">Alle Konten & Depots</option>
          <optgroup label="Konten">
            {accounts?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </optgroup>
          <optgroup label="Depots">
            {portfolios?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </optgroup>
        </Select>
        <Select value={type} onChange={(e) => setType(e.target.value)} className="sm:w-52">
          <option value="">Alle Typen</option>
          {Object.entries(TX_LABELS).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </Select>
      </div>
      {error ? (
        <ErrorBox error={error} />
      ) : isLoading ? (
        <Skeleton className="h-96" />
      ) : (
        <Card>
          <DataTable
            data={items}
            columns={columns}
            getRowId={(r) => r.id}
            onRowClick={(r) => {
              setEditId(r.id);
              setOpen(true);
            }}
            maxHeight="calc(100dvh - 260px)"
            renderCard={(t) => (
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{t.security_name ?? TX_LABELS[t.kind]}</div>
                  <div className="text-xs text-muted">
                    {TX_LABELS[t.kind]} · {date(t.date_time)} · {t.owner_name}
                  </div>
                </div>
                <div
                  className={cn("tabular text-sm font-medium", t.owner_kind === "account" && t.sign > 0 && "text-pos")}
                >
                  {money(t.amount, t.currency)}
                </div>
              </div>
            )}
          />
        </Card>
      )}
      <BookingDialog
        key={editId ?? "new"}
        open={open}
        editId={editId}
        onOpenChange={(o: boolean) => {
          setOpen(o);
          if (!o) setEditId(undefined);
        }}
      />
    </>
  );
}
