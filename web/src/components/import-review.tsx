import { CheckCircle2, Upload } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useAccounts, useInvalidateClient, usePortfolios } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Field, Select } from "@/components/ui/input";
import { Badge, ErrorBox } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, shares, TX_LABELS } from "@/lib/format";
import { cn } from "@/lib/utils";

export type ReviewRow = {
  key: string;
  tx: Schemas["PreviewTx"];
  selected: boolean;
  fileName?: string | null;
  fileHash?: string | null;
};

export function Dropzone({ onFiles, accept, label }: { onFiles: (f: File[]) => void; accept: string; label: string }) {
  const [over, setOver] = React.useState(false);
  return (
    <label
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        onFiles(Array.from(e.dataTransfer.files));
      }}
      className={cn(
        "flex cursor-pointer flex-col items-center gap-2 rounded-xl border-2 border-dashed border-border p-8 text-center hover:border-accent",
        over && "border-accent bg-accent/5",
      )}
    >
      <Upload className="size-6 text-muted" />
      <span className="text-sm font-medium">{label}</span>
      <span className="text-xs text-muted">Klicken oder Dateien hierher ziehen</span>
      <input
        type="file"
        multiple
        accept={accept}
        className="sr-only"
        onChange={(e) => onFiles(Array.from(e.target.files ?? []))}
      />
    </label>
  );
}

/** Table of recognised transactions with selection and commit. */
export function ImportReview({
  rows,
  setRows,
  onDone,
}: {
  rows: ReviewRow[];
  setRows: (r: ReviewRow[]) => void;
  onDone: (r: Schemas["CommitResult"]) => void;
}) {
  const clientId = useClientId();
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [account, setAccount] = React.useState("");
  const [portfolio, setPortfolio] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<unknown>(null);

  React.useEffect(() => {
    if (!portfolio && portfolios?.[0]) setPortfolio(portfolios[0].id);
  }, [portfolios, portfolio]);
  React.useEffect(() => {
    const ref = portfolios?.find((p) => p.id === portfolio)?.reference_account;
    if (ref) setAccount(ref);
    else if (!account && accounts?.[0]) setAccount(accounts[0].id);
  }, [portfolio, portfolios, accounts, account]);

  const selected = rows.filter((r) => r.selected).length;
  const commit = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await unwrap(
        api.POST("/clients/{id}/imports/commit", {
          params: { path: { id: clientId } },
          body: {
            account_id: account,
            portfolio_id: portfolio || null,
            items: rows
              .filter((r) => r.selected)
              .map((r) => ({ tx: r.tx, file_name: r.fileName ?? null, file_hash: r.fileHash ?? null })),
          },
        }),
      );
      invalidate();
      onDone(r);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader title={`Erkannte Buchungen (${rows.length})`} subtitle="Mögliche Duplikate sind abgewählt." />
      <CardBody className="grid gap-3 sm:grid-cols-2">
        <Field label="Depot">
          <Select value={portfolio} onChange={(e) => setPortfolio(e.target.value)}>
            {portfolios?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Verrechnungskonto">
          <Select value={account} onChange={(e) => setAccount(e.target.value)}>
            {accounts?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name} ({a.currency})
              </option>
            ))}
          </Select>
        </Field>
      </CardBody>
      <div className="max-h-[60dvh] overflow-auto">
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-surface">
            <tr className="border-y border-border text-xs text-muted">
              <th className="w-10 px-4 py-2 sm:px-5">
                <input
                  type="checkbox"
                  className="size-4"
                  checked={selected === rows.length}
                  onChange={(e) => setRows(rows.map((x) => ({ ...x, selected: e.target.checked })))}
                  aria-label="Alle auswählen"
                />
              </th>
              <th className="px-3 py-2 text-left font-medium">Datum</th>
              <th className="px-3 py-2 text-left font-medium">Typ</th>
              <th className="px-3 py-2 text-left font-medium">Wertpapier</th>
              <th className="hidden px-3 py-2 text-right font-medium md:table-cell">Stück</th>
              <th className="hidden px-3 py-2 text-right font-medium md:table-cell">Gebühren</th>
              <th className="hidden px-3 py-2 text-right font-medium md:table-cell">Steuern</th>
              <th className="px-4 py-2 text-right font-medium sm:px-5">Betrag</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key} className={cn("border-b border-border/60", !r.selected && "opacity-50")}>
                <td className="px-4 py-2 sm:px-5">
                  <input
                    type="checkbox"
                    className="size-4"
                    checked={r.selected}
                    onChange={(e) =>
                      setRows(rows.map((x) => (x.key === r.key ? { ...x, selected: e.target.checked } : x)))
                    }
                  />
                </td>
                <td className="tabular px-3 py-2">{date(r.tx.date_time)}</td>
                <td className="px-3 py-2">
                  {TX_LABELS[r.tx.kind] ?? r.tx.kind}
                  {r.tx.duplicate_of && <Badge className="ml-1 bg-warn/15 text-warn">Duplikat?</Badge>}
                </td>
                <td className="max-w-64 px-3 py-2">
                  <div className="truncate">{r.tx.name}</div>
                  <div className="text-xs text-muted">
                    {r.tx.isin}
                    {!r.tx.security_id && (r.tx.isin || r.tx.name) && <Badge className="ml-1">neu</Badge>}
                  </div>
                </td>
                <td className="tabular hidden px-3 py-2 text-right md:table-cell">
                  {r.tx.shares ? shares(r.tx.shares) : ""}
                </td>
                <td className="tabular hidden px-3 py-2 text-right md:table-cell">
                  {r.tx.fees ? money(r.tx.fees, r.tx.currency) : ""}
                </td>
                <td className="tabular hidden px-3 py-2 text-right md:table-cell">
                  {r.tx.taxes ? money(r.tx.taxes, r.tx.currency) : ""}
                </td>
                <td className="tabular px-4 py-2 text-right font-medium sm:px-5">
                  {money(r.tx.amount, r.tx.currency)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="flex items-center justify-end gap-3 p-4 sm:p-5">
        {error != null && <ErrorBox error={error} />}
        <Button onClick={commit} disabled={busy || !selected || !account}>
          {selected} Buchungen übernehmen
        </Button>
      </div>
    </Card>
  );
}

export function ImportResult({ result, onOpen }: { result: Schemas["CommitResult"]; onOpen: () => void }) {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-pos/30 bg-pos/10 p-3 text-sm">
      <CheckCircle2 className="mt-0.5 size-4 text-pos" />
      <div>
        {result.created} Buchungen importiert, {result.securities_created} Wertpapiere neu angelegt.
        {result.errors.length > 0 && (
          <ul className="mt-1 list-disc pl-4 text-neg">
            {result.errors.map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        )}
        <button type="button" className="ml-2 cursor-pointer text-accent" onClick={onOpen}>
          Zu den Buchungen
        </button>
      </div>
    </div>
  );
}
