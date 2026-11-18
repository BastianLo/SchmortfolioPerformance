import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Download, RefreshCw } from "lucide-react";
import * as React from "react";
import { api, unwrap } from "@/api/client";
import { qk, useClient, useExchangeRates, useMe } from "@/api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { Field, Input } from "@/components/ui/input";
import { ErrorBox, PageHeader, Spinner } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { date } from "@/lib/format";

export function SettingsPage() {
  const clientId = useClientId();
  const { data: client } = useClient(clientId);
  const { data: fx, refetch } = useExchangeRates();
  const { data: me } = useMe();
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [name, setName] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<unknown>(null);
  React.useEffect(() => setName(client?.name ?? ""), [client?.name]);

  const rename = async () => {
    await unwrap(api.PATCH("/clients/{id}", { params: { path: { id: clientId } }, body: { name } }));
    qc.invalidateQueries({ queryKey: qk.clients });
    qc.invalidateQueries({ queryKey: qk.client(clientId) });
  };
  const updateFx = async () => {
    setBusy(true);
    setError(null);
    try {
      await unwrap(api.POST("/exchange-rates/update"));
      await refetch();
      qc.invalidateQueries({ queryKey: qk.client(clientId) });
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };
  const remove = async () => {
    if (!confirm(`Portfolio „${client?.name}“ unwiderruflich löschen?`)) return;
    await unwrap(api.DELETE("/clients/{id}", { params: { path: { id: clientId } } }));
    await qc.invalidateQueries({ queryKey: qk.clients });
    navigate({ to: "/clients" });
  };

  return (
    <>
      <PageHeader title="Einstellungen" />
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader title="Portfolio" />
          <CardBody className="space-y-4">
            <Field label="Name">
              <div className="flex gap-2">
                <Input value={name} onChange={(e) => setName(e.target.value)} />
                <Button variant="outline" onClick={rename} disabled={!name || name === client?.name}>
                  Speichern
                </Button>
              </div>
            </Field>
            {client && (
              <dl className="grid grid-cols-2 gap-y-1 text-sm">
                <dt className="text-muted">Basiswährung</dt>
                <dd>{client.base_currency}</dd>
                <dt className="text-muted">Währungen</dt>
                <dd>{client.currencies.join(", ") || "–"}</dd>
                <dt className="text-muted">Wertpapiere / Kurse</dt>
                <dd>
                  {client.counts.securities} / {client.counts.prices.toLocaleString("de-DE")}
                </dd>
                <dt className="text-muted">Buchungen</dt>
                <dd>{client.counts.transactions.toLocaleString("de-DE")}</dd>
                <dt className="text-muted">PP-Dateiversion</dt>
                <dd>{client.pp_version}</dd>
              </dl>
            )}
            <div className="flex flex-wrap gap-2">
              <Button variant="outline" asChild>
                <a href={`/api/v1/clients/${clientId}/export/pp-xml`} download>
                  <Download /> Als PP-Datei exportieren
                </a>
              </Button>
              <Button variant="danger" onClick={remove}>
                Portfolio löschen
              </Button>
            </div>
          </CardBody>
        </Card>
        <Card>
          <CardHeader title="Wechselkurse" subtitle="Referenzkurse der Europäischen Zentralbank" />
          <CardBody className="space-y-3 text-sm">
            <dl className="grid grid-cols-2 gap-y-1">
              <dt className="text-muted">Kurse gespeichert</dt>
              <dd>{fx?.rates.toLocaleString("de-DE") ?? "–"}</dd>
              <dt className="text-muted">Letzter Kurs vom</dt>
              <dd>{date(fx?.last_date)}</dd>
              <dt className="text-muted">Letzte Aktualisierung</dt>
              <dd>{date(fx?.last_update)}</dd>
              <dt className="text-muted">Währungspaare</dt>
              <dd>{fx?.currencies.length ?? 0}</dd>
            </dl>
            {client?.missing_exchange_rates.length ? (
              <p className="text-warn">Fehlend: {client.missing_exchange_rates.join(", ")}</p>
            ) : null}
            {error != null && <ErrorBox error={error} />}
            {me?.is_admin && (
              <Button variant="outline" onClick={updateFx} disabled={busy}>
                {busy ? <Spinner className="size-4" /> : <RefreshCw />}
                Jetzt aktualisieren
              </Button>
            )}
          </CardBody>
        </Card>
        {me?.is_admin && <FeedSettingsCard />}
        <Card>
          <CardHeader title="Über" />
          <CardBody className="space-y-2 text-sm text-muted">
            <p>SchmortfolioPerformance – selbsthostbare Neuimplementierung von Portfolio Performance.</p>
            <p>
              Angemeldet als <span className="text-fg">{me?.username}</span>
              {me?.sso && " (SSO)"}.
            </p>
            <p>
              API-Dokumentation:{" "}
              <a href="/api/docs" className="text-accent" target="_blank" rel="noreferrer">
                /api/docs
              </a>
            </p>
          </CardBody>
        </Card>
      </div>
    </>
  );
}

function FeedSettingsCard() {
  const [key, setKey] = React.useState("");
  const [isSet, setIsSet] = React.useState(false);
  const [saved, setSaved] = React.useState(false);
  React.useEffect(() => {
    unwrap(api.GET("/settings/feeds")).then((s) => setIsSet(!!s.alphavantage_api_key_set));
  }, []);
  const save = async () => {
    await unwrap(api.PUT("/settings/feeds", { body: { alphavantage_api_key: key } }));
    setIsSet(!!key);
    setKey("");
    setSaved(true);
  };
  return (
    <Card>
      <CardHeader title="Kursquellen" subtitle="Kurse werden alle 6 Stunden automatisch aktualisiert." />
      <CardBody className="space-y-3 text-sm">
        <Field
          label="Alpha Vantage API-Key"
          hint={
            isSet
              ? "Ein Key ist hinterlegt. Leer speichern entfernt ihn."
              : "Kostenlos unter alphavantage.co erhältlich."
          }
        >
          <div className="flex gap-2">
            <Input
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder={isSet ? "••••••••" : ""}
            />
            <Button variant="outline" onClick={save}>
              Speichern
            </Button>
          </div>
        </Field>
        {saved && <p className="text-xs text-pos">Gespeichert.</p>}
      </CardBody>
    </Card>
  );
}
