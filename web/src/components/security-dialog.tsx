import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useInvalidateClient } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input, Select } from "@/components/ui/input";
import { ErrorBox } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";

export const FEEDS: { key: string; label: string; url?: boolean; hint?: string }[] = [
  { key: "", label: "Keine automatische Aktualisierung" },
  { key: "MANUAL", label: "Manuell" },
  { key: "YAHOO", label: "Yahoo Finance", hint: "Ticker-Symbol wie bei Yahoo, z. B. SAP.DE, AAPL, EUNL.DE" },
  { key: "GENERIC-JSON", label: "JSON (eigene URL)", url: true, hint: "URL mit JSON-Antwort; Pfade unten" },
  {
    key: "GENERIC_HTML_TABLE",
    label: "HTML-Tabelle (eigene URL)",
    url: true,
    hint: "Seite mit Kurstabelle (Datum, Schlusskurs)",
  },
  { key: "COINGECKO", label: "CoinGecko (Krypto)", hint: "Ticker = CoinGecko-ID, z. B. bitcoin" },
  { key: "ALPHAVANTAGE", label: "Alpha Vantage", hint: "API-Key in den Einstellungen hinterlegen" },
];

type Detail = Schemas["SecurityDetail"];

export function SecurityDialog({
  open,
  onOpenChange,
  existing,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  existing?: Detail;
  onCreated?: (id: string) => void;
}) {
  const clientId = useClientId();
  const invalidate = useInvalidateClient(clientId);
  const s = existing?.security;
  const [form, setForm] = React.useState(() => ({
    name: s?.name ?? "",
    isin: s?.isin ?? "",
    wkn: s?.wkn ?? "",
    ticker: s?.ticker ?? "",
    currency: s?.currency ?? "EUR",
    note: s?.note ?? "",
    feed: s?.feed ?? "YAHOO",
    feed_url: existing?.feed_url ?? "",
    calendar: existing?.calendar ?? "",
    retired: s?.retired ?? false,
    json_date: existing?.feed_config?.date_path ?? "",
    json_close: existing?.feed_config?.close_path ?? "",
  }));
  const [error, setError] = React.useState<unknown>(null);
  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm({ ...form, [k]: e.target.type === "checkbox" ? (e.target as HTMLInputElement).checked : e.target.value });
  const feed = FEEDS.find((f) => f.key === form.feed);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const body: Schemas["SecurityInput"] = {
      name: form.name,
      isin: form.isin || null,
      wkn: form.wkn || null,
      ticker: form.ticker || null,
      currency: form.currency || null,
      note: form.note || null,
      feed: form.feed || null,
      feed_url: form.feed_url || null,
      calendar: form.calendar || null,
      retired: form.retired,
      feed_config:
        form.feed === "GENERIC-JSON"
          ? { date_path: form.json_date || null, close_path: form.json_close || null }
          : null,
    };
    try {
      if (s) {
        await unwrap(
          api.PUT("/clients/{id}/securities/{sid}", { params: { path: { id: clientId, sid: s.id } }, body }),
        );
      } else {
        const id = await unwrap(api.POST("/clients/{id}/securities", { params: { path: { id: clientId } }, body }));
        onCreated?.(id);
      }
      await invalidate();
      onOpenChange(false);
    } catch (err) {
      setError(err);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title={s ? "Wertpapier bearbeiten" : "Neues Wertpapier"}>
      <form onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <div className="sm:col-span-2">
          <Field label="Name">
            <Input value={form.name} onChange={set("name")} required autoFocus />
          </Field>
        </div>
        <Field label="ISIN">
          <Input value={form.isin} onChange={set("isin")} maxLength={12} />
        </Field>
        <Field label="WKN">
          <Input value={form.wkn} onChange={set("wkn")} />
        </Field>
        <Field label="Ticker-Symbol">
          <Input value={form.ticker} onChange={set("ticker")} />
        </Field>
        <Field label="Währung">
          <Input value={form.currency} onChange={set("currency")} maxLength={3} />
        </Field>
        <div className="sm:col-span-2">
          <Field label="Kursquelle" hint={feed?.hint}>
            <Select value={form.feed} onChange={set("feed")}>
              {FEEDS.map((f) => (
                <option key={f.key} value={f.key}>
                  {f.label}
                </option>
              ))}
              {!FEEDS.some((f) => f.key === form.feed) && <option value={form.feed}>{form.feed} (aus PP)</option>}
            </Select>
          </Field>
        </div>
        {feed?.url && (
          <div className="sm:col-span-2">
            <Field label="URL" hint="Platzhalter: {TICKER}, {ISIN}, {WKN}, {FROM}, {TO}">
              <Input value={form.feed_url} onChange={set("feed_url")} placeholder="https://…" />
            </Field>
          </div>
        )}
        {form.feed === "GENERIC-JSON" && (
          <>
            <Field label="JSON-Pfad Datum" hint="z. B. $.data[*].date">
              <Input value={form.json_date} onChange={set("json_date")} />
            </Field>
            <Field label="JSON-Pfad Kurs" hint="z. B. $.data[*].close">
              <Input value={form.json_close} onChange={set("json_close")} />
            </Field>
          </>
        )}
        <div className="sm:col-span-2">
          <Field label="Notiz">
            <Input value={form.note} onChange={set("note")} />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm sm:col-span-2">
          <input
            type="checkbox"
            checked={form.retired}
            onChange={set("retired")}
            className="size-4 accent-[var(--accent)]"
          />
          Inaktiv (wird in Auswahllisten ausgeblendet)
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
          <Button type="submit">{s ? "Speichern" : "Anlegen"}</Button>
        </div>
      </form>
    </Dialog>
  );
}
