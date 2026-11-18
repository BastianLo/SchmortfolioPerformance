# Eigene PDF-Regeln schreiben

SchmortfolioPerformance liest Bankbelege mit Regeln im YAML-Format. Mitgeliefert sind Regeln für
Trade Republic, Scalable Capital, ING und DKB (`crates/pp-import/rules`). Für weitere Banken legst du
eine eigene Datei in `data/pdf-rules/<bank>.yaml` ab. Sie wird beim nächsten Import automatisch geladen,
ohne Neustart, und hat Vorrang vor den mitgelieferten Regeln.

## Ablauf

1. Beleg im Import hochladen. Wird er nicht erkannt, zeigt **„Text“** den extrahierten Text an.
2. Den Text in eine Datei kopieren und die Regel dagegen entwickeln (z. B. mit regex101.com, Modus „Rust“).
3. Regeldatei in `data/pdf-rules` speichern und den Beleg erneut hochladen.

## Aufbau

```yaml
bank: Beispielbank                       # Anzeigename
detect:                                  # mindestens ein Muster muss im Text vorkommen
  - 'Beispielbank AG'
documents:
  - name: Wertpapierabrechnung
    match: '^Wertpapierabrechnung (Kauf|Verkauf)'   # wählt den Dokumenttyp
    kind:
      capture: side                      # Wert der Gruppe "side" …
      map: { Kauf: BUY, Verkauf: SELL }  # … wird auf die Buchungsart abgebildet
    derive_fees: false                   # Gebühren = Betrag − Kurswert, falls keine Gebührenzeile
    fields:
      - '^Wertpapierabrechnung (?<side>Kauf|Verkauf)'
      - '^ISIN (?<isin>[A-Z]{2}[A-Z0-9]{9}\d)'
      - '^Stück (?<shares>[\d.,]+) (?<name>.+)$'
      - '^Handelstag (?<date>\d{2}\.\d{2}\.\d{4})'
      - '^Endbetrag (?<amount>[\d.,]+) (?<amount_cur>[A-Z]{3})'
      - pattern: '^Provision (?<fee>[\d.,]+) (?<fee_cur>[A-Z]{3})'
        optional: true                   # darf fehlen
      - pattern: '^(?:Kapitalertragsteuer|Solidaritätszuschlag) (?<tax>[\d.,]+) (?<tax_cur>[A-Z]{3})'
        optional: true
```

Jeder Eintrag unter `fields` ist ein regulärer Ausdruck (Rust-Syntax, Mehrzeilenmodus: `^` und `$` gelten
pro Zeile). Ohne `optional: true` muss das Muster vorkommen, sonst gilt der Dokumenttyp als nicht erkannt.
Mit `last: true` wird der letzte statt des ersten Treffers verwendet (z. B. bei mehreren „Gesamt“-Zeilen).

Statt `capture` kann `kind` auch einen festen Wert haben: `kind: { fixed: DIVIDENDS }`.
Buchungsarten: `BUY`, `SELL`, `DIVIDENDS`, `INTEREST`, `DEPOSIT`, `REMOVAL`, `FEES`, `TAXES`, `TAX_REFUND`.

## Felder (benannte Gruppen)

| Gruppe | Bedeutung |
|---|---|
| `date`, `time` | Buchungsdatum (`TT.MM.JJJJ` oder `JJJJ-MM-TT`), Uhrzeit optional |
| `name`, `isin`, `wkn` | Wertpapier; die Zuordnung erfolgt über ISIN, dann WKN, dann Name |
| `shares` | Stückzahl |
| `amount` | Betrag, der dem Konto belastet oder gutgeschrieben wird |
| `amount_cur` / `currency` | Währung des Betrags (Standard EUR) |
| `gross` | Kurswert vor Gebühren und Steuern (für `derive_fees`) |
| `fee…` | Gebühren. Alle Treffer aller Muster werden summiert, z. B. `fee`, `fee2` |
| `tax…` | Steuern, ebenfalls summiert |
| `refund…` | Steuererstattungen (verringern die Steuern) |
| `<gruppe>_cur` | Währung eines Werts. Abweichende Währungen werden mit dem Devisenkurs umgerechnet |
| `fx` | Devisenkurs Fremdwährung → Kontowährung (1 USD = 0,92 EUR) |
| `fxi` | Devisenkurs in umgekehrter Notation (1 EUR = 1,09 USD), wird invertiert |
| `sec_currency` | Währung des Wertpapiers, z. B. aus „Dividende pro Stück 0,24 USD“ |

Zahlen werden im deutschen Format (`1.234,56`) und im englischen Format gelesen. Ein nachgestelltes `-` oder `+` wird ignoriert.

## Tests für mitgelieferte Regeln

Neue Regeln für das Projekt brauchen Beispieltexte (anonymisiert) unter `testdata/pdf/<bank>/` und einen
Eintrag in `crates/pp-import/tests/pdf_rules.rs`. Ausführen mit:

```bash
cargo test -p pp-import --test pdf_rules
```
