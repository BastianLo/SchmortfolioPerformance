import { Link } from "@tanstack/react-router";
import { Eye, Plus, Trash2, X } from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { useInvalidateClient, useSecurities, useWatchlists } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input, Select } from "@/components/ui/input";
import { EmptyState, PageHeader, Skeleton } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date, percent, quote, tone } from "@/lib/format";

export function WatchlistsPage() {
  const clientId = useClientId();
  const { data: lists, isLoading } = useWatchlists(clientId);
  const { data: securities } = useSecurities(clientId);
  const invalidate = useInvalidateClient(clientId);
  const [name, setName] = React.useState("");
  const [adding, setAdding] = React.useState<Record<number, string>>({});

  const save = async (next: { name: string; securities: string[] }[]) => {
    await unwrap(api.PUT("/clients/{id}/watchlists", { params: { path: { id: clientId } }, body: next }));
    invalidate();
  };
  const all = lists ?? [];

  return (
    <>
      <PageHeader
        title="Watchlists"
        subtitle="Wertpapiere beobachten"
        actions={
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) save([...all, { name, securities: [] }]).then(() => setName(""));
            }}
          >
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Neue Watchlist"
              className="w-48"
            />
            <Button type="submit" disabled={!name.trim()}>
              <Plus />
            </Button>
          </form>
        }
      />
      {isLoading ? (
        <Skeleton className="h-60" />
      ) : !all.length ? (
        <Card>
          <EmptyState icon={<Eye className="size-5" />} title="Keine Watchlists" />
        </Card>
      ) : (
        <div className="grid gap-4 lg:grid-cols-2">
          {all.map((w, wi) => (
            <Card key={`${w.name}-${wi}`}>
              <CardHeader
                title={w.name}
                subtitle={`${w.securities.length} Wertpapiere`}
                actions={
                  <Button
                    size="icon"
                    variant="ghost"
                    aria-label="Watchlist löschen"
                    onClick={() => confirm(`Watchlist „${w.name}“ löschen?`) && save(all.filter((_, i) => i !== wi))}
                  >
                    <Trash2 />
                  </Button>
                }
              />
              <ul className="mt-2 divide-y divide-border">
                {w.securities.map((sid) => {
                  const s = securities?.find((x) => x.id === sid);
                  if (!s) return null;
                  return (
                    <li key={sid} className="group flex items-center gap-3 px-4 py-2.5 sm:px-5">
                      <Link
                        to="/c/$clientId/securities/$securityId"
                        params={{ clientId, securityId: sid }}
                        className="min-w-0 flex-1"
                      >
                        <div className="truncate text-sm font-medium">{s.name}</div>
                        <div className="text-xs text-muted">
                          {s.isin} · {date(s.latest_price_date)}
                        </div>
                      </Link>
                      <div className="text-right">
                        <div className="tabular text-sm">{quote(s.latest_price, s.currency)}</div>
                        <div className={`tabular text-xs ${tone(s.latest_change)}`}>
                          {percent(s.latest_change, { sign: true })}
                        </div>
                      </div>
                      <button
                        type="button"
                        aria-label="Entfernen"
                        className="cursor-pointer p-1 text-muted sm:opacity-0 sm:group-hover:opacity-100"
                        onClick={() =>
                          save(
                            all.map((x, i) =>
                              i === wi ? { ...x, securities: x.securities.filter((y) => y !== sid) } : x,
                            ),
                          )
                        }
                      >
                        <X className="size-4" />
                      </button>
                    </li>
                  );
                })}
              </ul>
              <div className="flex gap-2 p-4 sm:px-5">
                <Select value={adding[wi] ?? ""} onChange={(e) => setAdding({ ...adding, [wi]: e.target.value })}>
                  <option value="">Wertpapier hinzufügen …</option>
                  {securities
                    ?.filter((s) => !w.securities.includes(s.id))
                    .map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                </Select>
                <Button
                  variant="outline"
                  disabled={!adding[wi]}
                  onClick={() =>
                    save(all.map((x, i) => (i === wi ? { ...x, securities: [...x.securities, adding[wi]] } : x))).then(
                      () => setAdding({ ...adding, [wi]: "" }),
                    )
                  }
                >
                  <Plus />
                </Button>
              </div>
            </Card>
          ))}
        </div>
      )}
    </>
  );
}
