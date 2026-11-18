import { Pencil, PieChart, Plus, Trash2, X } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useAccounts, useInvalidateClient, useSecurities, useTaxonomies, useTaxonomy } from "@/api/hooks";
import { Donut } from "@/components/charts/presets";
import { FilterPicker } from "@/components/filter-picker";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { EmptyState, ErrorBox, PageHeader, Segmented, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { useFilter } from "@/lib/filter-context";
import { money, percent, tone } from "@/lib/format";
import { cn } from "@/lib/utils";

type Node = Schemas["TaxonomyNode"];

function EditDialog({
  node,
  taxonomyId,
  parentId,
  onClose,
}: {
  node?: Node;
  taxonomyId: string;
  parentId?: string;
  onClose: () => void;
}) {
  const clientId = useClientId();
  const { data: securities } = useSecurities(clientId);
  const { data: accounts } = useAccounts(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [name, setName] = React.useState(node?.name ?? "");
  const [color, setColor] = React.useState(node?.color ?? "#6366f1");
  const [weight, setWeight] = React.useState(String((node?.weight ?? 10000) / 100));
  const [assignments, setAssignments] = React.useState(
    node?.assignments.map((a) => ({ vehicle: a.vehicle_id, name: a.name, weight: a.weight })) ?? [],
  );
  const [add, setAdd] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const vehicles = [
    ...(securities ?? []).map((s) => ({ id: s.id, name: s.name })),
    ...(accounts ?? []).map((a) => ({ id: a.id, name: `${a.name} (Konto)` })),
  ];

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const body = {
      name,
      color,
      weight: Math.round(Number.parseFloat(weight.replace(",", ".")) * 100) || 0,
      parent_id: parentId ?? null,
      assignments: node ? assignments.map((a) => ({ vehicle: a.vehicle, weight: a.weight })) : null,
    };
    try {
      if (node)
        await unwrap(
          api.PUT("/clients/{id}/taxonomies/{tid}/classifications/{cid}", {
            params: { path: { id: clientId, tid: taxonomyId, cid: node.id } },
            body,
          }),
        );
      else
        await unwrap(
          api.POST("/clients/{id}/taxonomies/{tid}/classifications", {
            params: { path: { id: clientId, tid: taxonomyId } },
            body,
          }),
        );
      await invalidate();
      onClose();
    } catch (err) {
      setError(err);
    }
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={node ? "Kategorie bearbeiten" : "Neue Kategorie"}>
      <form onSubmit={submit} className="space-y-4">
        <div className="grid grid-cols-[1fr_auto_auto] gap-3">
          <Field label="Name">
            <Input value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
          </Field>
          <Field label="Soll %">
            <Input value={weight} onChange={(e) => setWeight(e.target.value)} className="w-20" inputMode="decimal" />
          </Field>
          <Field label="Farbe">
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              className="h-9 w-12 cursor-pointer rounded-lg border border-border"
            />
          </Field>
        </div>
        {node && (
          <div>
            <div className="mb-1.5 text-xs font-medium text-muted">Zugeordnete Wertpapiere und Konten</div>
            <ul className="divide-y divide-border rounded-lg border border-border">
              {assignments.map((a, i) => (
                <li key={a.vehicle} className="flex items-center gap-2 px-3 py-1.5 text-sm">
                  <span className="min-w-0 flex-1 truncate">
                    {a.name || vehicles.find((v) => v.id === a.vehicle)?.name}
                  </span>
                  <Input
                    className="h-7 w-16 text-right"
                    value={a.weight / 100}
                    onChange={(e) =>
                      setAssignments(
                        assignments.map((x, j) =>
                          j === i ? { ...x, weight: Math.round((Number(e.target.value) || 0) * 100) } : x,
                        ),
                      )
                    }
                  />
                  <span className="text-xs text-muted">%</span>
                  <button
                    type="button"
                    className="cursor-pointer p-1 text-muted"
                    onClick={() => setAssignments(assignments.filter((_, j) => j !== i))}
                    aria-label="Entfernen"
                  >
                    <X className="size-4" />
                  </button>
                </li>
              ))}
              {!assignments.length && <li className="px-3 py-2 text-xs text-muted">Keine Zuordnungen</li>}
            </ul>
            <div className="mt-2 flex gap-2">
              <Select value={add} onChange={(e) => setAdd(e.target.value)}>
                <option value="">Hinzufügen …</option>
                {vehicles
                  .filter((v) => !assignments.some((a) => a.vehicle === v.id))
                  .map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.name}
                    </option>
                  ))}
              </Select>
              <Button
                type="button"
                variant="outline"
                disabled={!add}
                onClick={() => {
                  setAssignments([
                    ...assignments,
                    { vehicle: add, name: vehicles.find((v) => v.id === add)?.name ?? "", weight: 10000 },
                  ]);
                  setAdd("");
                }}
              >
                <Plus />
              </Button>
            </div>
          </div>
        )}
        {error != null && <ErrorBox error={error} />}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            Abbrechen
          </Button>
          <Button type="submit">Speichern</Button>
        </div>
      </form>
    </Dialog>
  );
}

/** Target values: root target = actual total, children = parent target × weight. */
function targets(nodes: Node[]): Map<string, number> {
  const t = new Map<string, number>();
  for (const n of nodes) {
    if (!n.parent_id) t.set(n.id, n.value);
    else t.set(n.id, ((t.get(n.parent_id) ?? 0) * n.weight) / 10000);
  }
  return t;
}

export function TaxonomiesPage() {
  const clientId = useClientId();
  const { data: list, isLoading } = useTaxonomies(clientId);
  const [tid, setTid] = React.useState<string>();
  const active = tid ?? list?.[0]?.id;
  const { data, error } = useTaxonomy(clientId, active, useFilter().filter);
  const invalidate = useInvalidateClient(clientId);
  const [expanded, setExpanded] = React.useState<Record<string, boolean>>({});
  const [editing, setEditing] = React.useState<{ node?: Node; parent?: string } | null>(null);
  const [mode, setMode] = React.useState<"structure" | "rebalance">("structure");

  if (isLoading) return <Skeleton className="h-96" />;
  if (!list?.length)
    return (
      <>
        <PageHeader title="Klassifizierungen" />
        <Card>
          <EmptyState icon={<PieChart className="size-5" />} title="Keine Klassifizierungen">
            Klassifizierungen werden aus Portfolio Performance übernommen.
          </EmptyState>
        </Card>
      </>
    );

  const level1 = data?.nodes.filter((n) => n.depth === 1 && n.value !== 0) ?? [];
  const tgt = data ? targets(data.nodes) : new Map();
  const root = data?.nodes.find((n) => !n.parent_id);
  const weightSum = (parent: string) =>
    data?.nodes.filter((n) => n.parent_id === parent).reduce((s, n) => s + n.weight, 0) ?? 0;

  const remove = async (n: Node) => {
    if (!active || !confirm(`Kategorie „${n.name}“ löschen?`)) return;
    try {
      await unwrap(
        api.DELETE("/clients/{id}/taxonomies/{tid}/classifications/{cid}", {
          params: { path: { id: clientId, tid: active, cid: n.id } },
        }),
      );
      invalidate();
    } catch (e) {
      alert((e as Error).message);
    }
  };

  return (
    <>
      <PageHeader
        title="Klassifizierungen"
        actions={
          <>
            <FilterPicker />
            {list.length > 1 && (
              <Segmented
                value={active ?? ""}
                onChange={setTid}
                options={list.map((t) => ({ key: t.id, label: t.name }))}
              />
            )}
          </>
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
              title={mode === "structure" ? "Struktur" : "Rebalancing"}
              subtitle={
                mode === "structure"
                  ? "Ist-Anteil am Vermögen"
                  : "Soll-Wert aus den Gewichten, Differenz = Kauf (+) bzw. Verkauf (−)"
              }
              actions={
                <>
                  <Segmented
                    value={mode}
                    onChange={setMode}
                    options={[
                      { key: "structure", label: "Struktur" },
                      { key: "rebalance", label: "Rebalancing" },
                    ]}
                  />
                  <Button size="sm" variant="outline" onClick={() => setEditing({ parent: root?.id })}>
                    <Plus />
                  </Button>
                </>
              }
            />
            <div className="mt-2 divide-y divide-border">
              {data.nodes
                .filter((n) => n.depth > 0)
                .map((n) => {
                  const target = tgt.get(n.id) ?? 0;
                  const delta = target - n.value;
                  const sumWarn = n.parent_id && weightSum(n.parent_id) !== 10000;
                  return (
                    <div key={n.id} className="group">
                      <div
                        className="flex w-full items-center gap-3 py-2.5 pr-4 text-sm hover:bg-surface-2/60 sm:pr-5"
                        style={{ paddingLeft: `${(n.depth - 1) * 20 + 16}px` }}
                      >
                        <button
                          type="button"
                          onClick={() => setExpanded((e) => ({ ...e, [n.id]: !e[n.id] }))}
                          className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 text-left"
                        >
                          <span
                            className="size-2.5 shrink-0 rounded-full"
                            style={{ background: n.color ?? "#94a3b8" }}
                          />
                          <span className={cn("min-w-0 flex-1 truncate", n.depth === 1 && "font-medium")}>
                            {n.name}
                          </span>
                        </button>
                        {mode === "structure" ? (
                          <>
                            <span className="tabular w-16 text-right text-muted">
                              {percent(n.share, { digits: 1 })}
                            </span>
                            <span className="tabular w-28 text-right">{money(n.value)}</span>
                          </>
                        ) : (
                          <>
                            <span
                              className={cn(
                                "tabular hidden w-16 text-right text-xs sm:block",
                                sumWarn ? "text-warn" : "text-muted",
                              )}
                              title={sumWarn ? "Gewichte ergeben nicht 100 %" : undefined}
                            >
                              {percent(n.weight / 10000, { digits: 1 })}
                            </span>
                            <span className="tabular hidden w-24 text-right text-muted sm:block">
                              {money(Math.round(target), "EUR", { compact: true })}
                            </span>
                            <span className="tabular w-24 text-right">{money(n.value, "EUR", { compact: true })}</span>
                            <span className={cn("tabular w-24 text-right font-medium", tone(delta))}>
                              {money(Math.round(delta), "EUR", { sign: true, compact: true })}
                            </span>
                          </>
                        )}
                        <span className="flex gap-0.5 sm:opacity-0 sm:group-hover:opacity-100">
                          <button
                            type="button"
                            className="cursor-pointer p-1 text-muted"
                            aria-label="Bearbeiten"
                            onClick={() => setEditing({ node: n })}
                          >
                            <Pencil className="size-3.5" />
                          </button>
                          <button
                            type="button"
                            className="cursor-pointer p-1 text-muted"
                            aria-label="Unterkategorie"
                            onClick={() => setEditing({ parent: n.id })}
                          >
                            <Plus className="size-3.5" />
                          </button>
                          <button
                            type="button"
                            className="cursor-pointer p-1 text-muted"
                            aria-label="Löschen"
                            onClick={() => remove(n)}
                          >
                            <Trash2 className="size-3.5" />
                          </button>
                        </span>
                      </div>
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
                  );
                })}
            </div>
          </Card>
        </div>
      )}
      {editing && active && (
        <EditDialog
          node={editing.node}
          parentId={editing.parent}
          taxonomyId={active}
          onClose={() => setEditing(null)}
        />
      )}
    </>
  );
}
