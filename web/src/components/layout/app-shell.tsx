import { useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  ArrowLeftRight,
  BarChart3,
  Briefcase,
  Coins,
  FolderOpen,
  LayoutDashboard,
  LogOut,
  Menu,
  Monitor,
  Moon,
  PieChart,
  Settings,
  Sun,
  TrendingUp,
  Wallet,
} from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { useClients, useMe } from "@/api/hooks";
import { Dialog } from "@/components/ui/dialog";
import { useClientId } from "@/lib/client-id";
import { PeriodProvider } from "@/lib/period-context";
import { type ThemeMode, useTheme } from "@/lib/theme";
import { cn, setStorage } from "@/lib/utils";
import { Logo } from "./logo";

const NAV = [
  { to: "/c/$clientId", label: "Übersicht", icon: LayoutDashboard, exact: true, mobile: true },
  { to: "/c/$clientId/holdings", label: "Vermögensaufstellung", short: "Bestand", icon: Briefcase, mobile: true },
  { to: "/c/$clientId/performance", label: "Performance", icon: TrendingUp, mobile: true },
  { to: "/c/$clientId/transactions", label: "Buchungen", icon: ArrowLeftRight, mobile: true },
  { to: "/c/$clientId/securities", label: "Wertpapiere", icon: BarChart3 },
  { to: "/c/$clientId/accounts", label: "Konten & Depots", icon: Wallet },
  { to: "/c/$clientId/earnings", label: "Erträge", icon: Coins },
  { to: "/c/$clientId/taxonomies", label: "Klassifizierungen", icon: PieChart },
  { to: "/c/$clientId/settings", label: "Einstellungen", icon: Settings },
] as const;

function useActive() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const clientId = useClientId();
  return (to: string, exact?: boolean) => {
    const target = to.replace("$clientId", clientId);
    return exact ? pathname === target || pathname === `${target}/` : pathname.startsWith(target);
  };
}

function ThemeSwitch() {
  const { mode, setMode } = useTheme();
  const opts: { m: ThemeMode; icon: React.ElementType; label: string }[] = [
    { m: "light", icon: Sun, label: "Hell" },
    { m: "system", icon: Monitor, label: "System" },
    { m: "dark", icon: Moon, label: "Dunkel" },
  ];
  return (
    <div className="flex rounded-lg bg-surface-2 p-0.5">
      {opts.map((o) => (
        <button
          key={o.m}
          type="button"
          title={o.label}
          aria-label={o.label}
          onClick={() => setMode(o.m)}
          className={cn(
            "flex flex-1 cursor-pointer items-center justify-center rounded-md py-1 text-muted",
            mode === o.m && "bg-surface text-fg shadow-xs",
          )}
        >
          <o.icon className="size-3.5" />
        </button>
      ))}
    </div>
  );
}

function useLogout() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  return async () => {
    const r = await unwrap(api.POST("/auth/logout")).catch(() => null);
    qc.clear();
    if (r?.redirect) window.location.href = r.redirect;
    else navigate({ to: "/login", search: { error: undefined } });
  };
}

function ClientSwitcher() {
  const clientId = useClientId();
  const { data } = useClients();
  const navigate = useNavigate();
  return (
    <select
      value={clientId}
      onChange={(e) => {
        if (e.target.value === "__manage") navigate({ to: "/clients" });
        else navigate({ to: "/c/$clientId", params: { clientId: e.target.value } });
      }}
      className="h-9 w-full cursor-pointer truncate rounded-lg border border-border bg-surface px-2.5 text-sm font-medium outline-none"
      aria-label="Portfolio wählen"
    >
      {data?.map((c) => (
        <option key={c.id} value={c.id}>
          {c.name}
        </option>
      ))}
      <option value="__manage">Portfolios verwalten …</option>
    </select>
  );
}

function Sidebar() {
  const isActive = useActive();
  const clientId = useClientId();
  const { data: me } = useMe();
  const logout = useLogout();
  return (
    <aside className="sticky top-0 hidden h-dvh w-60 shrink-0 flex-col border-r border-border bg-surface lg:flex">
      <div className="flex items-center gap-2 px-5 pt-5 pb-4">
        <Logo className="size-7" />
        <span className="text-sm font-semibold tracking-tight">Schmortfolio</span>
      </div>
      <div className="px-3 pb-3">
        <ClientSwitcher />
      </div>
      <nav className="flex-1 space-y-0.5 overflow-y-auto px-3">
        {NAV.map((n) => (
          <Link
            key={n.to}
            to={n.to}
            params={{ clientId }}
            className={cn(
              "flex items-center gap-3 rounded-lg px-3 py-2 text-sm text-muted transition-colors hover:bg-surface-2 hover:text-fg",
              isActive(n.to, "exact" in n) && "bg-surface-2 font-medium text-fg",
            )}
          >
            <n.icon className="size-4" />
            {n.label}
          </Link>
        ))}
      </nav>
      <div className="space-y-3 border-t border-border p-3">
        <ThemeSwitch />
        <div className="flex items-center justify-between gap-2 px-1">
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">{me?.display_name ?? me?.username}</div>
            <div className="truncate text-xs text-muted">
              {me?.sso ? "SSO" : "Lokal"}
              {me?.is_admin ? " · Admin" : ""}
            </div>
          </div>
          <button
            type="button"
            onClick={logout}
            className="cursor-pointer rounded-md p-2 text-muted hover:bg-surface-2 hover:text-fg"
            title="Abmelden"
            aria-label="Abmelden"
          >
            <LogOut className="size-4" />
          </button>
        </div>
      </div>
    </aside>
  );
}

function MobileNav() {
  const isActive = useActive();
  const clientId = useClientId();
  const [more, setMore] = React.useState(false);
  const logout = useLogout();
  const mobile = NAV.filter((n) => "mobile" in n);
  const rest = NAV.filter((n) => !("mobile" in n));
  return (
    <>
      <header className="sticky top-0 z-30 flex items-center gap-3 border-b border-border bg-surface/90 px-4 py-2.5 backdrop-blur lg:hidden">
        <Logo className="size-6 shrink-0" />
        <div className="min-w-0 flex-1">
          <ClientSwitcher />
        </div>
      </header>
      <nav className="safe-bottom fixed inset-x-0 bottom-0 z-30 grid grid-cols-5 border-t border-border bg-surface/95 backdrop-blur lg:hidden">
        {mobile.map((n) => (
          <Link
            key={n.to}
            to={n.to}
            params={{ clientId }}
            className={cn(
              "flex flex-col items-center gap-0.5 py-2 text-[10px] text-muted",
              isActive(n.to, "exact" in n) && "text-accent",
            )}
          >
            <n.icon className="size-5" />
            {"short" in n ? n.short : n.label}
          </Link>
        ))}
        <button
          type="button"
          onClick={() => setMore(true)}
          className="flex cursor-pointer flex-col items-center gap-0.5 py-2 text-[10px] text-muted"
        >
          <Menu className="size-5" />
          Mehr
        </button>
      </nav>
      <Dialog open={more} onOpenChange={setMore} title="Mehr">
        <div className="grid grid-cols-2 gap-2">
          {rest.map((n) => (
            <Link
              key={n.to}
              to={n.to}
              params={{ clientId }}
              onClick={() => setMore(false)}
              className="flex items-center gap-2 rounded-xl border border-border p-3 text-sm"
            >
              <n.icon className="size-4 text-muted" />
              {n.label}
            </Link>
          ))}
          <Link
            to="/clients"
            onClick={() => setMore(false)}
            className="flex items-center gap-2 rounded-xl border border-border p-3 text-sm"
          >
            <FolderOpen className="size-4 text-muted" />
            Portfolios
          </Link>
          <button
            type="button"
            onClick={logout}
            className="flex cursor-pointer items-center gap-2 rounded-xl border border-border p-3 text-left text-sm"
          >
            <LogOut className="size-4 text-muted" />
            Abmelden
          </button>
        </div>
        <div className="mt-4">
          <ThemeSwitch />
        </div>
      </Dialog>
    </>
  );
}

export function AppShell() {
  const clientId = useClientId();
  React.useEffect(() => setStorage("lastClient", clientId), [clientId]);
  return (
    <PeriodProvider>
      <div className="flex min-h-dvh">
        <Sidebar />
        <div className="min-w-0 flex-1">
          <MobileNav />
          <main className="mx-auto max-w-7xl px-4 pt-4 pb-24 sm:px-6 sm:pt-6 lg:px-8 lg:pb-10">
            <Outlet />
          </main>
        </div>
      </div>
    </PeriodProvider>
  );
}
