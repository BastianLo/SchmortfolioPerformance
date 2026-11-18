import { Link, useParams } from "@tanstack/react-router";
import { ArrowLeft } from "lucide-react";
import * as React from "react";
import { useClient, usePrices, useSecurity } from "@/api/hooks";
import { PriceChart } from "@/components/charts/presets";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Badge, ErrorBox, PageHeader, Segmented, Skeleton, Stat } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, percent, quote, shares, TX_LABELS, tone } from "@/lib/format";
import { cn } from "@/lib/utils";

const RANGES = [
  { key: "1y", label: "1J", days: 365 },
  { key: "3y", label: "3J", days: 3 * 365 },
  { key: "5y", label: "5J", days: 5 * 365 },
  { key: "max", label: "Max", days: 0 },
] as const;

export function SecurityDetailPage() {
  const clientId = useClientId();
  const { securityId } = useParams({ strict: false }) as { securityId: string };
  const { data, error, isLoading } = useSecurity(clientId, securityId);
  const { data: prices } = usePrices(clientId, securityId);
  const { data: client } = useClient(clientId);
  const cur = client?.base_currency ?? "EUR";
  const [range, setRange] = React.useState<(typeof RANGES)[number]["key"]>("3y");

  const shown = React.useMemo(() => {
    if (!prices) return [];
    const days = RANGES.find((r) => r.key === range)?.days ?? 0;
    if (!days) return prices;
    const from = new Date(Date.now() - days * 86400_000).toISOString().slice(0, 10);
    return prices.filter((p) => p.date >= from);
  }, [prices, range]);

  const markers = React.useMemo(
    () =>
      (data?.transactions ?? [])
        .filter((t) => t.owner_kind === "portfolio" && (t.kind === "BUY" || t.kind === "SELL"))
        .map((t) => ({
          date: t.date_time.slice(0, 10),
          kind: t.kind === "BUY" ? ("buy" as const) : ("sell" as const),
          label: TX_LABELS[t.kind],
        })),
    [data],
  );

  if (error) return <ErrorBox error={error} />;
  if (isLoading || !data) return <Skeleton className="h-96" />;
  const s = data.security;
  const p = data.performance;

  return (
    <>
      <Link
        to="/c/$clientId/securities"
        params={{ clientId }}
        className="mb-3 inline-flex items-center gap-1 text-sm text-muted hover:text-fg"
      >
        <ArrowLeft className="size-4" /> Wertpapiere
      </Link>
      <PageHeader
        title={s.name}
        subtitle={
          <span className="flex flex-wrap gap-1.5">
            {s.isin && <Badge>ISIN {s.isin}</Badge>}
            {s.wkn && <Badge>WKN {s.wkn}</Badge>}
            {s.ticker && <Badge>{s.ticker}</Badge>}
            {s.currency && <Badge>{s.currency}</Badge>}
            {s.retired && <Badge>inaktiv</Badge>}
          </span>
        }
      />
      <div className="space-y-4">
        <Card>
          <CardBody>
            <div className="grid grid-cols-2 gap-5 sm:grid-cols-3 lg:grid-cols-6">
              <Stat
                label="Letzter Kurs"
                value={quote(s.latest_price, s.currency)}
                sub={
                  <span className={tone(s.latest_change)}>
                    {percent(s.latest_change, { sign: true })} · {date(s.latest_price_date)}
                  </span>
                }
              />
              <Stat label="Bestand" value={`${shares(s.shares)} Stk.`} />
              <Stat label="Marktwert" value={money(p?.market_value ?? 0, cur)} />
              <Stat label="Einstand (FIFO)" value={money(p?.fifo_cost ?? 0, cur)} />
              <Stat
                label="Nicht realisiert"
                value={money(p?.unrealized_gains ?? 0, cur, { sign: true })}
                valueClass={tone(p?.unrealized_gains)}
                sub={
                  <span className={tone(p?.unrealized_gains)}>
                    {percent(p?.unrealized_gains_percent, { sign: true })}
                  </span>
                }
              />
              <Stat label="IZF p.a." value={percent(p?.irr, { sign: true })} valueClass={tone(p?.irr)} />
              <Stat
                label="Realisiert"
                value={money(p?.realized_gains ?? 0, cur, { sign: true })}
                valueClass={tone(p?.realized_gains)}
              />
              <Stat label="Erträge" value={money(p?.dividends ?? 0, cur)} />
              <Stat label="Gebühren" value={money(p?.fees ?? 0, cur)} />
              <Stat label="Steuern" value={money(p?.taxes ?? 0, cur)} />
              <Stat
                label="Gesamtergebnis"
                value={money(p?.delta ?? 0, cur, { sign: true })}
                valueClass={tone(p?.delta)}
              />
              <Stat
                label="Kurshistorie"
                value={`${s.prices.toLocaleString("de-DE")}`}
                sub={<span className="text-muted">seit {date(s.first_price_date)}</span>}
              />
            </div>
          </CardBody>
        </Card>
        <Card>
          <CardHeader
            title="Kursverlauf"
            actions={
              <Segmented
                value={range}
                onChange={setRange}
                options={RANGES.map((r) => ({ key: r.key, label: r.label }))}
              />
            }
          />
          {shown.length ? (
            <PriceChart prices={shown} currency={s.currency} markers={markers} className="h-80 px-2" />
          ) : (
            <CardBody className="text-sm text-muted">Keine Kurse vorhanden.</CardBody>
          )}
        </Card>
        <Card>
          <CardHeader title={`Buchungen (${data.transactions.length})`} />
          <div className="mt-2 overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border text-xs text-muted">
                  <th className="px-4 py-2 text-left font-medium sm:px-5">Datum</th>
                  <th className="px-3 py-2 text-left font-medium">Typ</th>
                  <th className="hidden px-3 py-2 text-left font-medium md:table-cell">Konto/Depot</th>
                  <th className="px-3 py-2 text-right font-medium">Stück</th>
                  <th className="px-4 py-2 text-right font-medium sm:px-5">Betrag</th>
                </tr>
              </thead>
              <tbody>
                {data.transactions.map((t) => (
                  <tr key={t.id} className="border-b border-border/60 last:border-0">
                    <td className="tabular px-4 py-2 sm:px-5">{date(t.date_time)}</td>
                    <td className="px-3 py-2">{TX_LABELS[t.kind] ?? t.kind}</td>
                    <td className="hidden px-3 py-2 text-muted md:table-cell">{t.owner_name}</td>
                    <td className="tabular px-3 py-2 text-right">{t.shares ? shares(t.shares) : ""}</td>
                    <td
                      className={cn(
                        "tabular px-4 py-2 text-right sm:px-5",
                        t.owner_kind === "account" && t.sign > 0 && "text-pos",
                      )}
                    >
                      {money(t.amount, t.currency)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>
        {data.events.length > 0 && (
          <Card>
            <CardHeader title="Ereignisse" />
            <ul className="mt-2 divide-y divide-border">
              {data.events.map((e, i) => (
                <li key={`${e.date}-${i}`} className="flex gap-4 px-4 py-2 text-sm sm:px-5">
                  <span className="tabular text-muted">{date(e.date)}</span>
                  <span>
                    {e.kind === "StockSplit" ? "Aktiensplit" : e.kind === "DividendPayment" ? "Dividende" : "Notiz"}
                  </span>
                  <span className="text-muted">{e.details}</span>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </>
  );
}
