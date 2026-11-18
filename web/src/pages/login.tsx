import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { KeyRound, LogIn } from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { qk, useAuthConfig } from "@/api/hooks";
import { Logo } from "@/components/layout/logo";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/input";
import { ErrorBox, Spinner } from "@/components/ui/misc";

const SSO_ERRORS: Record<string, string> = {
  sso_forbidden: "Dein Konto ist für diese Anwendung nicht freigeschaltet (Gruppe fehlt).",
  sso_denied: "Die Anmeldung wurde beim Identity Provider abgebrochen.",
  sso_expired: "Die Anmeldung ist abgelaufen, bitte erneut versuchen.",
  sso_token: "Das Token des Identity Providers war ungültig.",
  sso_not_registered: "Es existiert kein Benutzer zu diesem SSO-Konto.",
  sso_disabled: "SSO ist nicht konfiguriert.",
  sso_config: "SSO ist fehlerhaft konfiguriert.",
  internal: "Interner Fehler bei der Anmeldung.",
};

export function LoginPage() {
  const { error: ssoError } = useSearch({ from: "/login" });
  const { data: cfg, isLoading } = useAuthConfig();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [username, setUsername] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [error, setError] = React.useState<unknown>(ssoError ? new Error(SSO_ERRORS[ssoError] ?? ssoError) : null);
  const [busy, setBusy] = React.useState(false);
  const setup = cfg?.setup_required;

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const body = { username, password };
      const me = await unwrap(setup ? api.POST("/auth/setup", { body }) : api.POST("/auth/login", { body }));
      qc.setQueryData(qk.me, me);
      qc.invalidateQueries({ queryKey: qk.authConfig });
      navigate({ to: "/" });
    } catch (err) {
      setError((err as { status?: number }).status === 401 ? new Error("Benutzername oder Passwort falsch") : err);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex min-h-dvh items-center justify-center bg-bg p-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 flex flex-col items-center gap-3 text-center">
          <Logo className="size-12" />
          <div>
            <h1 className="text-xl font-semibold tracking-tight">SchmortfolioPerformance</h1>
            <p className="mt-1 text-sm text-muted">
              {setup ? "Willkommen! Lege den ersten Administrator an." : "Melde dich an, um deine Portfolios zu sehen."}
            </p>
          </div>
        </div>
        <div className="rounded-2xl border border-border bg-surface p-6 shadow-sm">
          {isLoading ? (
            <div className="flex justify-center py-6">
              <Spinner />
            </div>
          ) : (
            <div className="space-y-4">
              {error != null && <ErrorBox error={error} />}
              {cfg?.oidc && !setup && (
                <Button asChild className="w-full" size="lg">
                  <a href="/api/v1/auth/oidc/login?return_to=/">
                    <KeyRound />
                    {cfg.oidc_label ?? "Mit SSO anmelden"}
                  </a>
                </Button>
              )}
              {cfg?.oidc && cfg.local_login && !setup && (
                <div className="flex items-center gap-3 text-xs text-muted">
                  <div className="h-px flex-1 bg-border" />
                  oder
                  <div className="h-px flex-1 bg-border" />
                </div>
              )}
              {(cfg?.local_login || setup) && (
                <form onSubmit={submit} className="space-y-4">
                  <Field label="Benutzername">
                    <Input
                      autoComplete="username"
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                      required
                      autoFocus
                    />
                  </Field>
                  <Field label="Passwort" hint={setup ? "Mindestens 8 Zeichen" : undefined}>
                    <Input
                      type="password"
                      autoComplete={setup ? "new-password" : "current-password"}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      required
                    />
                  </Field>
                  <Button
                    type="submit"
                    className="w-full"
                    size="lg"
                    disabled={busy}
                    variant={cfg?.oidc && !setup ? "outline" : "default"}
                  >
                    {busy ? <Spinner className="size-4 text-current" /> : <LogIn />}
                    {setup ? "Administrator anlegen" : "Anmelden"}
                  </Button>
                </form>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
