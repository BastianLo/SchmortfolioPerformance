# Changelog

## 1.0.0

Erste vollständige Version.

### Migration und Kompatibilität
- Import von Portfolio-Performance-Dateien: XML (Pfad- und ID-Referenzen), Binärformat (Protobuf),
  verschlüsselt (AES-128/256), ZIP; Migration alter Dateiversionen.
- Übernahme von Wertpapieren mit Kursen, Konten, Depots, Buchungen samt Gebühren, Steuern und
  Devisenkursen sowie Klassifizierungen, Watchlists und Sparplänen.
- Export als PP-XML; Import → Export → Import ist verlustfrei.

### Auswertungen
- Rechenkern mit denselben Ergebnissen wie PP (Golden Tests gegen PPs Testsuite): Vermögensaufstellung,
  FIFO- und Durchschnittseinstand, realisierte und nicht realisierte Gewinne, TTWROR, IZF, Drawdown, Volatilität.
- Übersicht, Vermögensaufstellung mit Stichtag, Performance mit Monatsrenditen, Erträge,
  Klassifizierungen mit Rebalancing.
- Filter auf einzelne Depots und Konten (Semantik wie PP-Filter).

### Erfassung und Daten
- Buchungen aller Arten, auch in Fremdwährung, Umbuchungen, Depotwechsel, Ein- und Auslieferungen; Bearbeiten und Löschen.
- Sparpläne mit automatischer Ausführung.
- Kursquellen: Yahoo Finance, JSON-URL, HTML-Tabelle, CoinGecko, Alpha Vantage; manuelle Kurse und Einfügen aus Excel.
- EZB-Wechselkurse, ergänzt um Kurse aus den Buchungen.
- PDF-Import mit YAML-Regeln (Trade Republic, Scalable Capital, ING, DKB) und eigenen Regeln; CSV-Import mit
  Spaltenzuordnung; Duplikaterkennung; Belege als Anhang.

### Betrieb
- Anmeldung lokal oder per OIDC (PKCE, Gruppen, Admin-Gruppe, Abmeldung beim Provider), mehrere Benutzer.
- API-Tokens (nur lesend oder mit Schreibzugriff), OpenAPI-Dokumentation.
- Tägliche Backups, Docker-Image (amd64/arm64), PWA mit Offline-Lesezugriff, mobile Ansicht, dunkles Design.
