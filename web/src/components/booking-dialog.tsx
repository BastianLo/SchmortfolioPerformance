import { useQuery } from "@tanstack/react-query";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { qk, useAccounts, useInvalidateClient, usePortfolios, useSecurities } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { ErrorBox, Spinner } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { AMOUNT, isoDate, money, SHARES } from "@/lib/format";

type Kind = Schemas["BookingKind"];

export const BOOKING_KINDS: { key: Kind; label: string }[] = [
  { key: "BUY", label: "Kauf" },
  { key: "SELL", label: "Verkauf" },
  { key: "DIVIDENDS", label: "Dividende" },
  { key: "DEPOSIT", label: "Einlage" },
  { key: "REMOVAL", label: "Entnahme" },
  { key: "CASH_TRANSFER", label: "Umbuchung Konto → Konto" },
  { key: "SECURITY_TRANSFER", label: "Depotwechsel" },
  { key: "DELIVERY_INBOUND", label: "Einlieferung" },
  { key: "DELIVERY_OUTBOUND", label: "Auslieferung" },
  { key: "INTEREST", label: "Zinsen" },
  { key: "INTEREST_CHARGE", label: "Zinsbelastung" },
  { key: "FEES", label: "Gebühren" },
  { key: "FEES_REFUND", label: "Gebührenerstattung" },
  { key: "TAXES", label: "Steuern" },
  { key: "TAX_REFUND", label: "Steuerrückerstattung" },
];

export function parseNum(s: string) {
  const t = s.trim();
  if (!t) return 0;
  // accept "1.234,56" and "1234.56"
  const norm = t.includes(",") ? t.replace(/\./g, "").replace(",", ".") : t;
  const v = Number.parseFloat(norm);
  return Number.isFinite(v) ? v : 0;
}

export function fmtNum(v: number, digits = 2) {
  if (!v) return "";
  return v.toLocaleString("de-DE", { maximumFractionDigits: digits, useGrouping: false });
}

const TRADE: Kind[] = ["BUY", "SELL", "DELIVERY_INBOUND", "DELIVERY_OUTBOUND"];

export function BookingDialog({
  open,
  onOpenChange,
  editId,
  preset,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** id of a transaction to edit */
  editId?: string;
  preset?: { kind?: Kind; securityId?: string; portfolioId?: string; accountId?: string };
}) {
  const clientId = useClientId();
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const { data: securities } = useSecurities(clientId);
  const invalidate = useInvalidateClient(clientId);
  const existing = useQuery({
    queryKey: [...qk.client(clientId), "booking", editId],
    queryFn: () =>
      unwrap(
        api.GET("/clients/{id}/transactions/{tid}", { params: { path: { id: clientId, tid: editId as string } } }),
      ),
    enabled: open && !!editId,
  });

  const [kind, setKind] = React.useState<Kind>(preset?.kind ?? "BUY");
  const [portfolio, setPortfolio] = React.useState(preset?.portfolioId ?? "");
  const [targetPortfolio, setTargetPortfolio] = React.useState("");
  const [account, setAccount] = React.useState(preset?.accountId ?? "");
  const [targetAccount, setTargetAccount] = React.useState("");
  const [security, setSecurity] = React.useState(preset?.securityId ?? "");
  const [day, setDay] = React.useState(isoDate(new Date()));
  const [qty, setQty] = React.useState("");
  const [price, setPrice] = React.useState("");
  const [amount, setAmount] = React.useState("");
  const [targetAmount, setTargetAmount] = React.useState("");
  const [rate, setRate] = React.useState("");
  const [fees, setFees] = React.useState("");
  const [taxes, setTaxes] = React.useState("");
  const [note, setNote] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const [busy, setBusy] = React.useState(false);

  // fill the form when editing
  React.useEffect(() => {
    const b = existing.data?.booking;
    if (!b) return;
    setKind(b.kind);
    setPortfolio(b.portfolio_id ?? "");
    setTargetPortfolio(b.target_portfolio_id ?? "");
    setAccount(b.account_id ?? "");
    setTargetAccount(b.target_account_id ?? "");
    setSecurity(b.security_id ?? "");
    setDay(b.date_time.slice(0, 10));
    const fee = (b.fees ?? 0) / AMOUNT;
    const tax = (b.taxes ?? 0) / AMOUNT;
    const shares = (b.shares ?? 0) / SHARES;
    const r = b.exchange_rate ? Number.parseFloat(b.exchange_rate) : 1;
    setFees(fmtNum(fee));
    setTaxes(fmtNum(tax));
    setQty(fmtNum(shares, 8));
    setRate(b.exchange_rate ? fmtNum(r, 6) : "");
    setNote(b.note ?? "");
    const total = b.amount / AMOUNT;
    if (TRADE.includes(b.kind)) {
      const purchase = b.kind === "BUY" || b.kind === "DELIVERY_INBOUND";
      const gross = purchase ? total - fee - tax : total + fee + tax;
      setPrice(shares ? fmtNum(gross / r / shares, 6) : "");
    } else if (b.kind === "DIVIDENDS" || b.kind === "INTEREST") {
      setAmount(fmtNum(total + fee + tax));
    } else {
      setAmount(fmtNum(total));
    }
    setTargetAmount(b.target_amount ? fmtNum(b.target_amount / AMOUNT) : "");
  }, [existing.data]);

  React.useEffect(() => {
    if (!open) setError(null);
  }, [open]);

  const trade = TRADE.includes(kind);
  const needsSecurity = trade || kind === "DIVIDENDS" || kind === "SECURITY_TRANSFER";
  const needsPortfolio = trade || kind === "SECURITY_TRANSFER";
  const needsAccount = !(kind === "DELIVERY_INBOUND" || kind === "DELIVERY_OUTBOUND" || kind === "SECURITY_TRANSFER");
  const withFees = trade || kind === "DIVIDENDS" || kind === "INTEREST";

  React.useEffect(() => {
    if (!portfolio && portfolios?.[0]) setPortfolio(portfolios[0].id);
  }, [portfolios, portfolio]);
  React.useEffect(() => {
    if (editId) return;
    const p = portfolios?.find((x) => x.id === portfolio);
    if ((kind === "BUY" || kind === "SELL") && p?.reference_account) setAccount(p.reference_account);
    else if (!account && accounts?.[0]) setAccount(accounts[0].id);
  }, [portfolio, portfolios, accounts, account, kind, editId]);

  const sec = securities?.find((s) => s.id === security);
  const acc = accounts?.find((a) => a.id === account);
  const pf = portfolios?.find((p) => p.id === portfolio);
  const txCurrency =
    kind === "DELIVERY_INBOUND" || kind === "DELIVERY_OUTBOUND"
      ? (accounts?.find((a) => a.id === pf?.reference_account)?.currency ?? sec?.currency ?? "EUR")
      : kind === "SECURITY_TRANSFER"
        ? (sec?.currency ?? "EUR")
        : (acc?.currency ?? "EUR");
  const secCurrency = sec?.currency ?? txCurrency;
  const forex = needsSecurity && kind !== "SECURITY_TRANSFER" && secCurrency !== txCurrency;
  const tgtAcc = accounts?.find((a) => a.id === targetAccount);
  const cashForex = kind === "CASH_TRANSFER" && tgtAcc && acc && tgtAcc.currency !== acc.currency;
  const r = forex ? parseNum(rate) : 1;

  const gross = trade ? parseNum(qty) * parseNum(price) * r : parseNum(amount);
  const purchase = kind === "BUY" || kind === "DELIVERY_INBOUND";
  const total = trade
    ? purchase
      ? gross + parseNum(fees) + parseNum(taxes)
      : gross - parseNum(fees) - parseNum(taxes)
    : kind === "DIVIDENDS" || kind === "INTEREST"
      ? gross - parseNum(fees) - parseNum(taxes)
      : gross;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    const body: Schemas["Booking"] = {
      kind,
      date_time: `${day}T00:00:00`,
      account_id: needsAccount ? account : null,
      portfolio_id: needsPortfolio ? portfolio : null,
      target_account_id: kind === "CASH_TRANSFER" ? targetAccount : null,
      target_portfolio_id: kind === "SECURITY_TRANSFER" ? targetPortfolio : null,
      security_id: needsSecurity ? security : null,
      amount: Math.round(total * AMOUNT),
      shares: needsSecurity ? Math.round(parseNum(qty) * SHARES) : 0,
      fees: withFees ? Math.round(parseNum(fees) * AMOUNT) : 0,
      taxes: withFees ? Math.round(parseNum(taxes) * AMOUNT) : 0,
      exchange_rate: forex && rate ? String(parseNum(rate)) : null,
      target_amount: cashForex ? Math.round(parseNum(targetAmount) * AMOUNT) : null,
      note: note || null,
    };
    try {
      if (editId) {
        await unwrap(
          api.PUT("/clients/{id}/transactions/{tid}", { params: { path: { id: clientId, tid: editId } }, body }),
        );
      } else {
        await unwrap(api.POST("/clients/{id}/transactions", { params: { path: { id: clientId } }, body }));
      }
      await invalidate();
      onOpenChange(false);
    } catch (err) {
      setError(err);
    } finally {
      setBusy(false);
    }
  };

  const loading = !!editId && existing.isLoading;

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={editId ? "Buchung bearbeiten" : "Neue Buchung"}>
      {loading ? (
        <div className="flex justify-center py-8">
          <Spinner />
        </div>
      ) : (
        <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
          <Field label="Typ">
            <Select value={kind} onChange={(e) => setKind(e.target.value as Kind)} disabled={!!editId}>
              {BOOKING_KINDS.map((t) => (
                <option key={t.key} value={t.key}>
                  {t.label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Datum">
            <Input type="date" value={day} onChange={(e) => setDay(e.target.value)} required />
          </Field>
          {needsPortfolio && (
            <Field label={kind === "SECURITY_TRANSFER" ? "Von Depot" : "Depot"}>
              <Select value={portfolio} onChange={(e) => setPortfolio(e.target.value)} required>
                {portfolios?.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          {kind === "SECURITY_TRANSFER" && (
            <Field label="Nach Depot">
              <Select value={targetPortfolio} onChange={(e) => setTargetPortfolio(e.target.value)} required>
                <option value="">Bitte wählen …</option>
                {portfolios
                  ?.filter((p) => p.id !== portfolio)
                  .map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
              </Select>
            </Field>
          )}
          {needsAccount && (
            <Field
              label={
                kind === "BUY" || kind === "SELL"
                  ? "Verrechnungskonto"
                  : kind === "CASH_TRANSFER"
                    ? "Von Konto"
                    : "Konto"
              }
            >
              <Select value={account} onChange={(e) => setAccount(e.target.value)} required>
                {accounts?.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name} ({a.currency})
                  </option>
                ))}
              </Select>
            </Field>
          )}
          {kind === "CASH_TRANSFER" && (
            <Field label="Nach Konto">
              <Select value={targetAccount} onChange={(e) => setTargetAccount(e.target.value)} required>
                <option value="">Bitte wählen …</option>
                {accounts
                  ?.filter((a) => a.id !== account)
                  .map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.name} ({a.currency})
                    </option>
                  ))}
              </Select>
            </Field>
          )}
          {needsSecurity && (
            <div className="sm:col-span-2">
              <Field label="Wertpapier">
                <Select value={security} onChange={(e) => setSecurity(e.target.value)} required>
                  <option value="">Bitte wählen …</option>
                  {securities
                    ?.filter((s) => !s.retired || s.id === security)
                    .map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name} {s.isin ? `(${s.isin})` : ""}
                      </option>
                    ))}
                </Select>
              </Field>
            </div>
          )}
          {needsSecurity && (
            <Field label={kind === "DIVIDENDS" ? "Stück (optional)" : "Stück"}>
              <Input
                inputMode="decimal"
                value={qty}
                onChange={(e) => setQty(e.target.value)}
                required={kind !== "DIVIDENDS"}
              />
            </Field>
          )}
          {trade ? (
            <Field label={`Kurs (${secCurrency})`}>
              <Input inputMode="decimal" value={price} onChange={(e) => setPrice(e.target.value)} required />
            </Field>
          ) : (
            <Field
              label={
                kind === "DIVIDENDS" || kind === "INTEREST"
                  ? `Bruttobetrag (${txCurrency})`
                  : kind === "SECURITY_TRANSFER"
                    ? `Wert (${txCurrency})`
                    : `Betrag (${txCurrency})`
              }
            >
              <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} required />
            </Field>
          )}
          {forex && (
            <Field label={`Wechselkurs ${secCurrency} → ${txCurrency}`} hint={`1 ${secCurrency} = x ${txCurrency}`}>
              <Input
                inputMode="decimal"
                value={rate}
                onChange={(e) => setRate(e.target.value)}
                required={trade}
                placeholder="z. B. 0,92"
              />
            </Field>
          )}
          {cashForex && (
            <Field label={`Gutschrift (${tgtAcc?.currency})`}>
              <Input
                inputMode="decimal"
                value={targetAmount}
                onChange={(e) => setTargetAmount(e.target.value)}
                required
              />
            </Field>
          )}
          {withFees && (
            <>
              <Field label={`Gebühren (${txCurrency})`}>
                <Input inputMode="decimal" value={fees} onChange={(e) => setFees(e.target.value)} placeholder="0,00" />
              </Field>
              <Field label={`Steuern (${txCurrency})`}>
                <Input
                  inputMode="decimal"
                  value={taxes}
                  onChange={(e) => setTaxes(e.target.value)}
                  placeholder="0,00"
                />
              </Field>
            </>
          )}
          <div className="sm:col-span-2">
            <Field label="Notiz">
              <Input value={note} onChange={(e) => setNote(e.target.value)} />
            </Field>
          </div>
          <div className="flex items-center justify-between rounded-lg bg-surface-2 px-3 py-2 text-sm sm:col-span-2">
            <span className="text-muted">
              {kind === "BUY" ? "Belastung" : kind === "SELL" || kind === "DIVIDENDS" ? "Gutschrift" : "Buchungsbetrag"}
            </span>
            <span className="tabular font-semibold">{money(Math.round(total * AMOUNT), txCurrency)}</span>
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
            <Button type="submit" disabled={busy}>
              {busy && <Spinner className="size-4 text-current" />}
              {editId ? "Speichern" : "Buchen"}
            </Button>
          </div>
        </form>
      )}
    </Dialog>
  );
}
