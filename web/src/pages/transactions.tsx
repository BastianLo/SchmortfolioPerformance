import { useNavigate, useSearch } from "@tanstack/react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Plus, Search, Trash2 } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import {
  useAccounts,
  useDeleteTransaction,
  useInvalidateClient,
  usePortfolios,
  useSecurities,
  useTransactions,
} from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { DataTable } from "@/components/ui/data-table";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { ErrorBox, PageHeader, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { AMOUNT, date, isoDate, money, SHARES, shares, TX_LABELS } from "@/lib/format";
import { cn } from "@/lib/utils";

type Row = Schemas["TransactionRow"];

const NEW_TYPES = [
  "BUY",
  "SELL",
  "DEPOSIT",
  "REMOVAL",
  "DIVIDENDS",
  "INTEREST",
  "FEES",
  "TAXES",
  "TAX_REFUND",
  "FEES_REFUND",
  "INTEREST_CHARGE",
];

function parseNum(s: string) {
  const v = Number.parseFloat(s.replace(/\./g, "").replace(",", "."));
  return Number.isFinite(v) ? v : 0;
}

function NewTransactionDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const clientId = useClientId();
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const { data: securities } = useSecurities(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [kind, setKind] = React.useState("BUY");
  const [portfolio, setPortfolio] = React.useState("");
  const [account, setAccount] = React.useState("");
  const [security, setSecurity] = React.useState("");
  const [day, setDay] = React.useState(isoDate(new Date()));
  const [qty, setQty] = React.useState("");
  const [price, setPrice] = React.useState("");
  const [amount, setAmount] = React.useState("");
  const [fees, setFees] = React.useState("");
  const [taxes, setTaxes] = React.useState("");
  const [note, setNote] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const trade = kind === "BUY" || kind === "SELL";
  const needsSecurity = trade || kind === "DIVIDENDS";

  React.useEffect(() => {
    if (!portfolio && portfolios?.[0]) setPortfolio(portfolios[0].id);
  }, [portfolios, portfolio]);
  React.useEffect(() => {
    const p = portfolios?.find((x) => x.id === portfolio);
    if (trade && p?.reference_account) setAccount(p.reference_account);
    else if (!account && accounts?.[0]) setAccount(accounts[0].id);
  }, [portfolio, portfolios, accounts, account, trade]);

  const gross = trade ? parseNum(qty) * parseNum(price) : parseNum(amount);
  const total = trade
    ? kind === "BUY"
      ? gross + parseNum(fees) + parseNum(taxes)
      : gross - parseNum(fees) - parseNum(taxes)
    : kind === "DIVIDENDS" || kind === "INTEREST"
      ? gross - parseNum(fees) - parseNum(taxes)
      : gross;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await unwrap(
        api.POST("/clients/{id}/transactions", {
          params: { path: { id: clientId } },
          body: {
            kind,
            account_id: account,
            portfolio_id: trade ? portfolio : null,
            security_id: needsSecurity ? security : null,
            date_time: `${day}T00:00:00`,
            amount: Math.round(total * AMOUNT),
            shares: trade || kind === "DIVIDENDS" ? Math.round(parseNum(qty) * SHARES) : 0,
            fees: Math.round(parseNum(fees) * AMOUNT),
            taxes: Math.round(parseNum(taxes) * AMOUNT),
            note: note || null,
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
    <Dialog open={open} onOpenChange={onOpenChange} title="Neue Buchung">
      <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <Field label="Typ">
          <Select value={kind} onChange={(e) => setKind(e.target.value)}>
            {NEW_TYPES.map((t) => (
              <option key={t} value={t}>
                {TX_LABELS[t]}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Datum">
          <Input type="date" value={day} onChange={(e) => setDay(e.target.value)} required />
        </Field>
        {trade && (
          <Field label="Depot">
            <Select value={portfolio} onChange={(e) => setPortfolio(e.target.value)} required>
              {portfolios?.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field label={trade ? "Verrechnungskonto" : "Konto"}>
          <Select value={account} onChange={(e) => setAccount(e.target.value)} required>
            {accounts?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name} ({a.currency})
              </option>
            ))}
          </Select>
        </Field>
        {needsSecurity && (
          <div className="sm:col-span-2">
            <Field label="Wertpapier">
              <Select value={security} onChange={(e) => setSecurity(e.target.value)} required>
                <option value="">Bitte wählen …</option>
                {securities
                  ?.filter((s) => !s.retired)
                  .map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name} {s.isin ? `(${s.isin})` : ""}
                    </option>
                  ))}
              </Select>
            </Field>
          </div>
        )}
        {trade ? (
          <>
            <Field label="Stück">
              <Input inputMode="decimal" value={qty} onChange={(e) => setQty(e.target.value)} required />
            </Field>
            <Field label="Kurs">
              <Input inputMode="decimal" value={price} onChange={(e) => setPrice(e.target.value)} required />
            </Field>
          </>
        ) : (
          <div className="sm:col-span-2">
            <Field label={kind === "DIVIDENDS" || kind === "INTEREST" ? "Bruttobetrag" : "Betrag"}>
              <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} required />
            </Field>
          </div>
        )}
        {(trade || kind === "DIVIDENDS" || kind === "INTEREST") && (
          <>
            <Field label="Gebühren">
              <Input inputMode="decimal" value={fees} onChange={(e) => setFees(e.target.value)} placeholder="0,00" />
            </Field>
            <Field label="Steuern">
              <Input inputMode="decimal" value={taxes} onChange={(e) => setTaxes(e.target.value)} placeholder="0,00" />
            </Field>
          </>
        )}
        <div className="sm:col-span-2">
          <Field label="Notiz">
            <Input value={note} onChange={(e) => setNote(e.target.value)} />
          </Field>
        </div>
        <div className="flex items-center justify-between rounded-lg bg-surface-2 px-3 py-2 text-sm sm:col-span-2">
          <span className="text-muted">{kind === "BUY" ? "Belastung Konto" : "Buchungsbetrag"}</span>
          <span className="tabular font-semibold">{money(Math.round(total * AMOUNT))}</span>
        </div>
        {error != null && (
          <div className="sm:col-span-2">
            <ErrorBox error={error} />
          </div>
        )}
        <div className="flex justify-end gap-2 sm:col-span-2">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Abbrechen
          </Button>
          <Button type="submit">Buchen</Button>
        </div>
      </form>
    </Dialog>
  );
}

export function TransactionsPage() {
  const clientId = useClientId();
  const search = useSearch({ strict: false }) as { owner?: string };
  const navigate = useNavigate();
  const [q, setQ] = React.useState("");
  const [deferredQ, setDeferredQ] = React.useState("");
  const [type, setType] = React.useState("");
  const [open, setOpen] = React.useState(false);
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
          <div className="max-w-72 truncate">
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
    [del],
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
      <NewTransactionDialog open={open} onOpenChange={setOpen} />
    </>
  );
}
