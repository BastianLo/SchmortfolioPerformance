-- Users and authentication
CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    email         TEXT,
    display_name  TEXT,
    password_hash TEXT,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    oidc_issuer   TEXT,
    oidc_subject  TEXT,
    created_at    TEXT NOT NULL,
    last_login_at TEXT
);
CREATE UNIQUE INDEX users_oidc ON users (oidc_issuer, oidc_subject) WHERE oidc_subject IS NOT NULL;

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    id_token   TEXT
);
CREATE INDEX sessions_user ON sessions (user_id);

-- A client corresponds to one Portfolio Performance file
CREATE TABLE clients (
    id            TEXT PRIMARY KEY,
    owner_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    pp_version    INTEGER NOT NULL DEFAULT 0,
    extra         TEXT NOT NULL DEFAULT '{}',
    data_version  INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX clients_owner ON clients (owner_id);

CREATE TABLE securities (
    client_id       TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    position        INTEGER NOT NULL DEFAULT 0,
    name            TEXT NOT NULL,
    currency        TEXT,
    target_currency TEXT,
    isin            TEXT,
    wkn             TEXT,
    ticker          TEXT,
    online_id       TEXT,
    calendar        TEXT,
    feed            TEXT,
    feed_url        TEXT,
    latest_feed     TEXT,
    latest_feed_url TEXT,
    note            TEXT,
    retired         INTEGER NOT NULL DEFAULT 0,
    latest          TEXT,
    events          TEXT NOT NULL DEFAULT '[]',
    attributes      TEXT NOT NULL DEFAULT '{}',
    updated_at      TEXT,
    PRIMARY KEY (client_id, id)
);

CREATE TABLE security_prices (
    client_id   TEXT NOT NULL,
    security_id TEXT NOT NULL,
    date        TEXT NOT NULL,
    value       INTEGER NOT NULL,
    PRIMARY KEY (client_id, security_id, date),
    FOREIGN KEY (client_id, security_id) REFERENCES securities (client_id, id) ON DELETE CASCADE
) WITHOUT ROWID;

CREATE TABLE accounts (
    client_id  TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    name       TEXT NOT NULL,
    currency   TEXT NOT NULL,
    note       TEXT,
    retired    INTEGER NOT NULL DEFAULT 0,
    attributes TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT,
    PRIMARY KEY (client_id, id)
);

CREATE TABLE portfolios (
    client_id         TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id                TEXT NOT NULL,
    position          INTEGER NOT NULL DEFAULT 0,
    name              TEXT NOT NULL,
    note              TEXT,
    retired           INTEGER NOT NULL DEFAULT 0,
    reference_account TEXT,
    attributes        TEXT NOT NULL DEFAULT '{}',
    updated_at        TEXT,
    PRIMARY KEY (client_id, id)
);

-- Account and portfolio transactions in one table
CREATE TABLE transactions (
    client_id   TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    owner_kind  TEXT NOT NULL CHECK (owner_kind IN ('account', 'portfolio')),
    owner_id    TEXT NOT NULL,
    type        TEXT NOT NULL,
    date_time   TEXT NOT NULL,
    currency    TEXT NOT NULL,
    amount      INTEGER NOT NULL,
    shares      INTEGER NOT NULL DEFAULT 0,
    security_id TEXT,
    note        TEXT,
    source      TEXT,
    ex_date     TEXT,
    cross_kind  TEXT,
    cross_owner TEXT,
    cross_tx    TEXT,
    units       TEXT NOT NULL DEFAULT '[]',
    updated_at  TEXT,
    PRIMARY KEY (client_id, id)
);
CREATE INDEX transactions_owner ON transactions (client_id, owner_id, date_time);
CREATE INDEX transactions_security ON transactions (client_id, security_id);

CREATE TABLE taxonomies (
    client_id  TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    name       TEXT NOT NULL,
    source     TEXT,
    dimensions TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (client_id, id)
);

CREATE TABLE classifications (
    client_id   TEXT NOT NULL,
    taxonomy_id TEXT NOT NULL,
    id          TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,
    parent_id   TEXT,
    name        TEXT NOT NULL,
    note        TEXT,
    color       TEXT,
    weight      INTEGER NOT NULL,
    rank        INTEGER NOT NULL,
    assignments TEXT NOT NULL DEFAULT '[]',
    data        TEXT,
    PRIMARY KEY (client_id, taxonomy_id, id),
    FOREIGN KEY (client_id, taxonomy_id) REFERENCES taxonomies (client_id, id) ON DELETE CASCADE
);

CREATE TABLE watchlists (
    client_id  TEXT NOT NULL REFERENCES clients (id) ON DELETE CASCADE,
    position   INTEGER NOT NULL,
    name       TEXT NOT NULL,
    securities TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (client_id, position)
);

-- Global exchange rates (e.g. ECB reference rates)
CREATE TABLE exchange_rates (
    base   TEXT NOT NULL,
    term   TEXT NOT NULL,
    date   TEXT NOT NULL,
    rate   TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'ECB',
    PRIMARY KEY (base, term, date)
) WITHOUT ROWID;

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
