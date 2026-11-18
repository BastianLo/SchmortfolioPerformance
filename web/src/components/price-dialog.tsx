import * as React from "react";
import { api, unwrap } from "@/api/client";
import { useInvalidateClient } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import { ErrorBox } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { isoDate, QUOTE } from "@/lib/format";
import { parseNum } from "./booking-dialog";

function parseDate(s: string): string | null {
  const t = s.trim();
  let m = t.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (m) return `${m[1]}-${m[2]}-${m[3]}`;
  m = t.match(/^(\d{1,2})\.(\d{1,2})\.(\d{2,4})$/);
  if (m) {
    const y = m[3].length === 2 ? `20${m[3]}` : m[3];
    return `${y}-${m[2].padStart(2, "0")}-${m[1].padStart(2, "0")}`;
  }
  return null;
}

/** Parses pasted lines like `2024-01-02;101,5` or `02.01.2024<TAB>101.50`. */
export function parsePriceLines(text: string) {
  const out: { date: string; value: number }[] = [];
  for (const line of text.split(/\r?\n/)) {
    const parts = line
      .split(/[;\t]|,(?=\s*\d+[.,]?\d*\s*$)/)
      .map((p) => p.trim())
      .filter(Boolean);
    if (parts.length < 2) continue;
    const d = parseDate(parts[0]);
    const v = parseNum(parts[parts.length - 1]);
    if (d && v > 0) out.push({ date: d, value: Math.round(v * QUOTE) });
  }
  return out;
}

export function PriceDialog({
  open,
  onOpenChange,
  securityId,
  currency,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  securityId: string;
  currency?: string | null;
}) {
  const clientId = useClientId();
  const invalidate = useInvalidateClient(clientId);
  const [day, setDay] = React.useState(isoDate(new Date()));
  const [value, setValue] = React.useState("");
  const [bulk, setBulk] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const parsed = React.useMemo(() => parsePriceLines(bulk), [bulk]);

  const save = async (prices: { date: string; value: number }[]) => {
    setError(null);
    try {
      await unwrap(
        api.PUT("/clients/{id}/securities/{sid}/prices", {
          params: { path: { id: clientId, sid: securityId } },
          body: prices,
        }),
      );
      await invalidate();
      setValue("");
      setBulk("");
      onOpenChange(false);
    } catch (e) {
      setError(e);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Kurse erfassen">
      <div className="space-y-5">
        <form
          className="grid gap-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end"
          onSubmit={(e) => {
            e.preventDefault();
            save([{ date: day, value: Math.round(parseNum(value) * QUOTE) }]);
          }}
        >
          <Field label="Datum">
            <Input type="date" value={day} onChange={(e) => setDay(e.target.value)} required />
          </Field>
          <Field label={`Kurs${currency ? ` (${currency})` : ""}`}>
            <Input inputMode="decimal" value={value} onChange={(e) => setValue(e.target.value)} required />
          </Field>
          <Button type="submit">Speichern</Button>
        </form>
        <div>
          <Field
            label="Mehrere Kurse einfügen"
            hint="Eine Zeile pro Kurs: Datum und Kurs, getrennt durch Semikolon oder Tab (z. B. aus Excel)."
          >
            <textarea
              value={bulk}
              onChange={(e) => setBulk(e.target.value)}
              rows={6}
              className="w-full rounded-lg border border-border bg-surface p-2 font-mono text-xs outline-none focus:border-accent"
              placeholder={"2024-01-02;101,50\n03.01.2024;102,10"}
            />
          </Field>
          <div className="mt-2 flex items-center justify-between">
            <span className="text-xs text-muted">{parsed.length} Kurse erkannt</span>
            <Button variant="outline" disabled={!parsed.length} onClick={() => save(parsed)}>
              Importieren
            </Button>
          </div>
        </div>
        {error != null && <ErrorBox error={error} />}
      </div>
    </Dialog>
  );
}
