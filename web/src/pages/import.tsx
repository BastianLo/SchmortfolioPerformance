import { useNavigate } from "@tanstack/react-router";
import { AlertTriangle, FileText } from "lucide-react";
import * as React from "react";
import { postForm, type Schemas } from "@/api/client";
import { Dropzone, ImportResult, ImportReview, type ReviewRow } from "@/components/import-review";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader } from "@/components/ui/card";
import { ErrorBox, PageHeader, Segmented, Spinner } from "@/components/ui/misc";
import { useClientId } from "@/lib/client-id";
import { CsvImport } from "./import-csv";

function PdfImport() {
  const clientId = useClientId();
  const navigate = useNavigate();
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<unknown>(null);
  const [items, setItems] = React.useState<Schemas["PdfPreviewItem"][]>([]);
  const [rows, setRows] = React.useState<ReviewRow[]>([]);
  const [result, setResult] = React.useState<Schemas["CommitResult"] | null>(null);
  const [showText, setShowText] = React.useState<string | null>(null);

  const upload = async (files: File[]) => {
    if (!files.length) return;
    setBusy(true);
    setError(null);
    setResult(null);
    const form = new FormData();
    for (const f of files) form.append("files", f);
    try {
      const r = await postForm<Schemas["PdfPreview"]>(`/clients/${clientId}/imports/pdf/preview`, form);
      setItems(r.items);
      setRows(
        r.items.flatMap((item, i) =>
          item.transactions.map((tx, j) => ({
            key: `${i}-${j}`,
            tx,
            selected: !tx.duplicate_of,
            fileName: item.file_name,
            fileHash: item.file_hash || null,
          })),
        ),
      );
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const failed = items.filter((i) => i.transactions.length === 0);

  return (
    <div className="space-y-4">
      <Card>
        <CardBody className="space-y-4">
          <p className="text-sm text-muted">
            Unterstützt: Trade Republic, Scalable Capital, ING, DKB (Käufe, Verkäufe, Sparpläne, Dividenden). Eigene
            Regeln lassen sich als YAML-Datei in <code className="rounded bg-surface-2 px-1">data/pdf-rules</code>{" "}
            ablegen. Das Original-PDF wird als Beleg an die Buchung angehängt.
          </p>
          <Dropzone onFiles={upload} accept=".pdf,.txt" label="PDF-Belege auswählen" />
          {busy && (
            <div className="flex items-center gap-2 text-sm text-muted">
              <Spinner className="size-4" /> Verarbeite …
            </div>
          )}
          {error != null && <ErrorBox error={error} />}
          {result && (
            <ImportResult
              result={result}
              onOpen={() => navigate({ to: "/c/$clientId/transactions", params: { clientId } })}
            />
          )}
        </CardBody>
      </Card>

      {failed.length > 0 && (
        <Card>
          <CardHeader title={`Nicht erkannt (${failed.length})`} />
          <ul className="mt-2 divide-y divide-border text-sm">
            {failed.map((f) => (
              <li key={f.file_name} className="px-4 py-3 sm:px-5">
                <div className="flex items-start gap-2">
                  <AlertTriangle className="mt-0.5 size-4 shrink-0 text-warn" />
                  <div className="min-w-0 flex-1">
                    <div className="font-medium">{f.file_name}</div>
                    <div className="text-xs text-muted">
                      {f.bank ?? "Unbekannte Bank"} · {f.errors.join("; ")}
                    </div>
                  </div>
                  {f.text && (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setShowText(showText === f.file_name ? null : f.file_name)}
                    >
                      <FileText /> Text
                    </Button>
                  )}
                </div>
                {showText === f.file_name && (
                  <pre className="mt-2 max-h-80 overflow-auto rounded-lg bg-surface-2 p-3 text-xs whitespace-pre-wrap">
                    {f.text}
                  </pre>
                )}
              </li>
            ))}
          </ul>
        </Card>
      )}

      {rows.length > 0 && (
        <ImportReview
          rows={rows}
          setRows={setRows}
          onDone={(r) => {
            setResult(r);
            setRows([]);
            setItems([]);
          }}
        />
      )}
    </div>
  );
}

export function ImportPage() {
  const [tab, setTab] = React.useState<"pdf" | "csv">("pdf");
  return (
    <>
      <PageHeader
        title="Import"
        subtitle="Bankbelege und CSV-Dateien einlesen"
        actions={
          <Segmented
            value={tab}
            onChange={setTab}
            options={[
              { key: "pdf", label: "PDF-Belege" },
              { key: "csv", label: "CSV" },
            ]}
          />
        }
      />
      {tab === "pdf" ? <PdfImport /> : <CsvImport />}
    </>
  );
}
