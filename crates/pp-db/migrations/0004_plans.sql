-- Investment plans (stored as JSON documents)
CREATE TABLE investment_plans (
    client_id TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id        TEXT NOT NULL,
    position  INTEGER NOT NULL DEFAULT 0,
    data      TEXT NOT NULL,
    PRIMARY KEY (client_id, id)
);
