import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Download, KeyRound, Plus, Shield, Trash2 } from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { useMe } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { Badge, ErrorBox } from "@/components/ui/misc";
import { date } from "@/lib/format";

export function PasswordCard() {
  const { data: me } = useMe();
  const [current, setCurrent] = React.useState("");
  const [next, setNext] = React.useState("");
  const [msg, setMsg] = React.useState<string | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await unwrap(api.POST("/auth/password", { body: { current: current || null, new: next } }));
      setMsg("Passwort geändert. Bitte neu anmelden.");
      setCurrent("");
      setNext("");
    } catch (err) {
      setError((err as { status?: number }).status === 401 ? new Error("Aktuelles Passwort falsch") : err);
    }
  };
  return (
    <Card>
      <CardHeader title="Passwort" subtitle={me?.sso ? "Für die Anmeldung ohne SSO" : undefined} />
      <CardBody>
        <form onSubmit={submit} className="space-y-3">
          <Field label="Aktuelles Passwort">
            <Input
              type="password"
              autoComplete="current-password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
          </Field>
          <Field label="Neues Passwort" hint="Mindestens 8 Zeichen">
            <Input
              type="password"
              autoComplete="new-password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
              required
              minLength={8}
            />
          </Field>
          {error != null && <ErrorBox error={error} />}
          {msg && <p className="text-sm text-pos">{msg}</p>}
          <Button type="submit" variant="outline">
            Ändern
          </Button>
        </form>
      </CardBody>
    </Card>
  );
}

export function TokensCard() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["tokens"], queryFn: () => unwrap(api.GET("/auth/tokens")) });
  const [name, setName] = React.useState("");
  const [write, setWrite] = React.useState(false);
  const [created, setCreated] = React.useState<string | null>(null);
  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    const r = await unwrap(api.POST("/auth/tokens", { body: { name, read_only: !write } }));
    setCreated(r.token);
    setName("");
    qc.invalidateQueries({ queryKey: ["tokens"] });
  };
  const remove = async (id: string) => {
    await unwrap(api.DELETE("/auth/tokens/{tid}", { params: { path: { tid: id } } }));
    qc.invalidateQueries({ queryKey: ["tokens"] });
  };
  return (
    <Card>
      <CardHeader
        title="API-Tokens"
        subtitle="Für Skripte und Integrationen wie Home Assistant (Header: Authorization: Bearer …)"
      />
      <CardBody className="space-y-3 text-sm">
        <ul className="divide-y divide-border rounded-lg border border-border">
          {data?.map((t) => (
            <li key={t.id} className="flex items-center gap-2 px-3 py-2">
              <KeyRound className="size-4 text-muted" />
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{t.name}</div>
                <div className="text-xs text-muted">
                  {t.read_only ? "nur lesen" : "lesen und schreiben"} · erstellt {date(t.created_at)}
                  {t.last_used_at ? ` · zuletzt ${date(t.last_used_at)}` : ""}
                </div>
              </div>
              <Button size="icon" variant="ghost" onClick={() => remove(t.id)} aria-label="Widerrufen">
                <Trash2 />
              </Button>
            </li>
          ))}
          {!data?.length && <li className="px-3 py-2 text-xs text-muted">Keine Tokens</li>}
        </ul>
        {created && (
          <div className="rounded-lg border border-warn/40 bg-warn/10 p-3">
            <div className="mb-1 text-xs">Token jetzt kopieren – er wird nicht noch einmal angezeigt:</div>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded bg-surface px-2 py-1 text-xs">{created}</code>
              <Button size="sm" variant="outline" onClick={() => navigator.clipboard?.writeText(created)}>
                <Copy />
              </Button>
            </div>
          </div>
        )}
        <form onSubmit={create} className="flex flex-wrap items-center gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Name, z. B. Home Assistant"
            className="min-w-40 flex-1"
            required
          />
          <label className="flex items-center gap-1.5 text-xs">
            <input type="checkbox" checked={write} onChange={(e) => setWrite(e.target.checked)} className="size-4" />
            Schreibzugriff
          </label>
          <Button type="submit" variant="outline">
            <Plus /> Erstellen
          </Button>
        </form>
      </CardBody>
    </Card>
  );
}

export function UsersCard() {
  const qc = useQueryClient();
  const { data: me } = useMe();
  const { data } = useQuery({ queryKey: ["users"], queryFn: () => unwrap(api.GET("/admin/users")) });
  const [username, setUsername] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [error, setError] = React.useState<unknown>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: ["users"] });
  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    try {
      await unwrap(api.POST("/admin/users", { body: { username, password, is_admin: false } }));
      setUsername("");
      setPassword("");
      refresh();
    } catch (err) {
      setError(err);
    }
  };
  const update = async (uid: string, body: { is_admin?: boolean; password?: string }) => {
    try {
      await unwrap(api.PUT("/admin/users/{uid}", { params: { path: { uid } }, body }));
      refresh();
    } catch (err) {
      setError(err);
    }
  };
  return (
    <Card>
      <CardHeader title="Benutzer" subtitle="Jeder Benutzer sieht nur seine eigenen Portfolios." />
      <CardBody className="space-y-3 text-sm">
        <ul className="divide-y divide-border rounded-lg border border-border">
          {data?.map((u) => (
            <li key={u.id} className="flex flex-wrap items-center gap-2 px-3 py-2">
              <div className="min-w-0 flex-1">
                <span className="font-medium">{u.username}</span>
                {u.is_admin && <Badge className="ml-1">Admin</Badge>}
                {u.sso && <Badge className="ml-1">SSO</Badge>}
                {u.email && <div className="text-xs text-muted">{u.email}</div>}
              </div>
              {u.id !== me?.id && (
                <>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => update(u.id, { is_admin: !u.is_admin })}
                    title="Admin-Rechte umschalten"
                  >
                    <Shield />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    title="Passwort zurücksetzen"
                    onClick={() => {
                      const p = prompt(`Neues Passwort für ${u.username} (min. 8 Zeichen)`);
                      if (p) update(u.id, { password: p });
                    }}
                  >
                    <KeyRound />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    title="Löschen"
                    onClick={async () => {
                      if (confirm(`Benutzer ${u.username} mit allen Portfolios löschen?`)) {
                        await unwrap(api.DELETE("/admin/users/{uid}", { params: { path: { uid: u.id } } }));
                        refresh();
                      }
                    }}
                  >
                    <Trash2 />
                  </Button>
                </>
              )}
            </li>
          ))}
        </ul>
        <form onSubmit={create} className="flex flex-wrap gap-2">
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="Benutzername"
            className="min-w-32 flex-1"
            required
          />
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Passwort"
            className="min-w-32 flex-1"
            required
            minLength={8}
          />
          <Button type="submit" variant="outline">
            <Plus /> Anlegen
          </Button>
        </form>
        {error != null && <ErrorBox error={error} />}
      </CardBody>
    </Card>
  );
}

export function BackupsCard() {
  const qc = useQueryClient();
  const { data } = useQuery({ queryKey: ["backups"], queryFn: () => unwrap(api.GET("/admin/backups")) });
  const [busy, setBusy] = React.useState(false);
  const create = async () => {
    setBusy(true);
    try {
      await unwrap(api.POST("/admin/backups"));
      qc.invalidateQueries({ queryKey: ["backups"] });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <CardHeader
        title="Backups"
        subtitle="Täglich automatisch in data/backups. Wiederherstellen: Server stoppen, Datei als data/schmortfolio.db einsetzen."
        actions={
          <Button size="sm" variant="outline" onClick={create} disabled={busy}>
            <Plus /> Jetzt sichern
          </Button>
        }
      />
      <ul className="mt-2 divide-y divide-border text-sm">
        {data?.map((b) => (
          <li key={b.name} className="flex items-center gap-2 px-4 py-2 sm:px-5">
            <span className="min-w-0 flex-1 truncate">{b.name}</span>
            <span className="text-xs text-muted">{(b.size / 1024 / 1024).toFixed(1)} MB</span>
            <Button size="icon" variant="ghost" asChild>
              <a href={`/api/v1/admin/backups/${b.name}`} download aria-label="Herunterladen">
                <Download />
              </a>
            </Button>
          </li>
        ))}
        {!data?.length && <li className="px-5 py-3 text-xs text-muted">Noch keine Backups</li>}
      </ul>
    </Card>
  );
}
