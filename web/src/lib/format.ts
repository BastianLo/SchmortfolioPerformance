// Values from the API: money in 1/100, shares and quotes in 1e-8.
export const AMOUNT = 100;
export const SHARES = 100_000_000;
export const QUOTE = 100_000_000;

const nf = new Map<string, Intl.NumberFormat>();
function fmt(key: string, opts: Intl.NumberFormatOptions) {
  let f = nf.get(key);
  if (!f) {
    f = new Intl.NumberFormat("de-DE", opts);
    nf.set(key, f);
  }
  return f;
}

export function money(
  amount: number | null | undefined,
  currency = "EUR",
  opts: { sign?: boolean; compact?: boolean } = {},
) {
  if (amount == null) return "–";
  const v = amount / AMOUNT;
  if (opts.compact && Math.abs(v) >= 10_000) {
    // German has no short form for thousands; drop the cents and abbreviate millions
    if (Math.abs(v) >= 1_000_000) {
      const n = fmt(`mio-${opts.sign}`, {
        maximumFractionDigits: 2,
        minimumFractionDigits: 2,
        signDisplay: opts.sign ? "exceptZero" : "auto",
      }).format(v / 1_000_000);
      return `${n} Mio. ${currencySymbol(currency)}`;
    }
    return fmt(`c-${currency}-${opts.sign}`, {
      style: "currency",
      currency,
      maximumFractionDigits: 0,
      minimumFractionDigits: 0,
      signDisplay: opts.sign ? "exceptZero" : "auto",
    }).format(v);
  }
  return fmt(`m-${currency}-${opts.sign}`, {
    style: "currency",
    currency,
    signDisplay: opts.sign ? "exceptZero" : "auto",
  }).format(v);
}

function currencySymbol(currency: string) {
  return (
    fmt(`sym-${currency}`, { style: "currency", currency, currencyDisplay: "narrowSymbol" })
      .formatToParts(0)
      .find((p) => p.type === "currency")?.value ?? currency
  );
}

export function number(v: number | null | undefined, digits = 2) {
  if (v == null) return "–";
  return fmt(`n-${digits}`, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
}

export function shares(v: number | null | undefined) {
  if (v == null) return "–";
  return fmt("shares", { maximumFractionDigits: 6 }).format(v / SHARES);
}

export function quote(v: number | null | undefined, currency?: string | null) {
  if (v == null) return "–";
  const x = v / QUOTE;
  if (!currency) return fmt("q", { minimumFractionDigits: 2, maximumFractionDigits: 4 }).format(x);
  return fmt(`q-${currency}`, {
    style: "currency",
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 4,
  }).format(x);
}

export function percent(v: number | null | undefined, opts: { sign?: boolean; digits?: number } = {}) {
  if (v == null || !Number.isFinite(v)) return "–";
  const d = opts.digits ?? 2;
  return fmt(`p-${d}-${opts.sign}`, {
    style: "percent",
    minimumFractionDigits: d,
    maximumFractionDigits: d,
    signDisplay: opts.sign ? "exceptZero" : "auto",
  }).format(v);
}

const df = new Intl.DateTimeFormat("de-DE", { day: "2-digit", month: "2-digit", year: "numeric" });
const dfShort = new Intl.DateTimeFormat("de-DE", { month: "short", year: "2-digit" });

export function date(v: string | null | undefined) {
  if (!v) return "–";
  return df.format(new Date(v.length <= 10 ? `${v}T00:00:00` : v));
}

export function monthYear(v: string) {
  return dfShort.format(new Date(`${v}T00:00:00`));
}

export function tone(v: number | null | undefined) {
  if (v == null || v === 0 || !Number.isFinite(v)) return "text-muted";
  return v > 0 ? "text-pos" : "text-neg";
}

export function isoDate(d: Date) {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export const TX_LABELS: Record<string, string> = {
  DEPOSIT: "Einlage",
  REMOVAL: "Entnahme",
  INTEREST: "Zinsen",
  INTEREST_CHARGE: "Zinsbelastung",
  DIVIDENDS: "Dividende",
  FEES: "Gebühren",
  FEES_REFUND: "Gebührenerstattung",
  TAXES: "Steuern",
  TAX_REFUND: "Steuerrückerstattung",
  BUY: "Kauf",
  SELL: "Verkauf",
  TRANSFER_IN: "Umbuchung (Eingang)",
  TRANSFER_OUT: "Umbuchung (Ausgang)",
  DELIVERY_INBOUND: "Einlieferung",
  DELIVERY_OUTBOUND: "Auslieferung",
};
