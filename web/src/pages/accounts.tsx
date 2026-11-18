import { Link } from "@tanstack/react-router";
import { Briefcase, Plus, Trash2, Wallet } from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { useAccounts, useClient, useInvalidateClient, usePortfolios } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { Badge, ErrorBox, PageHeader, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { money } from "@/lib/format";

function CreateDialog({
  kind,
  open,
  onOpenChange,
}: {
  kind: "account" | "portfolio";
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const clientId = useClientId();
  const { data: client } = useClient(clientId);
  const { data: accounts } = useAccounts(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [name, setName] = React.useState("");
  const [currency, setCurrency] = React.useState(client?.base_currency ?? "EUR");
  const [reference, setReference] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      if (kind === "account")
        await unwrap(
          api.POST("/clients/{id}/accounts", { params: { path: { id: clientId } }, body: { name, currency } }),
        );
      else
        await unwrap(
          api.POST("/clients/{id}/portfolios", {
            params: { path: { id: clientId } },
            body: { name, reference_account: reference || accounts?.[0]?.id || null },
          }),
        );
      await invalidate();
      setName("");
      onOpenChange(false);
    } catch (err) {
      setError(err);
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={kind === "account" ? "Neues Konto" : "Neues Depot"}>
      <form onSubmit={submit} className="space-y-4">
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
        </Field>
        {kind === "account" ? (
          <Field label="Währung">
            <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={3} />
          </Field>
        ) : (
          <Field label="Referenzkonto" hint="Verrechnungskonto für Käufe und Verkäufe">
            <Select value={reference} onChange={(e) => setReference(e.target.value)}>
              {accounts?.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {error != null && <ErrorBox error={error} />}
        <div className="flex justify-end">
          <Button type="submit">Anlegen</Button>
        </div>
      </form>
    </Dialog>
  );
}

export function AccountsPage() {
  const clientId = useClientId();
  const { data: accounts, isLoading: la } = useAccounts(clientId);
  const { data: portfolios, isLoading: lp } = usePortfolios(clientId);
  const { data: client } = useClient(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [dialog, setDialog] = React.useState<"account" | "portfolio" | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const cur = client?.base_currency ?? "EUR";

  const remove = async (kind: "accounts" | "portfolios", id: string, name: string) => {
    if (!confirm(`„${name}“ löschen?`)) return;
    setError(null);
    try {
      if (kind === "accounts")
        await unwrap(api.DELETE("/clients/{id}/accounts/{aid}", { params: { path: { id: clientId, aid: id } } }));
      else await unwrap(api.DELETE("/clients/{id}/portfolios/{pid}", { params: { path: { id: clientId, pid: id } } }));
      invalidate();
    } catch (e) {
      setError(e);
    }
  };

  if (la || lp) return <Skeleton className="h-96" />;
  return (
    <>
      <PageHeader title="Konten & Depots" />
      {error != null && (
        <div className="mb-4">
          <ErrorBox error={error} />
        </div>
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader
            title="Konten"
            subtitle={`Summe ${money(accounts?.reduce((s, a) => s + a.balance_converted, 0) ?? 0, cur)}`}
            actions={
              <Button size="sm" variant="outline" onClick={() => setDialog("account")}>
                <Plus /> Konto
              </Button>
            }
          />
          <ul className="mt-2 divide-y divide-border">
            {accounts?.map((a) => (
              <li key={a.id} className="group flex items-center gap-3 px-4 py-3 sm:px-5">
                <div className="rounded-lg bg-surface-2 p-2 text-muted">
                  <Wallet className="size-4" />
                </div>
                <Link
                  to="/c/$clientId/transactions"
                  params={{ clientId }}
                  search={{ owner: a.id } as never}
                  className="min-w-0 flex-1"
                >
                  <div className="truncate text-sm font-medium">
                    {a.name} {a.retired && <Badge>inaktiv</Badge>}
                  </div>
                  <div className="text-xs text-muted">
                    {a.transactions} Buchungen · {a.currency}
                  </div>
                </Link>
                <div className="tabular text-right text-sm font-medium">{money(a.balance, a.currency)}</div>
                <button
                  type="button"
                  onClick={() => remove("accounts", a.id, a.name)}
                  className="cursor-pointer p-1 text-muted opacity-0 group-hover:opacity-100"
                  aria-label="Löschen"
                >
                  <Trash2 className="size-4" />
                </button>
              </li>
            ))}
          </ul>
        </Card>
        <Card>
          <CardHeader
            title="Depots"
            subtitle={`Summe ${money(portfolios?.reduce((s, p) => s + p.value, 0) ?? 0, cur)}`}
            actions={
              <Button size="sm" variant="outline" onClick={() => setDialog("portfolio")} disabled={!accounts?.length}>
                <Plus /> Depot
              </Button>
            }
          />
          <ul className="mt-2 divide-y divide-border">
            {portfolios?.map((p) => (
              <li key={p.id} className="group flex items-center gap-3 px-4 py-3 sm:px-5">
                <div className="rounded-lg bg-surface-2 p-2 text-muted">
                  <Briefcase className="size-4" />
                </div>
                <Link
                  to="/c/$clientId/transactions"
                  params={{ clientId }}
                  search={{ owner: p.id } as never}
                  className="min-w-0 flex-1"
                >
                  <div className="truncate text-sm font-medium">
                    {p.name} {p.retired && <Badge>inaktiv</Badge>}
                  </div>
                  <div className="text-xs text-muted">
                    {p.positions} Positionen · {p.transactions} Buchungen · Referenz{" "}
                    {accounts?.find((a) => a.id === p.reference_account)?.name ?? "–"}
                  </div>
                </Link>
                <div className="tabular text-right text-sm font-medium">{money(p.value, cur)}</div>
                <button
                  type="button"
                  onClick={() => remove("portfolios", p.id, p.name)}
                  className="cursor-pointer p-1 text-muted opacity-0 group-hover:opacity-100"
                  aria-label="Löschen"
                >
                  <Trash2 className="size-4" />
                </button>
              </li>
            ))}
          </ul>
        </Card>
      </div>
      {dialog && <CreateDialog kind={dialog} open onOpenChange={(o) => !o && setDialog(null)} />}
    </>
  );
}
