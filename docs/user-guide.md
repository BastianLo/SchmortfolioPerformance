# Benutzerhandbuch

## Installation

```bash
docker compose up -d
```

Die App läuft danach unter `http://<server>:8080`. Beim ersten Aufruf legst du den Administrator an
(oder vorab per `PP_ADMIN_USERNAME` / `PP_ADMIN_PASSWORD`). Alle Daten liegen im Verzeichnis `data`:

| Pfad | Inhalt |
|---|---|
| `data/schmortfolio.db` | Datenbank (SQLite) |
| `data/backups/` | tägliche Backups (Anzahl: `PP_BACKUP_KEEP`, Standard 14) |
| `data/files/` | Original-PDFs der importierten Belege |
| `data/pdf-rules/` | eigene PDF-Regeln (siehe [pdf-rules.md](pdf-rules.md)) |

Für den Betrieb hinter einem Reverse Proxy (Traefik, Caddy, nginx) `PP_PUBLIC_URL` auf die externe
HTTPS-Adresse setzen. Cookies werden dann automatisch als `Secure` markiert.

### Als App installieren (PWA)

Am Handy die Seite im Browser öffnen und „Zum Home-Bildschirm“ wählen (iOS: Teilen-Menü, Android: Menü
„App installieren“). Zuletzt geladene Ansichten sind auch offline lesbar.

## Umstieg von Portfolio Performance

1. **Portfolios → PP-Datei importieren** und die `.xml`- oder `.portfolio`-Datei wählen, auch verschlüsselt.
2. **Prüfen** zeigt vorab, was übernommen wird.
3. Nach dem Import unter **Einstellungen → Wechselkurse** prüfen, ob alle Währungen abgedeckt sind. Die EZB-Kurse
   werden beim Start automatisch geladen.
4. Für automatische Kurse unter **Wertpapiere → Bearbeiten** die Kursquelle setzen (Yahoo: Ticker-Symbol). PP-Einstellungen
   für Yahoo, JSON und HTML-Tabellen werden übernommen.

Übernommen werden Wertpapiere mit Kurshistorie, Konten, Depots, alle Buchungen inklusive Gebühren, Steuern und
Devisenkursen, Klassifizierungen, Watchlists und Sparpläne. Nicht übernommen werden Dashboards, gespeicherte
Diagramm-Einstellungen und Lesezeichen.

Rückweg: **Einstellungen → Als PP-Datei exportieren** erzeugt eine XML-Datei, die PP öffnen kann.

## Funktionen

- **Übersicht**: Vermögen, Entwicklung im Zeitraum, Kennzahlen (TTWROR, IZF, Drawdown, Volatilität), Aufteilung,
  größte Positionen, letzte Erträge. Zeitraum und Filter oben rechts.
- **Filter**: Über „Alle“ einzelne Depots oder Konten wählen. Alle Auswertungen gelten dann nur für diese Auswahl,
  mit derselben Logik wie die Filter in PP: Käufe aus nicht gewählten Konten zählen als Einlieferung.
- **Vermögensaufstellung**: Bestand zu einem Stichtag mit Einstand (FIFO), Gewinn, IZF und Erträgen je Wertpapier.
- **Performance**: Verlauf (TTWROR, Vermögen, Drawdown), Monats- und Jahresrenditen, Berechnung der Wertänderung.
- **Buchungen**: Alle Buchungen mit Suche und Filtern. Ein Klick auf eine Buchung öffnet sie zum Bearbeiten.
  Fremdwährungskäufe brauchen den Devisenkurs der Abrechnung.
- **Wertpapiere**: Stammdaten, Kursquelle, Kurshistorie (auch per Einfügen aus Excel), Buchungen und Kennzahlen.
- **Sparpläne**: Fällige Ausführungen werden alle 6 Stunden automatisch gebucht, sofern „automatisch“ aktiv ist,
  sonst über „Buchen“.
- **Import**: PDF-Belege (Trade Republic, Scalable Capital, ING, DKB) und CSV-Dateien. Mögliche Duplikate
  werden erkannt, das Original-PDF wird an die Buchung angehängt (Büroklammer in der Buchungsliste).
- **Klassifizierungen**: Struktur, Soll-Gewichte und Rebalancing (Differenz zwischen Soll und Ist).

## Anmeldung per SSO (OIDC)

Beim Identity Provider einen „Confidential Client“ anlegen mit der Redirect-URI
`https://<deine-domain>/api/v1/auth/oidc/callback`, dann setzen:

```yaml
PP_PUBLIC_URL: https://portfolio.example.org
PP_OIDC_ISSUER: https://auth.example.org/application/o/schmortfolio/
PP_OIDC_CLIENT_ID: schmortfolio
PP_OIDC_CLIENT_SECRET: …
# optional
PP_OIDC_ALLOWED_GROUPS: portfolio-users
PP_OIDC_ADMIN_GROUPS: portfolio-admins
PP_DISABLE_LOCAL_LOGIN: "true"
```

Beispiele für die Issuer-URL:

- **Authentik:** `https://auth.example.org/application/o/<slug>/`
- **Keycloak:** `https://kc.example.org/realms/<realm>`
- **Authelia:** `https://auth.example.org`
- **Pocket ID:** `https://id.example.org`

Bei Keycloak liegen Rollen oft unter `realm_access.roles`; dann `PP_OIDC_GROUPS_CLAIM: realm_access.roles` setzen.

## Integrationen (API)

Unter **Einstellungen → API-Tokens** ein Token erzeugen (standardmäßig nur lesend). Die API-Dokumentation steht
unter `/api/docs`. Beispiel für einen REST-Sensor in Home Assistant:

```yaml
sensor:
  - platform: rest
    name: Depotwert
    resource: https://portfolio.example.org/api/v1/clients/<client-id>/reports/dashboard
    headers:
      Authorization: !secret schmortfolio_token   # "Bearer pp_…"
    value_template: "{{ (value_json.total / 100) | round(2) }}"
    unit_of_measurement: "EUR"
    json_attributes_path: "$.kpis"
    json_attributes: [ttwror, irr, max_drawdown]
    scan_interval: 3600
```

Die `client-id` steht in der Adresszeile (`/c/<client-id>`). Geldbeträge liefert die API in Cent, Stückzahlen und
Kurse mit dem Faktor 10⁸.

## Backup und Wiederherstellung

Unter **Einstellungen → Backups** (nur für Administratoren) lassen sich Backups anlegen und herunterladen. Zum
Wiederherstellen den Container stoppen, die Backup-Datei als `data/schmortfolio.db` ablegen (vorher
`schmortfolio.db-wal` und `-shm` löschen) und den Container wieder starten.
