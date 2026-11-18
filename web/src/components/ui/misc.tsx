import { AlertTriangle, Loader2 } from "lucide-react";
import type * as React from "react";
import { cn } from "@/lib/utils";

export function Badge({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md bg-surface-2 px-1.5 py-0.5 text-[11px] font-medium text-muted",
        className,
      )}
      {...props}
    />
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("animate-pulse rounded-md bg-surface-2", className)} />;
}

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn("size-5 animate-spin text-muted", className)} />;
}

export function EmptyState({
  icon,
  title,
  children,
}: {
  icon?: React.ReactNode;
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-14 text-center">
      {icon && <div className="rounded-full bg-surface-2 p-3 text-muted">{icon}</div>}
      <h3 className="text-base font-semibold">{title}</h3>
      {children && <div className="max-w-md text-sm text-muted">{children}</div>}
    </div>
  );
}

export function ErrorBox({ error }: { error: unknown }) {
  const msg = error instanceof Error ? error.message : String(error);
  return (
    <div className="flex items-start gap-2 rounded-lg border border-neg/30 bg-neg/10 p-3 text-sm text-neg">
      <AlertTriangle className="mt-0.5 size-4 shrink-0" />
      <span>{msg}</span>
    </div>
  );
}

export function Warnings({ items }: { items?: string[] }) {
  if (!items?.length) return null;
  return (
    <div className="flex items-start gap-2 rounded-lg border border-warn/30 bg-warn/10 p-3 text-sm">
      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warn" />
      <ul className="space-y-0.5">
        {items.map((w) => (
          <li key={w}>{w}</li>
        ))}
      </ul>
    </div>
  );
}

export function Stat({
  label,
  value,
  sub,
  className,
  valueClass,
}: {
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
  className?: string;
  valueClass?: string;
}) {
  return (
    <div className={cn("min-w-0", className)}>
      <div className="truncate text-xs font-medium text-muted">{label}</div>
      <div className={cn("tabular mt-1 truncate text-lg font-semibold sm:text-xl", valueClass)}>{value}</div>
      {sub && <div className="tabular mt-0.5 truncate text-xs">{sub}</div>}
    </div>
  );
}

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  className,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { key: T; label: string }[];
  className?: string;
}) {
  return (
    <div className={cn("inline-flex rounded-lg bg-surface-2 p-0.5", className)}>
      {options.map((o) => (
        <button
          key={o.key}
          type="button"
          onClick={() => onChange(o.key)}
          className={cn(
            "cursor-pointer rounded-md px-2.5 py-1 text-xs font-medium text-muted transition-colors",
            value === o.key && "bg-surface text-fg shadow-xs",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <div className="mb-4 flex flex-wrap items-end justify-between gap-3 sm:mb-6">
      <div className="min-w-0">
        <h1 className="truncate text-xl font-semibold tracking-tight sm:text-2xl">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}
