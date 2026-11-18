import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, createRoute, createRouter, Outlet, redirect } from "@tanstack/react-router";
import { api, unwrap } from "./api/client";
import { qk } from "./api/hooks";
import { AppShell } from "./components/layout/app-shell";
import { storage } from "./lib/utils";
import { AccountsPage } from "./pages/accounts";
import { ClientsPage } from "./pages/clients";
import { DashboardPage } from "./pages/dashboard";
import { EarningsPage } from "./pages/earnings";
import { HoldingsPage } from "./pages/holdings";
import { ImportPage } from "./pages/import";
import { LoginPage } from "./pages/login";
import { PerformancePage } from "./pages/performance";
import { PlansPage } from "./pages/plans";
import { SecuritiesPage } from "./pages/securities";
import { SecurityDetailPage } from "./pages/security-detail";
import { SettingsPage } from "./pages/settings";
import { TaxonomiesPage } from "./pages/taxonomies";
import { TransactionsPage } from "./pages/transactions";
import { WatchlistsPage } from "./pages/watchlists";

const rootRoute = createRootRouteWithContext<{ queryClient: QueryClient }>()({
  component: () => <Outlet />,
});

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  validateSearch: (s: Record<string, unknown>) => ({ error: typeof s.error === "string" ? s.error : undefined }),
  component: LoginPage,
});

const authedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "authed",
  beforeLoad: async ({ context }) => {
    try {
      await context.queryClient.ensureQueryData({ queryKey: qk.me, queryFn: () => unwrap(api.GET("/auth/me")) });
    } catch {
      throw redirect({ to: "/login", search: { error: undefined }, replace: true });
    }
  },
  component: () => <Outlet />,
});

const indexRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/",
  beforeLoad: async ({ context }) => {
    const clients = await context.queryClient.ensureQueryData({
      queryKey: qk.clients,
      queryFn: () => unwrap(api.GET("/clients")),
    });
    const last = storage<string | null>("lastClient", null);
    const target = clients.find((c) => c.id === last) ?? clients[0];
    if (target) throw redirect({ to: "/c/$clientId", params: { clientId: target.id } });
    throw redirect({ to: "/clients" });
  },
});

const clientsRoute = createRoute({ getParentRoute: () => authedRoute, path: "/clients", component: ClientsPage });

export const clientRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/c/$clientId",
  component: AppShell,
});

const dashboardRoute = createRoute({ getParentRoute: () => clientRoute, path: "/", component: DashboardPage });
const holdingsRoute = createRoute({ getParentRoute: () => clientRoute, path: "/holdings", component: HoldingsPage });
const performanceRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/performance",
  component: PerformancePage,
});
const securitiesRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/securities",
  component: SecuritiesPage,
});
const securityRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/securities/$securityId",
  component: SecurityDetailPage,
});
const accountsRoute = createRoute({ getParentRoute: () => clientRoute, path: "/accounts", component: AccountsPage });
const earningsRoute = createRoute({ getParentRoute: () => clientRoute, path: "/earnings", component: EarningsPage });
const taxonomiesRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/taxonomies",
  component: TaxonomiesPage,
});
const settingsRoute = createRoute({ getParentRoute: () => clientRoute, path: "/settings", component: SettingsPage });
const plansRoute = createRoute({ getParentRoute: () => clientRoute, path: "/plans", component: PlansPage });
const watchlistsRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/watchlists",
  component: WatchlistsPage,
});
const importRoute = createRoute({ getParentRoute: () => clientRoute, path: "/import", component: ImportPage });
const transactionsRoute = createRoute({
  getParentRoute: () => clientRoute,
  path: "/transactions",
  validateSearch: (s: Record<string, unknown>): { owner?: string } => ({
    owner: typeof s.owner === "string" ? s.owner : undefined,
  }),
  component: TransactionsPage,
});

const routeTree = rootRoute.addChildren([
  loginRoute,
  authedRoute.addChildren([
    indexRoute,
    clientsRoute,
    clientRoute.addChildren([
      dashboardRoute,
      holdingsRoute,
      performanceRoute,
      securitiesRoute,
      securityRoute,
      accountsRoute,
      transactionsRoute,
      importRoute,
      plansRoute,
      watchlistsRoute,
      earningsRoute,
      taxonomiesRoute,
      settingsRoute,
    ]),
  ]),
]);

export function createAppRouter(queryClient: QueryClient) {
  return createRouter({ routeTree, context: { queryClient }, defaultPreload: "intent", scrollRestoration: true });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
