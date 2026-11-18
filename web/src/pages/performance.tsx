import * as React from "react";
import { usePerformance } from "@/api/hooks";
import { BarsChart, ValueChart } from "@/components/charts/presets";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { ErrorBox, PageHeader, Segmented, Skeleton, Stat, Warnings } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { useFilter } from "@/lib/filter-context";
import { date, money, percent, tone } from "@/lib/format";
import { usePeriod } from "@/lib/period-context";
import { cn } from "@/lib/utils";
import { PeriodPicker } from "./dashboard";

const MONTHS = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];

function heat(v: number | undefined) {
  if (v == null) return undefined;
  const a = Math.min(1, Math.abs(v) / 0.08);
  return v >= 0
    ? `color-mix(in srgb, var(--pos) ${Math.round(a * 45)}%, transparent)`
    : `color-mix(in srgb, var(--neg) ${Math.round(a * 45)}%, transparent)`;
}

export function PerformancePage() {
  const clientId = useClientId();
  const { range } = usePeriod();
  const [mode, setMode] = React.useState<"return" | "value" | "drawdown">("return");
  const { data, error, isLoading } = usePerformance(clientId, range, undefined, useFilter().filter);

  const years = React.useMemo(() => {
    if (!data) return [];
    const ys = [...new Set(data.returns.map((r) => r.year))].sort((a, b) => b - a);
    return ys.map((y) => ({
      year: y,
      months: Array.from({ length: 12 }, (_, m) => data.returns.find((r) => r.year === y && r.month === m + 1)?.ttwror),
      total: data.returns.find((r) => r.year === y && r.month === 0)?.ttwror,
    }));
  }, [data]);

  if (error) return <ErrorBox error={error} />;
  const k = data?.kpis;
  return (
    <>
      <PageHeader
        title="Performance"
        subtitle={k ? `${date(k.from)} – ${date(k.to)}` : undefined}
        actions={<PeriodPicker />}
      />
      {isLoading || !data || !k ? (
        <Skeleton className="h-96" />
      ) : (
        <div className="space-y-4">
          <Warnings items={data.warnings.slice(0, 3)} />
          <Card>
            <CardBody>
              <div className="grid grid-cols-2 gap-5 sm:grid-cols-3 lg:grid-cols-6">
                <Stat
                  label="TTWROR"
                  value={percent(k.ttwror, { sign: true })}
                  valueClass={tone(k.ttwror)}
                  sub={<span className="text-muted">zeitgewichtet</span>}
                />
                <Stat
                  label="TTWROR p.a."
                  value={percent(k.ttwror_annualized, { sign: true })}
                  valueClass={tone(k.ttwror_annualized)}
                />
                <Stat
                  label="IZF / IRR"
                  value={percent(k.irr, { sign: true })}
                  valueClass={tone(k.irr)}
                  sub={<span className="text-muted">geldgewichtet p.a.</span>}
                />
                <Stat
                  label="Absolute Änderung"
                  value={money(k.absolute_delta, k.currency, { sign: true })}
                  valueClass={tone(k.absolute_delta)}
                />
                <Stat
                  label="Max. Drawdown"
                  value={percent(-k.max_drawdown)}
                  valueClass="text-neg"
                  sub={
                    <span className="text-muted">
                      {date(k.max_drawdown_start)} – {date(k.max_drawdown_end)}
                    </span>
                  }
                />
                <Stat
                  label="Volatilität p.a."
                  value={percent(k.volatility)}
                  sub={<span className="text-muted">Semi-Abw. {percent(k.semi_deviation)}</span>}
                />
              </div>
            </CardBody>
          </Card>
          <Card>
            <CardHeader
              title="Verlauf"
              actions={
                <Segmented
                  value={mode}
                  onChange={setMode}
                  options={[
                    { key: "return", label: "TTWROR" },
                    { key: "value", label: "Vermögen" },
                    { key: "drawdown", label: "Drawdown" },
                  ]}
                />
              }
            />
            <ValueChart series={data.series} currency={k.currency} mode={mode} className="h-72 px-2 sm:h-80" />
          </Card>
          <Card>
            <CardHeader title="Monatsrenditen" subtitle="TTWROR je Monat und Jahr" />
            <CardBody className="overflow-x-auto">
              <table className="w-full min-w-[640px] text-xs">
                <thead>
                  <tr className="text-muted">
                    <th className="py-1 text-left font-medium">Jahr</th>
                    {MONTHS.map((m) => (
                      <th key={m} className="py-1 text-right font-medium">
                        {m}
                      </th>
                    ))}
                    <th className="py-1 pl-2 text-right font-semibold">Jahr</th>
                  </tr>
                </thead>
                <tbody>
                  {years.map((y) => (
                    <tr key={y.year}>
                      <td className="py-0.5 font-medium">{y.year}</td>
                      {y.months.map((v, i) => (
                        <td key={MONTHS[i]} className="p-0.5">
                          <div className="tabular rounded px-1 py-1 text-right" style={{ background: heat(v) }}>
                            {v == null ? "" : percent(v, { digits: 1 })}
                          </div>
                        </td>
                      ))}
                      <td className="p-0.5 pl-2">
                        <div className={cn("tabular rounded px-1 py-1 text-right font-semibold", tone(y.total))}>
                          {percent(y.total, { digits: 1, sign: true })}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </CardBody>
          </Card>
          <div className="grid gap-4 lg:grid-cols-5">
            <Card className="lg:col-span-2">
              <CardHeader title="Jahresrenditen" />
              <BarsChart
                categories={years.map((y) => String(y.year)).reverse()}
                series={[{ name: "TTWROR", data: years.map((y) => y.total ?? 0).reverse() }]}
                percentMode
                className="h-64 px-2"
              />
            </Card>
            <Card className="lg:col-span-3">
              <CardHeader
                title="Berechnung"
                subtitle="Zusammensetzung der Wertänderung (wie in Portfolio Performance)"
              />
              <CardBody>
                <div className="grid gap-x-8 text-sm sm:grid-cols-2">
                  {[
                    ["Anfangswert", k.initial_value, false],
                    ["Einlagen", k.deposits, true],
                    ["Entnahmen", -k.removals, true],
                    ["Kursgewinne realisiert", k.realized_capital_gains, true],
                    ["Kursgewinne nicht realisiert", k.unrealized_capital_gains, true],
                    ["Dividenden (brutto)", k.dividends, true],
                    ["Zinsen (brutto)", k.interest, true],
                    ["Gebühren", -k.fees, true],
                    ["Steuern", -k.taxes, true],
                    ["Endwert", k.final_value, false],
                  ].map(([label, v, toned]) => (
                    <div key={label as string} className="flex justify-between border-b border-border/60 py-2">
                      <span className={toned ? "text-muted" : "font-medium"}>{label}</span>
                      <span className={cn("tabular", toned ? tone(v as number) : "font-semibold")}>
                        {money(v as number, k.currency, { sign: !!toned })}
                      </span>
                    </div>
                  ))}
                </div>
              </CardBody>
            </Card>
          </div>
        </div>
      )}
    </>
  );
}
