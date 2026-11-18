import * as React from "react";
import { useEarnings } from "@/api/hooks";
import { BarsChart, Donut } from "@/components/charts/presets";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { ErrorBox, PageHeader, Segmented, Skeleton, Stat } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { useFilter } from "@/lib/filter-context";
import { date, money, percent, TX_LABELS } from "@/lib/format";
import { usePeriod } from "@/lib/period-context";
import { PeriodPicker } from "./dashboard";

const MONTHS = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];

export function EarningsPage() {
  const clientId = useClientId();
  const { range } = usePeriod();
  const { data, error, isLoading } = useEarnings(clientId, range, useFilter().filter);
  const [view, setView] = React.useState<"month" | "year">("month");

  const chart = React.useMemo(() => {
    if (!data) return { categories: [], div: [], int: [] };
    if (view === "month") {
      return {
        categories: data.by_month.map((b) => `${MONTHS[b.month - 1]} ${String(b.year).slice(2)}`),
        div: data.by_month.map((b) => b.dividends),
        int: data.by_month.map((b) => b.interest),
      };
    }
    const years = [...new Set(data.by_month.map((b) => b.year))];
    return {
      categories: years.map(String),
      div: years.map((y) => data.by_month.filter((b) => b.year === y).reduce((s, b) => s + b.dividends, 0)),
      int: years.map((y) => data.by_month.filter((b) => b.year === y).reduce((s, b) => s + b.interest, 0)),
    };
  }, [data, view]);

  if (error) return <ErrorBox error={error} />;
  return (
    <>
      <PageHeader title="Erträge" subtitle="Dividenden und Zinsen (netto nach Steuern)" actions={<PeriodPicker />} />
      {isLoading || !data ? (
        <Skeleton className="h-96" />
      ) : (
        <div className="space-y-4">
          <Card>
            <CardBody>
              <div className="grid grid-cols-2 gap-5 sm:grid-cols-4">
                <Stat label="Erträge netto" value={money(data.total, data.currency)} />
                <Stat label="Erträge brutto" value={money(data.total_gross, data.currency)} />
                <Stat label="Steuern" value={money(data.total_taxes, data.currency)} />
                <Stat
                  label="Ø pro Monat"
                  value={money(Math.round(data.total / Math.max(1, data.by_month.length)), data.currency)}
                />
              </div>
            </CardBody>
          </Card>
          <Card>
            <CardHeader
              title="Verlauf"
              actions={
                <Segmented
                  value={view}
                  onChange={setView}
                  options={[
                    { key: "month", label: "Monate" },
                    { key: "year", label: "Jahre" },
                  ]}
                />
              }
            />
            <BarsChart
              categories={chart.categories}
              series={[
                { name: "Dividenden", data: chart.div, color: "#10b981" },
                { name: "Zinsen", data: chart.int, color: "#6366f1" },
              ]}
              currency={data.currency}
              stacked
              className="h-72 px-2"
            />
          </Card>
          <div className="grid gap-4 lg:grid-cols-5">
            <Card className="lg:col-span-2">
              <CardHeader title="Nach Wertpapier" />
              <CardBody className="pt-0">
                <Donut data={data.by_security} currency={data.currency} className="h-48" />
                <ul className="mt-2 space-y-1.5 text-sm">
                  {data.by_security.slice(0, 10).map((s) => (
                    <li key={s.id} className="flex justify-between gap-3">
                      <span className="truncate">{s.name}</span>
                      <span className="tabular shrink-0 text-muted">
                        {money(s.value, data.currency)} · {percent(s.share, { digits: 1 })}
                      </span>
                    </li>
                  ))}
                </ul>
              </CardBody>
            </Card>
            <Card className="lg:col-span-3">
              <CardHeader title={`Zahlungen (${data.entries.length})`} />
              <div className="mt-2 max-h-[480px] overflow-y-auto">
                <table className="w-full text-sm">
                  <thead className="sticky top-0 bg-surface">
                    <tr className="border-b border-border text-xs text-muted">
                      <th className="px-4 py-2 text-left font-medium sm:px-5">Datum</th>
                      <th className="px-3 py-2 text-left font-medium">Wertpapier</th>
                      <th className="hidden px-3 py-2 text-right font-medium sm:table-cell">Brutto</th>
                      <th className="hidden px-3 py-2 text-right font-medium sm:table-cell">Steuern</th>
                      <th className="px-4 py-2 text-right font-medium sm:px-5">Netto</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.entries.map((e, i) => (
                      <tr key={`${e.date}-${i}`} className="border-b border-border/60">
                        <td className="tabular px-4 py-2 sm:px-5">{date(e.date)}</td>
                        <td className="max-w-56 truncate px-3 py-2">
                          {e.name}
                          <span className="ml-1 text-xs text-muted">
                            {e.kind === "INTEREST" ? TX_LABELS.INTEREST : ""}
                          </span>
                        </td>
                        <td className="tabular hidden px-3 py-2 text-right text-muted sm:table-cell">
                          {money(e.gross, data.currency)}
                        </td>
                        <td className="tabular hidden px-3 py-2 text-right text-muted sm:table-cell">
                          {money(e.taxes, data.currency)}
                        </td>
                        <td className="tabular px-4 py-2 text-right font-medium text-pos sm:px-5">
                          {money(e.amount, data.currency)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Card>
          </div>
        </div>
      )}
    </>
  );
}
