import { CalendarClock, Pencil, Play, Plus, Trash2 } from "lucide-react";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useAccounts, useInvalidateClient, usePlans, usePortfolios, useSecurities } from "@/api/hooks";
import { fmtNum, parseNum } from "@/components/booking-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { Badge, EmptyState, ErrorBox, PageHeader, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { AMOUNT, date, isoDate, money } from "@/lib/format";

type Plan = Schemas["PlanDto"];
type PlanType = Schemas["PlanType"];

const TYPES: { key: PlanType; label: string }[] = [
  { key: "PURCHASE_OR_DELIVERY", label: "Wertpapierkauf" },
  { key: "DEPOSIT", label: "Einlage" },
  { key: "REMOVAL", label: "Entnahme" },
  { key: "INTEREST", label: "Zinsen" },
];

export function intervalLabel(i: number) {
  if (i > 100) return i - 100 === 1 ? "wöchentlich" : `alle ${i - 100} Wochen`;
  return (
    (
      { 1: "monatlich", 2: "alle 2 Monate", 3: "vierteljährlich", 6: "halbjährlich", 12: "jährlich" } as Record<
        number,
        string
      >
    )[i] ?? `alle ${i} Monate`
  );
}

function PlanDialog({ open, onOpenChange, plan }: { open: boolean; onOpenChange: (o: boolean) => void; plan?: Plan }) {
  const clientId = useClientId();
  const { data: securities } = useSecurities(clientId);
  const { data: accounts } = useAccounts(clientId);
  const { data: portfolios } = usePortfolios(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [f, setF] = React.useState(() => ({
    name: plan?.name ?? "",
    kind: (plan?.kind ?? "PURCHASE_OR_DELIVERY") as PlanType,
    security: plan?.security ?? "",
    portfolio: plan?.portfolio ?? portfolios?.[0]?.id ?? "",
    account: plan?.account ?? "",
    start: plan?.start.slice(0, 10) ?? isoDate(new Date()),
    interval: String(plan?.interval ?? 1),
    amount: plan ? fmtNum(plan.amount / AMOUNT) : "",
    fees: plan ? fmtNum(plan.fees / AMOUNT) : "",
    auto: plan?.auto_generate ?? true,
  }));
  const [error, setError] = React.useState<unknown>(null);
  const set = (k: keyof typeof f) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setF({ ...f, [k]: e.target.type === "checkbox" ? (e.target as HTMLInputElement).checked : e.target.value });
  React.useEffect(() => {
    if (!f.portfolio && portfolios?.[0]) setF((x) => ({ ...x, portfolio: portfolios[0].id }));
  }, [portfolios, f.portfolio]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const purchase = f.kind === "PURCHASE_OR_DELIVERY";
    const body: Schemas["PlanInput"] = {
      name: f.name,
      kind: f.kind,
      security: purchase ? f.security : null,
      portfolio: purchase ? f.portfolio : null,
      account: f.account || null,
      auto_generate: f.auto,
      start: `${f.start}T00:00:00`,
      interval: Number(f.interval),
      amount: Math.round(parseNum(f.amount) * AMOUNT),
      fees: Math.round(parseNum(f.fees) * AMOUNT),
    };
    try {
      if (plan)
        await unwrap(api.PUT("/clients/{id}/plans/{pid}", { params: { path: { id: clientId, pid: plan.id } }, body }));
      else await unwrap(api.POST("/clients/{id}/plans", { params: { path: { id: clientId } }, body }));
      await invalidate();
      onOpenChange(false);
    } catch (err) {
      setError(err);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={plan ? "Sparplan bearbeiten" : "Neuer Sparplan"}>
      <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <div className="sm:col-span-2">
          <Field label="Name">
            <Input value={f.name} onChange={set("name")} required autoFocus />
          </Field>
        </div>
        <Field label="Art">
          <Select value={f.kind} onChange={set("kind")}>
            {TYPES.map((t) => (
              <option key={t.key} value={t.key}>
                {t.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Intervall">
          <Select value={f.interval} onChange={set("interval")}>
            {[1, 2, 3, 6, 12, 101, 102, 104].map((i) => (
              <option key={i} value={i}>
                {intervalLabel(i)}
              </option>
            ))}
          </Select>
        </Field>
        {f.kind === "PURCHASE_OR_DELIVERY" && (
          <>
            <div className="sm:col-span-2">
              <Field label="Wertpapier">
                <Select value={f.security} onChange={set("security")} required>
                  <option value="">Bitte wählen …</option>
                  {securities
                    ?.filter((s) => !s.retired)
                    .map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                </Select>
              </Field>
            </div>
            <Field label="Depot">
              <Select value={f.portfolio} onChange={set("portfolio")}>
                {portfolios?.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </Select>
            </Field>
          </>
        )}
        <Field
          label="Konto"
          hint={f.kind === "PURCHASE_OR_DELIVERY" ? "Ohne Konto werden Einlieferungen gebucht" : undefined}
        >
          <Select value={f.account} onChange={set("account")} required={f.kind !== "PURCHASE_OR_DELIVERY"}>
            {f.kind === "PURCHASE_OR_DELIVERY" && <option value="">– ohne (Einlieferung) –</option>}
            {accounts?.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Start">
          <Input type="date" value={f.start} onChange={set("start")} required />
        </Field>
        <Field label="Betrag">
          <Input inputMode="decimal" value={f.amount} onChange={set("amount")} required />
        </Field>
        <Field label="Gebühren">
          <Input inputMode="decimal" value={f.fees} onChange={set("fees")} placeholder="0,00" />
        </Field>
        <label className="flex items-center gap-2 text-sm sm:col-span-2">
          <input type="checkbox" checked={f.auto} onChange={set("auto")} className="size-4" />
          Fällige Ausführungen automatisch buchen
        </label>
        {error != null && (
          <div className="sm:col-span-2">
            <ErrorBox error={error} />
          </div>
        )}
        <div className="flex justify-end gap-2 sm:col-span-2">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Abbrechen
          </Button>
          <Button type="submit">Speichern</Button>
        </div>
      </form>
    </Dialog>
  );
}

export function PlansPage() {
  const clientId = useClientId();
  const { data, isLoading } = usePlans(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [edit, setEdit] = React.useState<Plan | "new" | null>(null);
  const [msg, setMsg] = React.useState<string | null>(null);
  const today = isoDate(new Date());

  const run = async (p: Plan) => {
    const r = await unwrap(
      api.POST("/clients/{id}/plans/{pid}/execute", { params: { path: { id: clientId, pid: p.id } } }),
    );
    setMsg(r.errors.length ? r.errors.join("; ") : `${r.created} Ausführungen gebucht`);
    invalidate();
  };
  const remove = async (p: Plan) => {
    if (!confirm(`Sparplan „${p.name}“ löschen? Bereits gebuchte Ausführungen bleiben erhalten.`)) return;
    await unwrap(api.DELETE("/clients/{id}/plans/{pid}", { params: { path: { id: clientId, pid: p.id } } }));
    invalidate();
  };

  return (
    <>
      <PageHeader
        title="Sparpläne"
        subtitle="Regelmäßige Käufe, Einlagen und Entnahmen"
        actions={
          <Button onClick={() => setEdit("new")}>
            <Plus /> Sparplan
          </Button>
        }
      />
      {msg && <p className="mb-3 text-sm text-muted">{msg}</p>}
      {isLoading ? (
        <Skeleton className="h-60" />
      ) : !data?.length ? (
        <Card>
          <EmptyState icon={<CalendarClock className="size-5" />} title="Noch keine Sparpläne">
            Lege einen Sparplan an – fällige Ausführungen werden automatisch gebucht.
          </EmptyState>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {data.map((p) => {
            const due = p.next_date <= today;
            return (
              <Card key={p.id} className="p-4 sm:p-5">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="truncate font-medium">{p.name}</div>
                    <div className="truncate text-sm text-muted">
                      {TYPES.find((t) => t.key === p.kind)?.label}
                      {p.security_name ? ` · ${p.security_name}` : ""}
                    </div>
                  </div>
                  <div className="tabular text-right font-semibold">{money(p.amount)}</div>
                </div>
                <div className="mt-3 flex flex-wrap gap-1.5 text-xs">
                  <Badge>{intervalLabel(p.interval)}</Badge>
                  <Badge>seit {date(p.start)}</Badge>
                  <Badge>{p.executions} Ausführungen</Badge>
                  {p.auto_generate && <Badge className="bg-accent/15 text-accent">automatisch</Badge>}
                  {p.account_name && <Badge>{p.account_name}</Badge>}
                </div>
                <div className="mt-4 flex items-center justify-between gap-2">
                  <span className={due ? "text-sm font-medium text-warn" : "text-sm text-muted"}>
                    {due ? "Fällig seit" : "Nächste Ausführung"} {date(p.next_date)}
                  </span>
                  <div className="flex gap-1">
                    {due && (
                      <Button size="sm" variant="outline" onClick={() => run(p)}>
                        <Play /> Buchen
                      </Button>
                    )}
                    <Button size="icon" variant="ghost" onClick={() => setEdit(p)} aria-label="Bearbeiten">
                      <Pencil />
                    </Button>
                    <Button size="icon" variant="ghost" onClick={() => remove(p)} aria-label="Löschen">
                      <Trash2 />
                    </Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}
      {edit && <PlanDialog open plan={edit === "new" ? undefined : edit} onOpenChange={(o) => !o && setEdit(null)} />}
    </>
  );
}
