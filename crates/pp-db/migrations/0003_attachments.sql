-- Documents (e.g. the original PDF) attached to transactions. The file is
-- stored in the data directory under files/<sha256>.
CREATE TABLE attachments (
    client_id      TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL,
    file_hash      TEXT NOT NULL,
    file_name      TEXT NOT NULL,
    content_type   TEXT NOT NULL,
    size           INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    PRIMARY KEY (client_id, transaction_id, file_hash)
);
CREATE INDEX attachments_hash ON attachments (client_id, file_hash);
