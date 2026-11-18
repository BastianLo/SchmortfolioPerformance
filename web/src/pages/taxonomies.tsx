import * as React from "react";
import { useTaxonomies, useTaxonomy } from "@/api/hooks";
import { Donut } from "@/components/charts/presets";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { EmptyState, ErrorBox, PageHeader, Segmented, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { money, percent } from "@/lib/format";
import { cn } from "@/lib/utils";

export function TaxonomiesPage() {
  const clientId = useClientId();
  const { data: list, isLoading } = useTaxonomies(clientId);
  const [tid, setTid] = React.useState<string>();
  const active = tid ?? list?.[0]?.id;
  const { data, error } = useTaxonomy(clientId, active);
  const [expanded, setExpanded] = React.useState<Record<string, boolean>>({});

  if (isLoading) return <Skeleton className="h-96" />;
  if (!list?.length)
    return (
      <>
        <PageHeader title="Klassifizierungen" />
        <Card>
          <EmptyState title="Keine Klassifizierungen">
            Klassifizierungen werden aus Portfolio Performance übernommen.
          </EmptyState>
        </Card>
      </>
    );

  const level1 = data?.nodes.filter((n) => n.depth === 1 && n.value !== 0) ?? [];
  return (
    <>
      <PageHeader
        title="Klassifizierungen"
        actions={
          list.length > 1 && (
            <Segmented
              value={active ?? ""}
              onChange={setTid}
              options={list.map((t) => ({ key: t.id, label: t.name }))}
            />
          )
        }
      />
      {error ? (
        <ErrorBox error={error} />
      ) : !data ? (
        <Skeleton className="h-96" />
      ) : (
        <div className="grid gap-4 lg:grid-cols-5">
          <Card className="lg:col-span-2">
            <CardHeader title={data.name} subtitle={`Gesamt ${money(data.total)}`} />
            <CardBody>
              <Donut
                data={level1.map((n) => ({ name: n.name, value: n.value, color: n.color }))}
                currency="EUR"
                className="h-64"
              />
            </CardBody>
          </Card>
          <Card className="lg:col-span-3">
            <CardHeader
              title="Struktur"
              subtitle="Ist-Anteil am Gesamtvermögen und Soll-Gewichtung innerhalb der Ebene"
            />
            <div className="mt-2 divide-y divide-border">
              {data.nodes
                .filter((n) => n.depth > 0)
                .map((n) => (
                  <div key={n.id}>
                    <button
                      type="button"
                      onClick={() => setExpanded((e) => ({ ...e, [n.id]: !e[n.id] }))}
                      className="flex w-full cursor-pointer items-center gap-3 px-4 py-2.5 text-left text-sm hover:bg-surface-2/60 sm:px-5"
                      style={{ paddingLeft: `${(n.depth - 1) * 20 + 16}px` }}
                    >
                      <span className="size-2.5 shrink-0 rounded-full" style={{ background: n.color ?? "#94a3b8" }} />
                      <span className={cn("min-w-0 flex-1 truncate", n.depth === 1 && "font-medium")}>{n.name}</span>
                      <span className="tabular hidden w-20 text-right text-xs text-muted sm:block">
                        Soll {percent(n.weight / 10000, { digits: 0 })}
                      </span>
                      <span className="tabular w-16 text-right text-muted">{percent(n.share, { digits: 1 })}</span>
                      <span className="tabular w-28 text-right">{money(n.value)}</span>
                    </button>
                    {expanded[n.id] && n.assignments.length > 0 && (
                      <ul className="bg-surface-2/40 pb-1">
                        {n.assignments.map((a) => (
                          <li
                            key={a.vehicle_id}
                            className="flex justify-between gap-3 py-1.5 pr-4 text-xs sm:pr-5"
                            style={{ paddingLeft: `${n.depth * 20 + 30}px` }}
                          >
                            <span className="truncate text-muted">
                              {a.name}
                              {a.weight !== 10000 && ` (${percent(a.weight / 10000, { digits: 0 })})`}
                            </span>
                            <span className="tabular">{money(a.value)}</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                ))}
            </div>
          </Card>
        </div>
      )}
    </>
  );
}
