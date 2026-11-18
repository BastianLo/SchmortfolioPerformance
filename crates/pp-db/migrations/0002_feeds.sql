-- Result of the last quote update per security
CREATE TABLE feed_status (
    client_id   TEXT NOT NULL,
    security_id TEXT NOT NULL,
    last_run    TEXT NOT NULL,
    ok          INTEGER NOT NULL,
    message     TEXT,
    new_prices  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (client_id, security_id),
    FOREIGN KEY (client_id, security_id) REFERENCES securities (client_id, id) ON DELETE CASCADE
);
