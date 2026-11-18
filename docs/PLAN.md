# SchmortfolioPerformance – Umsetzungsplan (Variante A: Rust + React)

> Status: Entwurf v1 · Stand: 2026-11-18

## 1. Ziel und Abgrenzung

**Ziel:** Eine selbsthostbare Neuimplementierung von Portfolio Performance (PP) als Web-App mit PWA.
Sie soll moderner, schneller und mobil nutzbar sein und PP-Dateien verlustfrei importieren können.

**Annahmen** (offene Fragen mit sinnvollen Standards belegt, später änderbar):

| Frage | Annahme für v1 |
|---|---|
| Benutzer | Mehrbenutzerfähig im Datenmodell (`user` → `client`). Login lokal oder per OIDC (SSO). Haushalt mit Freigaben kommt später. |
| Offline | v1: Offline-**Lesen** über den Service-Worker-Cache. Offline-Erfassung erst ab M8 (local-first über WASM-Rechenkern). |
| Priorität | Zuerst **PP-Import + Auswertungen** (das ist der Migrationsgrund), danach Erfassung, Kurse und PDF-Import. |
| Datenbank | SQLite als Standard (eine Datei, ein Container), Postgres optional. |
| Lizenz | EPL-2.0 oder MIT/Apache-2.0. Entscheidung vor M5 (PDF-Regeln aus PP portieren → EPL-kompatibel bleiben). |

**Nicht-Ziele für v1:** Desktop-App, Broker-API-Anbindung (Trading), Cloud-SaaS-Betrieb.

---

## 2. Architekturüberblick

```
┌──────────────────────────── Browser / PWA ─────────────────────────────┐
│ React + TanStack Router/Query/Table · shadcn/ui · ECharts/Lightweight  │
│ Service Worker (Workbox): App-Shell + API-Cache (stale-while-revalidate)│
│ [später] pp-core als WASM für Offline-Berechnungen                     │
└───────────────▲──────────────────────────────────────────▲─────────────┘
                │ REST/JSON (OpenAPI, generierter TS-Client)│ SSE (Job-Fortschritt)
┌───────────────┴──────────────────────────────────────────┴─────────────┐
│                       pp-server (Axum, eine Binary)                    │
│  api ─ auth ─ static (eingebettetes Frontend)                          │
│  services: Buchungen · Auswertungen · Import · Kurse · Reports         │
│  jobs: Kurs-Updates, Wechselkurse, Neuberechnung (tokio + Cron)        │
├────────────────────────────────────────────────────────────────────────┤
│ pp-core (reine Fachlogik, kein IO)  │ pp-import (XML/Protobuf/PDF/CSV) │
│ pp-quotes (Feed-Adapter)            │ pp-db (SQLx, Migrationen)        │
└─────────────────────────────────────┴──────────────────────────────────┘
                │
        SQLite (WAL) / Postgres          Volumes: /data (db, uploads, backups)
```

**Grundprinzipien**
1. **`pp-core` ist rein:** keine Datenbank, kein Netzwerk, keine Uhr. Eingabe sind Daten, Ausgabe sind Ergebnisse. Dadurch ist es deterministisch testbar und nach WASM kompilierbar.
2. **Ganzzahl-Arithmetik:** Geld als `i64` in Minor Units ×100 (wie PP), Stückzahl als `i64` ×10⁸, Kurse als `i64` ×10⁸. Wertberechnungen (Stück × Kurs, FX) in `rust_decimal` mit Rundung wie PP (HALF_UP). Renditen (TTWROR, IRR) in `f64`, genau wie PP, damit die Ergebnisse identisch sind. Kein `f64` für Geldbeträge.
3. **Buchungen sind die Wahrheit, alles andere ist abgeleitet:** Bestände, Einstandswerte und Performance werden aus den Buchungen berechnet und gecacht (Snapshot-Cache, invalidiert ab dem frühesten geänderten Datum).
4. **API-first:** Die OpenAPI-Spec wird aus dem Rust-Code erzeugt (`utoipa`), der TS-Client daraus generiert. Die öffentliche API ist zugleich ein Feature (Home Assistant, Skripte).

---

## 3. Tech-Stack

| Bereich | Wahl | Begründung |
|---|---|---|
| Sprache Backend | Rust (stable, Edition 2024) | Typsicherheit, Leistung, eine Binary |
| HTTP | `axum` + `tower-http` (Kompression, CORS, Tracing) | Standard im Tokio-Ökosystem |
| DB | `sqlx` (SQLite/Postgres), Migrationen über `sqlx migrate` | zur Compile-Zeit geprüfte Queries |
| Serialisierung | `serde`, `serde_json` | – |
| OpenAPI | `utoipa` + `utoipa-scalar` (Doku-UI) | Spec aus dem Code |
| Dezimal | `rust_decimal` | Renditen, FX-Umrechnung |
| Datum | `jiff` oder `chrono` | Handelstage, Zeitzonen |
| Protobuf | `prost` | PP-Binärformat |
| XML | `quick-xml` (Streaming) | große PP-XML-Dateien |
| PDF-Text | `pdfium-render` (statisch gelinkt) oder `lopdf`/`pdf-extract` | Textextraktion für Importer |
| Regex | `regex` (+ `fancy-regex` für Lookarounds aus PP) | PDF-Regeln |
| HTTP-Client | `reqwest` (rustls) | Kurs-Feeds |
| Jobs | `tokio` + `tokio-cron-scheduler`, Job-Tabelle in der DB | ohne externe Queue |
| Auth | `argon2`, Session-Cookies (`tower-sessions`), optional OIDC (`openidconnect`) | Self-Hosting + SSO (Authelia/Authentik) |
| Logging | `tracing`, `tracing-subscriber` (JSON) | – |
| Frontend | React 19, TypeScript, Vite | Ökosystem |
| Routing/Data | TanStack Router, TanStack Query, TanStack Table + Virtual | typsicheres Routing, Caching, große Tabellen |
| UI | Tailwind CSS 4, shadcn/ui (Radix), lucide-icons | modern, anpassbar, Dark Mode |
| Charts | Apache ECharts (Auswertungen), TradingView Lightweight Charts (Kursverläufe) | Leistung bei langen Zeitreihen |
| Formulare | react-hook-form + zod | Buchungsmasken |
| i18n | i18next (de/en ab Start) | – |
| PWA | vite-plugin-pwa (Workbox) | Manifest, Service Worker |
| API-Client | `openapi-typescript` + `openapi-fetch` | generiert, schlank |
| Tests | `cargo nextest`, `insta` (Snapshots), `proptest`; Vitest; Playwright | – |
| Tooling | pnpm, Biome (Lint/Format), `cargo clippy`, `cargo deny` | – |

---

## 4. Repository-Struktur

```
/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── pp-core/               # Domänenmodell + Berechnungen (no IO, wasm-fähig)
│   │   ├── model/             # Security, Account, Portfolio, Transaction, Taxonomy …
│   │   ├── money.rs           # Money, Quantity, Price, CurrencyConverter
│   │   ├── snapshot/          # Bestände/Bewertung zu Stichtag
│   │   ├── performance/       # TTWROR, IRR, Drawdown, Volatilität, Sharpe …
│   │   ├── lots/              # FIFO/Durchschnitt, realisierte/unrealisierte G/V
│   │   └── tests/fixtures/
│   ├── pp-import/             # PP-XML, PP-Protobuf, CSV, PDF-Regelengine
│   │   ├── pp_xml/
│   │   ├── pp_proto/          # client.proto (aus PP übernommen, Lizenz beachten)
│   │   ├── csv/
│   │   └── pdf/               # Engine + rules/*.yaml
│   ├── pp-quotes/             # Feed-Adapter (Yahoo, EZB, Alpha Vantage, JSON, HTML …)
│   ├── pp-db/                 # Repository-Layer, Migrationen
│   ├── pp-server/             # Axum-App, Auth, Jobs, statische Auslieferung
│   └── pp-wasm/               # (ab M8) wasm-bindgen-Wrapper um pp-core
├── web/                       # React-App (pnpm)
│   ├── src/routes/            # TanStack Router (dateibasiert)
│   ├── src/features/          # dashboard, securities, accounts, transactions, …
│   ├── src/components/ui/     # shadcn
│   └── src/api/               # generierter Client
├── testdata/                  # anonymisierte PP-Dateien + erwartete Werte
├── docker/                    # Dockerfile, compose.yaml
├── docs/                      # PLAN.md, ADRs, API-Doku, Nutzer-Doku
└── .github/workflows/         # CI
```

---

## 5. Domänenmodell (an PP angelehnt)

Das Modell bildet PP 1:1 ab, damit der Import verlustfrei ist, und erweitert es nur additiv.

| Entität | Wichtige Felder |
|---|---|
| `Client` | Ein „Portfolio-Dokument“ (entspricht einer PP-Datei); Basiswährung, Einstellungen. Ein User kann mehrere Clients haben. |
| `Security` | uuid, Name, ISIN, WKN, Ticker, Währung, Ziel-Währung, Feed + Feed-URL, Latest-Feed, Kalender, `is_retired`, Notiz, Attribute (JSON), Properties |
| `SecurityPrice` | security_id, Datum, Kurs (i64 ×10⁸); separate Tabelle `latest_price` (high/low/volume) |
| `SecurityEvent` | Split, Dividenden-Ankündigung, Notiz |
| `Account` | Verrechnungskonto: Währung, `is_retired`, Attribute |
| `Portfolio` | Depot: Referenzkonto, `is_retired` |
| `AccountTransaction` | Typ ∈ {DEPOSIT, REMOVAL, INTEREST, INTEREST_CHARGE, DIVIDENDS, FEES, FEES_REFUND, TAXES, TAX_REFUND, BUY, SELL, TRANSFER_IN, TRANSFER_OUT}, Datum+Uhrzeit, Betrag, Währung, Security (optional), Stück (Dividende), Notiz, Quelle |
| `PortfolioTransaction` | Typ ∈ {BUY, SELL, TRANSFER_IN, TRANSFER_OUT, DELIVERY_INBOUND, DELIVERY_OUTBOUND}, Stück, Betrag, Security |
| `TransactionUnit` | GROSS_VALUE / FEE / TAX, Betrag, optional Fremdwährungsbetrag + Kurs |
| `CrossEntry` | verknüpft Buchungspaare (Kauf: Depot + Konto; Umbuchungen) |
| `Taxonomy` / `Classification` / `Assignment` | Baum mit Gewichten (Basis 10000), Farben, Rang |
| `Watchlist`, `InvestmentPlan`, `Bookmark`, `AttributeType`, `Dashboard`-Konfiguration | übernommen |
| `ExchangeRate` | Paar, Datum, Kurs (EZB + eigene) |

**Rust-Seite:** Transaktionen als `enum` mit typ-spezifischen Varianten. Ungültige Zustände (z. B. Kauf ohne Stückzahl) sind dadurch nicht darstellbar.

**DB-Seite:** normalisierte Tabellen, alle IDs als UUID (PP nutzt UUIDs, Import bleibt referenzstabil), `client_id` auf allen Tabellen (Mandantentrennung), `updated_at` + `version` für optimistisches Locking.

**Neu gegenüber PP (additiv):** Tags auf Buchungen, Dateianhänge (Original-PDF zur Buchung), Audit-Log, Import-Herkunft (`source`, `import_batch_id`), damit ein Import vollständig rückgängig gemacht werden kann.

---

## 6. Rechenkern (`pp-core`)

### 6.1 Bausteine
1. **CurrencyConverter:** Kurs zum Stichtag (letzter bekannter Kurs ≤ Datum), Kreuzkurse über EUR.
2. **PriceIndex pro Wertpapier:** sortierter Vektor (Datum, Kurs), Lookup per Binärsuche. Splits werden auf historische Kurse angewendet, wo PP das tut.
3. **ClientSnapshot (Stichtag):** Bestände je Depot und Konto, Marktwert, Einstand.
4. **Lots/Positionen:** FIFO (PP-Standard) und gleitender Durchschnitt; realisierte und unrealisierte Gewinne, Gebühren und Steuern getrennt ausgewiesen.
5. **PerformanceIndex (Zeitreihe):** tägliche Werte, Zu- und Abflüsse, kumulierte TTWROR, Delta. Basis für alle Charts.
6. **Kennzahlen:** IRR (Newton-Raphson mit Bisektion als Fallback), TTWROR (kumuliert/annualisiert), Max-Drawdown + Dauer, Volatilität, Semivarianz, Sharpe, Dividendenrendite, Personal-Rate-of-Return.
7. **Filter/Aggregation:** dieselben Berechnungen für beliebige Teilmengen (Depot, Konto, Wertpapier, Klassifikation, Watchlist). Entspricht den „Client-Filtern“ in PP.

### 6.2 Leistung (wo der Gewinn gegenüber PP entsteht)
- Ein Durchlauf über die chronologisch sortierten Buchungen erzeugt alle Tageswerte: O(Tage + Buchungen).
- **Inkrementelle Neuberechnung:** Ändert sich eine Buchung am Datum *d*, wird ab *d* neu gerechnet. Der Zustand davor kommt aus dem Snapshot-Cache.
- Parallelisierung pro Wertpapier/Filter mit `rayon`.
- Ergebnis-Cache im Server (`moka`), Schlüssel (client_id, filter_hash, data_version).
- **Ziel:** Ein typisches Portfolio (20 Jahre, 200 Wertpapiere, 10.000 Buchungen) braucht < 50 ms für den vollständigen Performance-Index, < 5 ms aus dem Cache.

### 6.3 Korrektheit
- **Golden Tests:** Echte (anonymisierte) PP-Dateien liegen unter `testdata/`. Erwartete Werte werden aus PP exportiert (CSV der Performance-/Vermögensaufstellung) und versioniert. Toleranz: Geldwerte exakt, Renditen ±0,0001 %.
- `proptest` für Invarianten: Summe der Bestände = Kauf − Verkauf; Einzahlung und Entnahme gleicher Höhe ⇒ TTWROR unverändert; Umbuchungen sind performance-neutral.

---

## 7. Migration aus Portfolio Performance

| Format | Erkennung | Umsetzung |
|---|---|---|
| Binär (`.portfolio`, Protobuf) | Header-Signatur `PPPBV1` | `prost` mit `client.proto` aus dem PP-Repo |
| XML (`.xml`, `.portfolio` unverschlüsselt) | `<client>` | Streaming mit `quick-xml`; XStream-Referenzen (`reference="../.."`, `id`/`ref`) über eine Referenztabelle auflösen; beide XML-Varianten (Pfad- und ID-Referenzen) unterstützen |
| Verschlüsselt | PP-Header für AES | Passwort-Dialog, Entschlüsselung wie in PP (Algorithmus vor M2 verifizieren), danach XML/Protobuf |
| ZIP-komprimiert | ZIP-Magic | entpacken, dann wie oben |

**Ablauf in der App:** Datei hochladen → **Dry-Run** mit Vorschau (Anzahl Wertpapiere, Buchungen, Konten, Warnungen zu nicht unterstützten Feldern) → Import in einen neuen Client → **Abgleichbericht** (Bestände und Gesamtwert zum heutigen Tag, mit den Werten aus PP zum Vergleich).

**Verlustfreiheit:** Unbekannte Felder und Properties landen als JSON in `extra`, damit sie nicht verloren gehen.

**Export zurück nach PP-XML** (ab M7): kein Lock-in, auch das ist ein Verkaufsargument.

---

## 8. API

- REST unter `/api/v1`, JSON, Spec unter `/api/openapi.json`, Doku-UI unter `/api/docs`.
- Ressourcen: `/clients/{id}/securities`, `/accounts`, `/portfolios`, `/transactions`, `/taxonomies`, `/watchlists`, `/reports/{performance|holdings|dividends|realized|taxonomy}`, `/imports`, `/jobs`.
- Auswertungen: `GET /clients/{id}/reports/performance?from=&to=&filter=&interval=day|week|month`.
- Lange Aufgaben (Import, Kurs-Update) laufen als Job: `POST` gibt eine `job_id` zurück, der Fortschritt kommt per SSE `/jobs/{id}/events`.
- Paginierung per Cursor, Filter per Query-Parameter, Fehler im RFC-9457-Format (`problem+json`).
- Personal-Access-Tokens für Skripte und Home Assistant (nur lesend oder lesend/schreibend).

---

## 9. Frontend und UX

### 9.1 Seiten (MVP)
1. **Dashboard:** Gesamtvermögen, Delta heute/YTD/gesamt, TTWROR- und IRR-Kacheln, Vermögensverlauf, Allokation (Donut), nächste Dividenden. Kacheln konfigurierbar (Drag & Drop).
2. **Vermögensaufstellung:** Bestände mit Gruppierung nach Depot/Klassifikation, Sortierung, Spaltenauswahl, Sparklines.
3. **Wertpapiere:** Liste + Detail (Kurschart mit Kauf-/Verkaufsmarkern, Buchungen, Kennzahlen, Events).
4. **Konten/Depots:** Buchungslisten mit virtualisierter Tabelle, Inline-Filter, Saldo-Verlauf.
5. **Buchung erfassen:** ein Dialog für alle Typen, Tastatur-optimiert, Wertpapier-Suche (ISIN/Name/Ticker), Gebühren und Steuern als Zeilen, Live-Vorschau der Gegenbuchung.
6. **Performance:** Zeitraum-Picker, Benchmark-Vergleich, Berechnungsdetails (wie das PP-„Performance-Berechnungs“-Panel), Drawdown-Chart.
7. **Dividenden/Erträge:** Monats-/Jahresübersicht (Heatmap), Prognose.
8. **Klassifizierungen:** Baum-Editor, Rebalancing-Ansicht (Soll/Ist).
9. **Import:** PP-Datei, CSV, PDF (Mehrfach-Upload, Vorschau, Duplikaterkennung).
10. **Einstellungen:** Kurs-Feeds, API-Keys, Benutzer, Backups, Sprache, Theme.

### 9.2 Mobile/PWA
- Mobile-first-Layouts: Bottom-Navigation (Dashboard · Bestände · Buchen · Mehr), Tabellen werden auf kleinen Bildschirmen zu Karten, Swipe-Gesten in Listen.
- Installierbar (Manifest, Icons, Splash), Service Worker mit App-Shell-Precache und `stale-while-revalidate` für Lese-Endpunkte. Offline-Banner, Schreibaktionen sind offline deaktiviert (bis M8).
- Web-Push (optional): Kursalarme, neue Dividenden.
- Ziel: Lighthouse PWA/Performance ≥ 90 auf Mittelklasse-Android, Time-to-Interactive < 2 s.

### 9.3 Neue Features gegenüber PP (Backlog, priorisiert)
1. Deklarative PDF-Regeln + Community-Repository
2. Duplikaterkennung beim Import (Hash aus Datum, Betrag, ISIN, Typ)
3. Belege als Anhang an Buchungen
4. Kursalarme und Benachrichtigungen (Push/Mail/ntfy)
5. Steuer-Hilfen DE (Teilfreistellung, Vorabpauschale, Freistellungsauftrag-Tracking)
6. Mehrbenutzer/Haushalt, Freigabe nur lesend
7. API + Home-Assistant-Integration
8. Command-Palette (⌘K) und Tastaturbedienung
9. Szenario-/Sparplan-Simulation

---

## 10. Kursdaten

- `trait QuoteFeed { async fn latest(..); async fn history(from, to); }`, ein Adapter pro Quelle.
- **v1-Adapter:** Yahoo Finance, EZB-Wechselkurse, Alpha Vantage, generischer JSON-Feed (JSONPath wie in PP), generische HTML-Tabelle (`scraper`), manuell/CSV.
- **Später:** Finnhub, Twelve Data, CoinGecko, onvista/ariva (sofern rechtlich und technisch stabil), PP-Portfolio-Report-API.
- **Scheduler:** Kurse beim Start und zu konfigurierbaren Zeiten; Rate-Limits pro Feed (`governor`); exponentielles Backoff; Fehlerstatus pro Wertpapier sichtbar im UI.
- Feed-Konfiguration aus dem PP-Import übernehmen (Feed-ID-Mapping-Tabelle).

---

## 11. PDF- und CSV-Import

**Engine:** PDF → Text (seitenweise, mit Layout-Erhaltung) → Bank erkennen (Erkennungs-Regex) → Dokumenttyp bestimmen → Felder extrahieren → normalisierte Buchungsvorschläge → Vorschau → Übernahme.

**Regelformat (YAML), Beispiel:**
```yaml
bank: Beispielbank
detect: ["Beispielbank AG", "Wertpapierabrechnung"]
documents:
  - type: buy
    when: "Wertpapierabrechnung Kauf"
    fields:
      isin:   'ISIN\s+(?<v>[A-Z]{2}[A-Z0-9]{9}\d)'
      shares: 'Stück\s+(?<v>[\d.,]+)'
      amount: 'Ausmachender Betrag\s+(?<v>[\d.,]+)\s+(?<cur>[A-Z]{3})'
      date:   'Schlusstag\s+(?<v>\d{2}\.\d{2}\.\d{4})'
      fee:    { pattern: 'Provision\s+(?<v>[\d.,]+)', optional: true }
      tax:    { pattern: 'Kapitalertragsteuer\s+(?<v>[\d.,]+)', optional: true }
```
- Komplexe Fälle (mehrere Buchungen pro Dokument, FX-Abschnitte) bekommen Blöcke und Wiederholungen in der Regel-DSL. Als Notausgang gibt es Parser in Rust.
- **Tests:** Pro Regel liegen anonymisierte Beispieltexte (`.txt`) mit erwartetem Ergebnis vor, analog zur PP-Testsuite. Ein CLI `pp-import test-rules` macht Beiträge ohne Rust-Kenntnisse möglich.
- **Reihenfolge der Banken:** nach Verbreitung (Trade Republic, Scalable, ING, comdirect, Consorsbank, DKB, flatex, Smartbroker, Baader, onvista, Interactive Brokers …).
- **CSV:** Mapping-Assistent mit Spaltenzuordnung, gespeicherten Profilen und Vorschau.

---

## 12. Sicherheit und Betrieb

- **Auth:** lokaler Login (Argon2id), Session-Cookie (HttpOnly, SameSite=Lax, Secure), CSRF-Schutz für mutierende Requests (SameSite + Origin-Prüfung), Header-Auth hinter einem Reverse Proxy (`trusted proxy`-Konfiguration) später.
- **OIDC (ab M1):** Authorization Code Flow mit PKCE, getestet gegen Authentik, Authelia, Keycloak, Pocket ID, Zitadel.
  - Konfiguration per Env: `PP_OIDC_ISSUER`, `PP_OIDC_CLIENT_ID`, `PP_OIDC_CLIENT_SECRET`, `PP_OIDC_REDIRECT_URL` (Standard `<PP_PUBLIC_URL>/api/v1/auth/oidc/callback`), `PP_OIDC_SCOPES` (Standard `openid profile email`), `PP_OIDC_BUTTON_LABEL`.
  - Benutzerzuordnung über `(issuer, sub)`; optional Verknüpfung mit vorhandenem lokalem Konto über die verifizierte E-Mail (`PP_OIDC_LINK_BY_EMAIL=true`).
  - Automatisches Anlegen neuer Benutzer (`PP_OIDC_AUTO_REGISTER`, Standard `true`).
  - Zugriff und Rollen über einen Gruppen-Claim: `PP_OIDC_GROUPS_CLAIM` (Standard `groups`), `PP_OIDC_ALLOWED_GROUPS`, `PP_OIDC_ADMIN_GROUPS`.
  - Lokaler Login abschaltbar (`PP_DISABLE_LOCAL_LOGIN=true`), dann nur SSO.
  - RP-initiated Logout (`end_session_endpoint`), falls der Provider es anbietet.
- Erster Start: Setup-Assistent legt den Admin an (oder Env-Variablen `PP_ADMIN_*`).
- Rate-Limit auf Login, Sicherheits-Header (CSP, HSTS über den Proxy).
- **Backups:** täglicher SQLite-Snapshot (`VACUUM INTO`) nach `/data/backups`, Rotation, Download im UI. Export als PP-XML und JSON.
- **Konfiguration:** Env-Variablen + optional `config.toml`; Healthcheck `/healthz`; Prometheus-Metriken `/metrics` (optional).

---

## 13. Deployment

- **Multi-Stage-Dockerfile:**
  1. `node`-Stage baut `web/` → `dist/`
  2. `rust`-Stage (`cargo-chef` für Layer-Caching) baut `pp-server` statisch (musl) und bettet `dist/` per `rust-embed` ein
  3. Runtime: `gcr.io/distroless/static` oder `scratch` + CA-Zertifikate
- Images für `linux/amd64` und `linux/arm64` (Raspberry Pi, NAS) über `docker buildx` in der CI; Veröffentlichung auf GHCR.
- `compose.yaml`: ein Service, Volume `/data`, Port 8080. Optional mit Postgres-Profil.
- Ziel: Image < 40 MB, RAM im Leerlauf < 50 MB.

---

## 14. Qualitätssicherung und CI

- **GitHub Actions:** `fmt` · `clippy -D warnings` · `nextest` · `cargo deny` · Frontend `biome check` · `tsc` · `vitest` · Playwright-E2E gegen den gebauten Container · Prüfung, ob der OpenAPI-Client aktuell ist · Docker-Build multi-arch (nur auf `main`/Tags).
- **Golden-Test-Korpus:** mindestens 5 reale PP-Dateien unterschiedlicher Größe und Struktur (FX, Splits, Umbuchungen, Sparpläne, Leerverkäufe/negative Bestände, Retired). Vor jedem Release müssen alle Abgleiche grün sein.
- Benchmarks mit `criterion` für den Rechenkern, Regressionen werden in der CI gemeldet.
- ADRs (`docs/adr/`) für wichtige Entscheidungen.

---

## 15. Meilensteine

| # | Meilenstein | Inhalt | Abnahmekriterium |
|---|---|---|---|
| **M0** | Fundament (1–2 Wo.) | Cargo-Workspace, `web/` mit Vite/React/Tailwind/shadcn, Axum-Hello-World mit eingebettetem Frontend, SQLx-Migrationen, CI, Dockerfile, Lizenzentscheidung, ADR-001 (Architektur) | `docker compose up` liefert die App aus, CI ist grün |
| **M1** | Domänenmodell + Persistenz (2–3 Wo.) | `pp-core`-Modell (Money/Quantity/Transaktions-Enums), DB-Schema, Repository-Layer, CRUD-API für Wertpapiere/Konten/Depots/Buchungen, OpenAPI + generierter Client, Auth (lokal + **OIDC**) | Buchungen per API anlegen und lesen; E2E-Login lokal und per OIDC |
| **M2** | PP-Import (3–4 Wo.) | XML- und Protobuf-Parser, Referenzauflösung, Verschlüsselung, Mapping → DB, Dry-Run + Vorschau-UI, Testkorpus | 5 Testdateien vollständig importiert; Anzahl der Entitäten stimmt 100 % |
| **M3** | Rechenkern I (3–4 Wo.) | Währungsumrechnung, Snapshot, Vermögensaufstellung, FIFO/Einstand, realisierte/unrealisierte G/V, Dividendenübersicht | Golden Tests: Bestände und Werte zum Stichtag identisch mit PP |
| **M4** | Rechenkern II + Auswertungs-UI (3–4 Wo.) | PerformanceIndex, TTWROR, IRR, Drawdown, Volatilität, Filter, Caching; Dashboard, Vermögensaufstellung, Performance-Seite, Wertpapier-Detail mit Charts | Golden Tests Performance innerhalb der Toleranz; Benchmark-Ziel aus 6.2 erreicht |
| **🚀 Alpha** | | **Nutzbar als „PP-Viewer im Browser“** | Eigene PP-Datei importieren und auswerten |
| **M5** | Erfassung + Kurse (3 Wo.) | Buchungsdialog (alle Typen), Bearbeiten/Löschen mit inkrementeller Neuberechnung, Quote-Feeds (Yahoo, EZB, AV, JSON, HTML), Scheduler, Job-UI mit SSE | Täglicher Betrieb ohne PP möglich |
| **M6** | Mobile + PWA (2 Wo.) | Responsive-Pass über alle Seiten, Bottom-Nav, Karten-Layouts, Manifest, Service Worker, Offline-Lesen, Install-Prompt | Lighthouse ≥ 90; installierbar auf iOS und Android |
| **M7** | Import-Ausbau (4+ Wo., fortlaufend) | PDF-Regelengine + DSL, erste 5–10 Banken, CSV-Assistent, Duplikaterkennung, Belege als Anhang, PP-XML-Export | Top-5-Banken mit Testabdeckung; Roundtrip-Test PP → App → PP |
| **🚀 Beta / v1.0** | | Klassifizierungen + Rebalancing, Watchlists, Sparpläne, Backups, Doku, Multi-Arch-Release auf GHCR | Öffentliches Release |
| **M8** | Danach | WASM-Rechenkern im Browser, Offline-Erfassung mit Sync, Mehrbenutzer/Haushalt mit Freigaben, Kursalarme/Push, Steuer-Features DE, Home-Assistant-Integration | – |

Die Zeitangaben gelten grob für eine Person in Teilzeit mit KI-Unterstützung. Die Reihenfolge ist wichtiger als die Dauer.

---

## 16. Risiken und Gegenmaßnahmen

| Risiko | Wirkung | Gegenmaßnahme |
|---|---|---|
| Rechenergebnisse weichen von PP ab | Vertrauensverlust | Golden-Test-Korpus ab M2, PP-Quellcode als Referenz für Randfälle (Splits, FX, Umbuchungen, Steuer-Rückerstattungen), transparente Berechnungsdetails im UI |
| Undokumentierte Eigenheiten im PP-Format (XStream-Referenzen, Versionen) | Import schlägt fehl | Streaming-Parser mit Referenztabelle, unbekannte Felder in `extra` aufheben, breiter Testkorpus aus verschiedenen PP-Versionen |
| PDF-Importer: großer Aufwand | Migration für viele unattraktiv | DSL + Community-Beiträge, Priorisierung nach Verbreitung, PP-Importer als Referenz (Lizenz!) |
| Kursquellen ändern APIs oder sperren | Kurse fehlen | Adapter-Architektur, mehrere Quellen, Fallback-Kette pro Wertpapier, Fehlerstatus im UI |
| Rust-Entwicklungstempo | Verzögerung | `pp-core` klein und rein halten, Boilerplate (CRUD) generisch, Frontend parallel gegen den OpenAPI-Mock entwickeln |
| Lizenz (EPL) bei Übernahme von PP-Artefakten | rechtliche Unsicherheit | Entscheidung in M0; übernommene Dateien (proto, Regex) klar kennzeichnen |

---

## 17. Nächste konkrete Schritte

1. Lizenz festlegen und `LICENSE` + `README.md` anlegen.
2. M0 umsetzen: Workspace-Gerüst, Frontend-Gerüst, Dockerfile, CI.
3. Testkorpus sammeln: eigene PP-Datei(en) anonymisieren (Skript dafür ist Teil von M2), Referenzwerte aus PP exportieren.
4. `client.proto` und die XML-Struktur aus dem aktuellen PP-Release analysieren → ADR-002 „Import-Mapping“.
