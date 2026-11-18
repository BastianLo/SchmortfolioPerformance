import { useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { ChevronRight, FileUp, Plus, Sparkles, Trash2 } from "lucide-react";
import * as React from "react";
import { api, postForm, type Schemas, unwrap } from "@/api/client";
import { qk, useClients } from "@/api/hooks";
import { Logo } from "@/components/layout/logo";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field, Input } from "@/components/ui/input";
import { EmptyState, ErrorBox, Spinner } from "@/components/ui/misc";
import { date } from "@/lib/format";

function ImportDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [file, setFile] = React.useState<File | null>(null);
  const [password, setPassword] = React.useState("");
  const [needsPassword, setNeedsPassword] = React.useState(false);
  const [name, setName] = React.useState("");
  const [preview, setPreview] = React.useState<Schemas["ImportResponse"] | null>(null);
  const [error, setError] = React.useState<unknown>(null);
  const [busy, setBusy] = React.useState(false);
  const qc = useQueryClient();
  const navigate = useNavigate();

  React.useEffect(() => {
    if (!open) {
      setFile(null);
      setPassword("");
      setNeedsPassword(false);
      setName("");
      setPreview(null);
      setError(null);
    }
  }, [open]);

  const send = async (dryRun: boolean) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    const form = new FormData();
    form.append("file", file);
    if (password) form.append("password", password);
    if (name) form.append("name", name);
    form.append("dry_run", String(dryRun));
    try {
      const r = await postForm<Schemas["ImportResponse"]>("/clients/import", form);
      if (dryRun) setPreview(r);
      else {
        await qc.invalidateQueries({ queryKey: qk.clients });
        onOpenChange(false);
        if (r.client) navigate({ to: "/c/$clientId", params: { clientId: r.client.id } });
      }
    } catch (e) {
      const msg = (e as Error).message;
      if (msg.includes("Passwort")) setNeedsPassword(true);
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Portfolio Performance importieren"
      description="Unterstützt .xml, .portfolio (binär, verschlüsselt) und .zip – alle Dateiversionen ab PP 0.x."
    >
      <div className="space-y-4">
        <label className="flex cursor-pointer flex-col items-center gap-2 rounded-xl border-2 border-dashed border-border p-6 text-center hover:border-accent">
          <FileUp className="size-6 text-muted" />
          <span className="text-sm font-medium">{file ? file.name : "Datei auswählen"}</span>
          <span className="text-xs text-muted">
            {file ? `${(file.size / 1024).toFixed(0)} KB` : "Klicken oder Datei hierher ziehen"}
          </span>
          <input
            type="file"
            accept=".xml,.portfolio,.zip"
            className="sr-only"
            onChange={(e) => {
              const f = e.target.files?.[0] ?? null;
              setFile(f);
              setPreview(null);
              setError(null);
              if (f && !name) setName(f.name.replace(/\.[^.]+$/, ""));
            }}
          />
        </label>
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Mein Portfolio" />
        </Field>
        {needsPassword && (
          <Field label="Passwort der Datei">
            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoFocus />
          </Field>
        )}
        {error != null && <ErrorBox error={error} />}
        {preview && (
          <div className="rounded-xl bg-surface-2 p-4 text-sm">
            <div className="mb-2 font-medium">Vorschau</div>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-muted">
              <dt>Format</dt>
              <dd className="text-fg">
                {preview.format} · Version {preview.file_version}
                {preview.encrypted ? " · verschlüsselt" : ""}
              </dd>
              <dt>Basiswährung</dt>
              <dd className="text-fg">{preview.base_currency}</dd>
              <dt>Wertpapiere</dt>
              <dd className="text-fg">
                {preview.counts.securities} ({preview.counts.prices.toLocaleString("de-DE")} Kurse)
              </dd>
              <dt>Konten / Depots</dt>
              <dd className="text-fg">
                {preview.counts.accounts} / {preview.counts.portfolios}
              </dd>
              <dt>Buchungen</dt>
              <dd className="text-fg">{preview.counts.transactions.toLocaleString("de-DE")}</dd>
              <dt>Klassifizierungen</dt>
              <dd className="text-fg">{preview.counts.taxonomies}</dd>
            </dl>
            {preview.warnings.length > 0 && (
              <ul className="mt-3 list-disc pl-5 text-xs text-warn">
                {preview.warnings.slice(0, 8).map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            )}
          </div>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => send(true)} disabled={!file || busy}>
            Prüfen
          </Button>
          <Button onClick={() => send(false)} disabled={!file || busy}>
            {busy && <Spinner className="size-4 text-current" />}
            Importieren
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

function CreateDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const [name, setName] = React.useState("");
  const [currency, setCurrency] = React.useState("EUR");
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [error, setError] = React.useState<unknown>(null);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const c = await unwrap(api.POST("/clients", { body: { name, base_currency: currency } }));
      await qc.invalidateQueries({ queryKey: qk.clients });
      onOpenChange(false);
      navigate({ to: "/c/$clientId/accounts", params: { clientId: c.id } });
    } catch (err) {
      setError(err);
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Neues Portfolio">
      <form onSubmit={submit} className="space-y-4">
        <Field label="Name">
          <Input value={name} onChange={(e) => setName(e.target.value)} required autoFocus />
        </Field>
        <Field label="Basiswährung">
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={3} required />
        </Field>
        {error != null && <ErrorBox error={error} />}
        <div className="flex justify-end">
          <Button type="submit">Anlegen</Button>
        </div>
      </form>
    </Dialog>
  );
}

export function ClientsPage() {
  const { data, isLoading } = useClients();
  const [importOpen, setImportOpen] = React.useState(false);
  const [createOpen, setCreateOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const qc = useQueryClient();
  const navigate = useNavigate();

  const demo = async () => {
    setBusy(true);
    try {
      const c = await unwrap(api.POST("/clients/demo"));
      await qc.invalidateQueries({ queryKey: qk.clients });
      navigate({ to: "/c/$clientId", params: { clientId: c.id } });
    } finally {
      setBusy(false);
    }
  };

  const remove = async (c: Schemas["ClientDto"]) => {
    if (!confirm(`Portfolio „${c.name}“ mit allen Daten unwiderruflich löschen?`)) return;
    await unwrap(api.DELETE("/clients/{id}", { params: { path: { id: c.id } } }));
    qc.invalidateQueries({ queryKey: qk.clients });
  };

  return (
    <div className="min-h-dvh bg-bg">
      <div className="mx-auto max-w-3xl px-4 py-8 sm:py-14">
        <div className="mb-8 flex items-center gap-3">
          <Logo className="size-10" />
          <div>
            <h1 className="text-xl font-semibold tracking-tight">Deine Portfolios</h1>
            <p className="text-sm text-muted">Ein Portfolio entspricht einer Portfolio-Performance-Datei.</p>
          </div>
        </div>
        <div className="mb-6 grid gap-3 sm:grid-cols-3">
          <button
            type="button"
            onClick={() => setImportOpen(true)}
            className="flex cursor-pointer flex-col gap-2 rounded-xl border border-border bg-surface p-4 text-left hover:border-accent"
          >
            <FileUp className="size-5 text-accent" />
            <span className="text-sm font-medium">PP-Datei importieren</span>
            <span className="text-xs text-muted">Migration aus Portfolio Performance</span>
          </button>
          <button
            type="button"
            onClick={demo}
            disabled={busy}
            className="flex cursor-pointer flex-col gap-2 rounded-xl border border-border bg-surface p-4 text-left hover:border-accent"
          >
            {busy ? <Spinner className="size-5" /> : <Sparkles className="size-5 text-accent" />}
            <span className="text-sm font-medium">Demo-Portfolio</span>
            <span className="text-xs text-muted">5 Jahre Beispieldaten zum Ausprobieren</span>
          </button>
          <button
            type="button"
            onClick={() => setCreateOpen(true)}
            className="flex cursor-pointer flex-col gap-2 rounded-xl border border-border bg-surface p-4 text-left hover:border-accent"
          >
            <Plus className="size-5 text-accent" />
            <span className="text-sm font-medium">Leeres Portfolio</span>
            <span className="text-xs text-muted">Von Grund auf neu beginnen</span>
          </button>
        </div>
        <Card>
          {isLoading ? (
            <div className="flex justify-center p-8">
              <Spinner />
            </div>
          ) : !data?.length ? (
            <EmptyState title="Noch keine Portfolios">
              Importiere deine PP-Datei oder starte mit dem Demo-Portfolio.
            </EmptyState>
          ) : (
            <ul className="divide-y divide-border">
              {data.map((c) => (
                <li key={c.id} className="flex items-center gap-3 px-4 py-3 hover:bg-surface-2/60">
                  <Link to="/c/$clientId" params={{ clientId: c.id }} className="min-w-0 flex-1">
                    <div className="truncate font-medium">{c.name}</div>
                    <div className="text-xs text-muted">
                      {c.base_currency} · angelegt {date(c.created_at)}
                    </div>
                  </Link>
                  <Button variant="ghost" size="icon" onClick={() => remove(c)} aria-label="Löschen" title="Löschen">
                    <Trash2 className="text-muted" />
                  </Button>
                  <Link to="/c/$clientId" params={{ clientId: c.id }} aria-label="Öffnen">
                    <ChevronRight className="size-4 text-muted" />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
      <ImportDialog open={importOpen} onOpenChange={setImportOpen} />
      <CreateDialog open={createOpen} onOpenChange={setCreateOpen} />
    </div>
  );
}
