import { useNavigate } from "@tanstack/react-router";
import * as React from "react";
import { api, type Schemas, unwrap } from "@/api/client";
import { useAccounts } from "@/api/hooks";
import { Dropzone, ImportResult, ImportReview, type ReviewRow } from "@/components/import-review";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Field, Select } from "@/components/ui/input";
import { ErrorBox } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { AMOUNT, SHARES } from "@/lib/format";
import { setStorage, storage } from "@/lib/utils";

/** Splits CSV text with quotes; detects `;`, `,` or tab as delimiter. */
export function parseCsv(text: string): string[][] {
  const first = text.split(/\r?\n/)[0] ?? "";
  const delim = [";", "\t", ","].map((d) => [d, first.split(d).length] as const).sort((a, b) => b[1] - a[1])[0][0];
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"') quoted = true;
    else if (c === delim) {
      row.push(cell);
      cell = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      if (row.some((x) => x.trim())) rows.push(row);
      row = [];
      cell = "";
    } else cell += c;
  }
  row.push(cell);
  if (row.some((x) => x.trim())) rows.push(row);
  return rows;
}

/** Number in German or English notation. */
export function parseNumber(s: string | undefined): number {
  if (!s) return 0;
  const t = s.replace(/[^\d,.-]/g, "");
  const lastDot = t.lastIndexOf(".");
  const lastComma = t.lastIndexOf(",");
  let n = t;
  if (lastComma > lastDot) n = t.replace(/\./g, "").replace(",", ".");
  else if (lastDot > lastComma && lastComma >= 0) n = t.replace(/,/g, "");
  const v = Number.parseFloat(n);
  return Number.isFinite(v) ? v : 0;
}

export function parseDate(s: string | undefined): string | null {
  if (!s) return null;
  const t = s.trim();
  let m = t.match(/^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2}))?/);
  if (m) return `${m[1]}-${m[2]}-${m[3]}T${m[4] ?? "00"}:${m[5] ?? "00"}:00`;
  m = t.match(/^(\d{1,2})[./](\d{1,2})[./](\d{2,4})(?: (\d{2}):(\d{2}))?/);
  if (m) {
    const y = m[3].length === 2 ? `20${m[3]}` : m[3];
    return `${y}-${m[2].padStart(2, "0")}-${m[1].padStart(2, "0")}T${m[4] ?? "00"}:${m[5] ?? "00"}:00`;
  }
  return null;
}

const KINDS: Record<string, string> = {
  kauf: "BUY",
  buy: "BUY",
  einlieferung: "BUY",
  verkauf: "SELL",
  sell: "SELL",
  auslieferung: "SELL",
  dividende: "DIVIDENDS",
  dividenden: "DIVIDENDS",
  dividend: "DIVIDENDS",
  ausschüttung: "DIVIDENDS",
  ertrag: "DIVIDENDS",
  einlage: "DEPOSIT",
  einzahlung: "DEPOSIT",
  deposit: "DEPOSIT",
  entnahme: "REMOVAL",
  auszahlung: "REMOVAL",
  removal: "REMOVAL",
  zinsen: "INTEREST",
  interest: "INTEREST",
  gebühren: "FEES",
  gebühr: "FEES",
  fees: "FEES",
  steuern: "TAXES",
  taxes: "TAXES",
  steuerrückerstattung: "TAX_REFUND",
};

const FIELDS = [
  { key: "date", label: "Datum", required: true, guess: ["datum", "date", "buchungstag", "valuta"] },
  { key: "kind", label: "Typ", guess: ["typ", "type", "art", "buchungsart", "transaktion"] },
  { key: "amount", label: "Betrag", required: true, guess: ["wert", "betrag", "amount", "value", "gesamt", "total"] },
  { key: "currency", label: "Währung", guess: ["buchungswährung", "währung", "currency"] },
  { key: "shares", label: "Stück", guess: ["stück", "anzahl", "shares", "quantity", "menge"] },
  { key: "name", label: "Wertpapiername", guess: ["wertpapiername", "wertpapier", "name", "security", "bezeichnung"] },
  { key: "isin", label: "ISIN", guess: ["isin"] },
  { key: "wkn", label: "WKN", guess: ["wkn"] },
  { key: "fees", label: "Gebühren", guess: ["gebühren", "gebühr", "fees", "provision"] },
  { key: "taxes", label: "Steuern", guess: ["steuern", "steuer", "taxes", "tax"] },
  { key: "rate", label: "Wechselkurs", guess: ["wechselkurs", "exchange rate", "devisenkurs"] },
] as const;

type FieldKey = (typeof FIELDS)[number]["key"];

function guessMapping(header: string[]): Record<FieldKey, number> {
  const m = {} as Record<FieldKey, number>;
  const lower = header.map((h) => h.trim().toLowerCase());
  for (const f of FIELDS) {
    m[f.key] = lower.findIndex((h) => (f.guess as readonly string[]).includes(h));
    if (m[f.key] < 0) m[f.key] = lower.findIndex((h) => f.guess.some((g) => h.startsWith(g)));
  }
  return m;
}

export function CsvImport() {
  const clientId = useClientId();
  const navigate = useNavigate();
  const { data: accounts } = useAccounts(clientId);
  const [rows, setRows] = React.useState<string[][]>([]);
  const [mapping, setMapping] = React.useState<Record<FieldKey, number> | null>(null);
  const [defaultKind, setDefaultKind] = React.useState("BUY");
  const [review, setReview] = React.useState<ReviewRow[]>([]);
  const [result, setResult] = React.useState<Schemas["CommitResult"] | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const header = rows[0] ?? [];
  const signature = header.join("|");

  const load = async (files: File[]) => {
    const f = files[0];
    if (!f) return;
    setResult(null);
    setReview([]);
    const buf = await f.arrayBuffer();
    let text = new TextDecoder("utf-8").decode(buf);
    if (text.includes("�")) text = new TextDecoder("windows-1252").decode(buf);
    const parsed = parseCsv(text.replace(/^﻿/, ""));
    setRows(parsed);
    const sig = (parsed[0] ?? []).join("|");
    setMapping(storage<Record<FieldKey, number> | null>(`csv-profile:${sig}`, null) ?? guessMapping(parsed[0] ?? []));
  };

  const build = (): Schemas["MatchItem"][] => {
    if (!mapping) return [];
    const cur = accounts?.[0]?.currency ?? "EUR";
    const col = (r: string[], k: FieldKey) => (mapping[k] >= 0 ? r[mapping[k]]?.trim() : undefined);
    const out: Schemas["MatchItem"][] = [];
    for (const r of rows.slice(1)) {
      const date = parseDate(col(r, "date"));
      if (!date) continue;
      const rawKind = col(r, "kind")?.toLowerCase();
      const kind = rawKind ? (KINDS[rawKind] ?? KINDS[rawKind.split(/[ (]/)[0]] ?? rawKind.toUpperCase()) : defaultKind;
      const rate = parseNumber(col(r, "rate"));
      out.push({
        kind,
        date_time: date,
        name: col(r, "name") || null,
        isin: col(r, "isin") || null,
        wkn: col(r, "wkn") || null,
        shares: Math.round(Math.abs(parseNumber(col(r, "shares"))) * SHARES),
        amount: Math.round(Math.abs(parseNumber(col(r, "amount"))) * AMOUNT),
        currency: col(r, "currency") || cur,
        fees: Math.round(Math.abs(parseNumber(col(r, "fees"))) * AMOUNT),
        taxes: Math.round(Math.abs(parseNumber(col(r, "taxes"))) * AMOUNT),
        exchange_rate: rate ? String(rate) : null,
      });
    }
    return out;
  };

  const check = async () => {
    setError(null);
    try {
      if (mapping) setStorage(`csv-profile:${signature}`, mapping);
      const items = build();
      const matched = await unwrap(
        api.POST("/clients/{id}/imports/match", { params: { path: { id: clientId } }, body: { items } }),
      );
      setReview(matched.map((tx, i) => ({ key: String(i), tx, selected: !tx.duplicate_of })));
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div className="space-y-4">
      <Card>
        <CardBody className="space-y-4">
          <p className="text-sm text-muted">
            CSV mit einer Buchung pro Zeile, z. B. ein Export aus Portfolio Performance oder deiner Bank. Die Spalten
            werden automatisch erkannt und können angepasst werden; die Zuordnung wird für Dateien mit gleichen Spalten
            gemerkt.
          </p>
          <Dropzone onFiles={load} accept=".csv,.txt" label="CSV-Datei auswählen" />
          {result && (
            <ImportResult
              result={result}
              onOpen={() => navigate({ to: "/c/$clientId/transactions", params: { clientId } })}
            />
          )}
        </CardBody>
      </Card>
      {mapping && header.length > 0 && (
        <Card>
          <CardHeader title="Spaltenzuordnung" subtitle={`${rows.length - 1} Zeilen`} />
          <CardBody className="space-y-4">
            <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-4">
              {FIELDS.map((f) => (
                <Field key={f.key} label={`${f.label}${"required" in f ? " *" : ""}`}>
                  <Select
                    value={mapping[f.key]}
                    onChange={(e) => setMapping({ ...mapping, [f.key]: Number(e.target.value) })}
                  >
                    <option value={-1}>–</option>
                    {header.map((h, i) => (
                      <option key={`${h}-${i}`} value={i}>
                        {h || `Spalte ${i + 1}`}
                      </option>
                    ))}
                  </Select>
                </Field>
              ))}
              {mapping.kind < 0 && (
                <Field label="Typ für alle Zeilen">
                  <Select value={defaultKind} onChange={(e) => setDefaultKind(e.target.value)}>
                    {["BUY", "SELL", "DIVIDENDS", "DEPOSIT", "REMOVAL", "INTEREST", "FEES", "TAXES"].map((k) => (
                      <option key={k} value={k}>
                        {k}
                      </option>
                    ))}
                  </Select>
                </Field>
              )}
            </div>
            <div className="overflow-x-auto rounded-lg border border-border">
              <table className="w-full text-xs">
                <thead>
                  <tr className="bg-surface-2">
                    {header.map((h, i) => (
                      <th key={`${h}-${i}`} className="px-2 py-1.5 text-left font-medium whitespace-nowrap">
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(1, 6).map((r, i) => (
                    <tr key={i} className="border-t border-border">
                      {header.map((_, j) => (
                        <td key={j} className="px-2 py-1 whitespace-nowrap">
                          {r[j]}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {error != null && <ErrorBox error={error} />}
            <div className="flex justify-end">
              <Button onClick={check} disabled={mapping.date < 0 || mapping.amount < 0}>
                Prüfen
              </Button>
            </div>
          </CardBody>
        </Card>
      )}
      {review.length > 0 && (
        <ImportReview
          rows={review}
          setRows={setReview}
          onDone={(r) => {
            setResult(r);
            setReview([]);
            setRows([]);
            setMapping(null);
          }}
        />
      )}
    </div>
  );
}
