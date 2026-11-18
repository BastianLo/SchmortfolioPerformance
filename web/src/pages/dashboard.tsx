import { Link } from "@tanstack/react-router";
import { ArrowDownRight, ArrowUpRight, Coins } from "lucide-react";
import * as React from "react";
import { useClient, useDashboard, useTaxonomies } from "@/api/hooks";
import { Donut, ValueChart } from "@/components/charts/presets";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/input";
import { ErrorBox, PageHeader, Segmented, Skeleton, Stat, Warnings } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, money, percent, TX_LABELS, tone } from "@/lib/format";
import { PERIODS } from "@/lib/period";
import { usePeriod } from "@/lib/period-context";
import { cn } from "@/lib/utils";

export function PeriodPicker() {
  const { period, setPeriod } = usePeriod();
  return <Segmented value={period} onChange={setPeriod} options={PERIODS} />;
}

function DashboardSkeleton() {
  return (
    <div className="grid gap-4 lg:grid-cols-3">
      <Skeleton className="h-80 lg:col-span-2" />
      <Skeleton className="h-80" />
      <Skeleton className="h-40 lg:col-span-3" />
    </div>
  );
}

export function DashboardPage() {
  const clientId = useClientId();
  const { range, period } = usePeriod();
  const [taxonomy, setTaxonomy] = React.useState<string | undefined>();
  const [chartMode, setChartMode] = React.useState<"value" | "return" | "drawdown">("value");
  const { data: client } = useClient(clientId);
  const { data: taxonomies } = useTaxonomies(clientId);
  const { data, error, isLoading, isFetching } = useDashboard(clientId, range, taxonomy);
  const periodLabel = PERIODS.find((p) => p.key === period)?.label;

  if (error) return <ErrorBox error={error} />;
  if (isLoading || !data) {
    return (
      <>
        <PageHeader title={client?.name ?? "Übersicht"} actions={<PeriodPicker />} />
        <DashboardSkeleton />
      </>
    );
  }
  const k = data.kpis;
  const cur = data.currency;
  const empty = (client?.counts.transactions ?? 0) === 0;

  return (
    <>
      <PageHeader
        title={client?.name ?? "Übersicht"}
        subtitle={`Stand ${date(data.date)}`}
        actions={<PeriodPicker />}
      />
      <div className="space-y-4">
        <Warnings
          items={[
            ...(client?.missing_exchange_rates.length
              ? [
                  `Fehlende Wechselkurse: ${client.missing_exchange_rates.join(", ")} – Kurse werden aus Buchungen abgeleitet oder 1:1 umgerechnet.`,
                ]
              : []),
            ...data.warnings
              .filter((w) => !(client?.missing_exchange_rates.length && w.includes("Wechselkurs")))
              .slice(0, 3),
          ]}
        />
        {empty && (
          <Card>
            <CardBody className="text-sm text-muted">
              Dieses Portfolio ist noch leer.{" "}
              <Link to="/c/$clientId/accounts" params={{ clientId }} className="font-medium text-accent">
                Konten und Depots anlegen
              </Link>{" "}
              oder eine PP-Datei importieren.
            </CardBody>
          </Card>
        )}
        <div className="grid gap-4 lg:grid-cols-3">
          <Card className={cn("lg:col-span-2", isFetching && "opacity-80 transition-opacity")}>
            <CardBody className="pb-2">
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div>
                  <div className="text-xs font-medium text-muted">Gesamtvermögen</div>
                  <div className="tabular mt-1 text-3xl font-semibold tracking-tight sm:text-4xl">
                    {money(data.total, cur)}
                  </div>
                  <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
                    <span className={cn("tabular inline-flex items-center gap-1 font-medium", tone(data.day_change))}>
                      {data.day_change >= 0 ? (
                        <ArrowUpRight className="size-4" />
                      ) : (
                        <ArrowDownRight className="size-4" />
                      )}
                      {money(data.day_change, cur, { sign: true })} ({percent(data.day_change_percent, { sign: true })})
                      <span className="font-normal text-muted">heute</span>
                    </span>
                    <span className={cn("tabular font-medium", tone(k.absolute_delta))}>
                      {money(k.absolute_delta, cur, { sign: true })}
                      <span className="ml-1 font-normal text-muted">{periodLabel}</span>
                    </span>
                  </div>
                </div>
                <Segmented
                  value={chartMode}
                  onChange={setChartMode}
                  options={[
                    { key: "value", label: "Vermögen" },
                    { key: "return", label: "Rendite" },
                    { key: "drawdown", label: "Drawdown" },
                  ]}
                />
              </div>
            </CardBody>
            <ValueChart series={data.series} currency={cur} mode={chartMode} className="h-64 px-2 sm:h-72" />
          </Card>

          <Card>
            <CardHeader
              title="Vermögensaufteilung"
              actions={
                taxonomies && taxonomies.length > 0 ? (
                  <Select
                    value={data.taxonomy_id ?? ""}
                    onChange={(e) => setTaxonomy(e.target.value || undefined)}
                    className="h-8 w-40 text-xs"
                  >
                    {taxonomies.map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.name}
                      </option>
                    ))}
                  </Select>
                ) : undefined
              }
            />
            <CardBody className="pt-0">
              <Donut data={data.allocation} currency={cur} className="h-48" />
              <ul className="mt-2 space-y-1.5">
                {data.allocation.map((s, i) => (
                  <li key={s.id} className="flex items-center gap-2 text-sm">
                    <span
                      className="size-2.5 shrink-0 rounded-full"
                      style={{ background: s.color ?? ["#6366f1", "#10b981", "#f59e0b", "#ef4444", "#0ea5e9"][i % 5] }}
                    />
                    <span className="min-w-0 flex-1 truncate">{s.name}</span>
                    <span className="tabular text-muted">{percent(s.share, { digits: 1 })}</span>
                    <span className="tabular w-24 text-right">{money(s.value, cur, { compact: true })}</span>
                  </li>
                ))}
              </ul>
            </CardBody>
          </Card>
        </div>

        <Card>
          <CardBody>
            <div className="grid grid-cols-2 gap-x-6 gap-y-5 sm:grid-cols-4 2xl:grid-cols-8">
              <Stat
                label={`TTWROR ${periodLabel}`}
                value={percent(k.ttwror, { sign: true })}
                valueClass={tone(k.ttwror)}
              />
              <Stat
                label="TTWROR p.a."
                value={percent(k.ttwror_annualized, { sign: true })}
                valueClass={tone(k.ttwror_annualized)}
              />
              <Stat label="IZF (IRR) p.a." value={percent(k.irr, { sign: true })} valueClass={tone(k.irr)} />
              <Stat
                label="Max. Drawdown"
                value={percent(-k.max_drawdown)}
                valueClass={k.max_drawdown > 0 ? "text-neg" : undefined}
              />
              <Stat label="Volatilität p.a." value={percent(k.volatility)} />
              <Stat
                label="Kursgewinne"
                value={money(k.capital_gains, cur, { compact: true, sign: true })}
                valueClass={tone(k.capital_gains)}
              />
              <Stat label="Erträge 12 Mon." value={money(data.earnings_last_12_months, cur, { compact: true })} />
              <Stat label="Gebühren + Steuern" value={money(k.fees + k.taxes, cur, { compact: true })} />
            </div>
          </CardBody>
        </Card>

        <div className="grid gap-4 lg:grid-cols-3">
          <Card className="lg:col-span-2">
            <CardHeader
              title="Größte Positionen"
              actions={
                <Link to="/c/$clientId/holdings" params={{ clientId }} className="text-xs font-medium text-accent">
                  Alle anzeigen
                </Link>
              }
            />
            <div className="mt-2 divide-y divide-border">
              {data.top_positions.map((p) => (
                <Link
                  key={p.security_id}
                  to="/c/$clientId/securities/$securityId"
                  params={{ clientId, securityId: p.security_id }}
                  className="flex items-center gap-3 px-4 py-3 hover:bg-surface-2/60 sm:px-5"
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{p.name}</div>
                    <div className="mt-1 h-1.5 w-full max-w-48 overflow-hidden rounded-full bg-surface-2">
                      <div
                        className="h-full rounded-full bg-accent"
                        style={{ width: `${Math.min(100, p.share * 100 * 2)}%` }}
                      />
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="tabular text-sm font-medium">{money(p.market_value, cur)}</div>
                    <div className={cn("tabular text-xs", tone(p.gain))}>
                      {money(p.gain, cur, { sign: true })} · {percent(p.gain_percent, { sign: true })}
                    </div>
                  </div>
                </Link>
              ))}
              {data.top_positions.length === 0 && (
                <div className="px-5 py-6 text-sm text-muted">Keine Positionen im Bestand.</div>
              )}
            </div>
          </Card>

          <Card>
            <CardHeader
              title="Letzte Erträge"
              actions={
                <Link to="/c/$clientId/earnings" params={{ clientId }} className="text-xs font-medium text-accent">
                  Details
                </Link>
              }
            />
            <ul className="mt-2 divide-y divide-border">
              {data.recent_earnings.map((e, i) => (
                <li key={`${e.date}-${i}`} className="flex items-center gap-3 px-4 py-2.5 sm:px-5">
                  <div className="rounded-lg bg-pos/10 p-1.5 text-pos">
                    <Coins className="size-4" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm">{e.name}</div>
                    <div className="text-xs text-muted">
                      {TX_LABELS[e.kind] ?? e.kind} · {date(e.date)}
                    </div>
                  </div>
                  <div className="tabular text-sm font-medium text-pos">{money(e.amount, cur)}</div>
                </li>
              ))}
              {data.recent_earnings.length === 0 && (
                <li className="px-5 py-6 text-sm text-muted">Noch keine Erträge.</li>
              )}
            </ul>
          </Card>
        </div>

        <Card>
          <CardHeader title={`Entwicklung im Zeitraum (${periodLabel})`} subtitle={`${date(k.from)} – ${date(k.to)}`} />
          <CardBody>
            <div className="grid gap-x-8 gap-y-2 text-sm sm:grid-cols-2 lg:grid-cols-4">
              {[
                ["Anfangswert", k.initial_value],
                ["Einlagen − Entnahmen", k.transfers],
                ["Kursgewinne (realisiert)", k.realized_capital_gains],
                ["Kursgewinne (nicht realisiert)", k.unrealized_capital_gains],
                ["Erträge (brutto)", k.earnings],
                ["Gebühren", -k.fees],
                ["Steuern", -k.taxes],
                ["Endwert", k.final_value],
              ].map(([label, v]) => (
                <div key={label as string} className="flex justify-between gap-3 border-b border-border/60 py-1.5">
                  <span className="text-muted">{label}</span>
                  <span
                    className={cn(
                      "tabular font-medium",
                      label !== "Anfangswert" && label !== "Endwert" && tone(v as number),
                    )}
                  >
                    {money(v as number, cur)}
                  </span>
                </div>
              ))}
            </div>
          </CardBody>
        </Card>
      </div>
    </>
  );
}
