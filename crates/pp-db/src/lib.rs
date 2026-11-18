//! Persistence layer (SQLite via sqlx).

use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime, Utc};
use pp_core::ExchangeRateSeries;
use pp_core::model::*;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

pub use sqlx;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, DbError>;

#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

const DT_FMT: &str = "%Y-%m-%dT%H:%M:%S";

fn fmt_dt(d: &NaiveDateTime) -> String {
    d.format(DT_FMT).to_string()
}

fn parse_dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, DT_FMT)
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"))
        .unwrap_or_default()
}

fn parse_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub oidc_issuer: Option<String>,
    pub oidc_subject: Option<String>,
}

fn user_from_row(r: &sqlx::sqlite::SqliteRow) -> User {
    User {
        id: r.get("id"),
        username: r.get("username"),
        email: r.get("email"),
        display_name: r.get("display_name"),
        password_hash: r.get("password_hash"),
        is_admin: r.get::<i64, _>("is_admin") != 0,
        oidc_issuer: r.get("oidc_issuer"),
        oidc_subject: r.get("oidc_subject"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientMeta {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub base_currency: String,
    pub pp_version: i64,
    pub data_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

fn meta_from_row(r: &sqlx::sqlite::SqliteRow) -> ClientMeta {
    ClientMeta {
        id: r.get("id"),
        owner_id: r.get("owner_id"),
        name: r.get("name"),
        base_currency: r.get("base_currency"),
        pp_version: r.get("pp_version"),
        data_version: r.get("data_version"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub read_only: bool,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attachment {
    pub file_hash: String,
    pub file_name: String,
    pub content_type: String,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedStatus {
    pub last_run: String,
    pub ok: bool,
    pub message: Option<String>,
    pub new_prices: i64,
}

pub struct NewUser<'a> {
    pub username: &'a str,
    pub email: Option<&'a str>,
    pub display_name: Option<&'a str>,
    pub password_hash: Option<&'a str>,
    pub is_admin: bool,
    pub oidc_issuer: Option<&'a str>,
    pub oidc_subject: Option<&'a str>,
}

impl Db {
    pub async fn connect(url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(10));
        // an in-memory database exists per connection, so use a single one
        let max = if url.contains(":memory:") { 1 } else { 8 };
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    // ---------------------------------------------------------------- users

    pub async fn count_users(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn create_user(&self, u: NewUser<'_>) -> Result<User> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, username, email, display_name, password_hash, is_admin, oidc_issuer, oidc_subject, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(u.username)
        .bind(u.email)
        .bind(u.display_name)
        .bind(u.password_hash)
        .bind(u.is_admin as i64)
        .bind(u.oidc_issuer)
        .bind(u.oidc_subject)
        .bind(now())
        .execute(&self.pool)
        .await?;
        self.user(&id).await
    }

    pub async fn user(&self, id: &str) -> Result<User> {
        let r = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        r.as_ref().map(user_from_row).ok_or(DbError::NotFound)
    }

    pub async fn user_by_username(&self, username: &str) -> Result<Option<User>> {
        let r = sqlx::query("SELECT * FROM users WHERE username = ? COLLATE NOCASE")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.as_ref().map(user_from_row))
    }

    pub async fn user_by_oidc(&self, issuer: &str, subject: &str) -> Result<Option<User>> {
        let r = sqlx::query("SELECT * FROM users WHERE oidc_issuer = ? AND oidc_subject = ?")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.as_ref().map(user_from_row))
    }

    pub async fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        let r = sqlx::query("SELECT * FROM users WHERE email = ? COLLATE NOCASE")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.as_ref().map(user_from_row))
    }

    pub async fn link_oidc(&self, user_id: &str, issuer: &str, subject: &str) -> Result<()> {
        sqlx::query("UPDATE users SET oidc_issuer = ?, oidc_subject = ? WHERE id = ?")
            .bind(issuer)
            .bind(subject)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_user_profile(
        &self,
        user_id: &str,
        email: Option<&str>,
        display_name: Option<&str>,
        is_admin: Option<bool>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE users SET email = COALESCE(?, email), display_name = COALESCE(?, display_name),
             is_admin = COALESCE(?, is_admin), last_login_at = ? WHERE id = ?",
        )
        .bind(email)
        .bind(display_name)
        .bind(is_admin.map(|b| b as i64))
        .bind(now())
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_users(&self) -> Result<Vec<User>> {
        let rows = sqlx::query("SELECT * FROM users ORDER BY username")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(user_from_row).collect())
    }

    pub async fn set_password(&self, user_id: &str, hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_admin(&self, user_id: &str, admin: bool) -> Result<()> {
        sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?")
            .bind(admin as i64)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<()> {
        let r = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        if r.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    // --------------------------------------------------------------- tokens

    pub async fn create_token(
        &self,
        user_id: &str,
        name: &str,
        token_hash: &str,
        read_only: bool,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO api_tokens (id, user_id, name, token_hash, read_only, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(user_id)
            .bind(name)
            .bind(token_hash)
            .bind(read_only as i64)
            .bind(now())
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn list_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>> {
        let rows = sqlx::query("SELECT * FROM api_tokens WHERE user_id = ? ORDER BY created_at")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| ApiToken {
                id: r.get("id"),
                name: r.get("name"),
                read_only: r.get::<i64, _>("read_only") != 0,
                created_at: r.get("created_at"),
                last_used_at: r.get("last_used_at"),
            })
            .collect())
    }

    pub async fn delete_token(&self, user_id: &str, id: &str) -> Result<()> {
        let r = sqlx::query("DELETE FROM api_tokens WHERE user_id = ? AND id = ?")
            .bind(user_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if r.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    /// Returns the user and the read-only flag of a token.
    pub async fn token_user(&self, token_hash: &str) -> Result<Option<(User, bool)>> {
        let r = sqlx::query("SELECT u.*, t.read_only AS t_read_only, t.id AS t_id FROM api_tokens t JOIN users u ON u.id = t.user_id WHERE t.token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = r else { return Ok(None) };
        let tid: String = r.get("t_id");
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
            .bind(now())
            .bind(tid)
            .execute(&self.pool)
            .await?;
        Ok(Some((
            user_from_row(&r),
            r.get::<i64, _>("t_read_only") != 0,
        )))
    }

    // ---------------------------------------------------------------- backup

    /// Writes a consistent copy of the database (SQLite `VACUUM INTO`).
    pub async fn backup_to(&self, path: &str) -> Result<()> {
        // a bound parameter is not reliably honoured by VACUUM INTO; quote the
        // path as SQL string literal instead
        let sql = format!("VACUUM INTO '{}'", path.replace('\'', "''"));
        sqlx::query(&sql).execute(&self.pool).await?;
        Ok(())
    }

    // ------------------------------------------------------------- sessions

    pub async fn create_session(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: &str,
        id_token: Option<&str>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO sessions (token_hash, user_id, created_at, expires_at, id_token) VALUES (?, ?, ?, ?, ?)")
            .bind(token_hash)
            .bind(user_id)
            .bind(now())
            .bind(expires_at)
            .bind(id_token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns the user and the stored id token of a valid session.
    pub async fn session_user(&self, token_hash: &str) -> Result<Option<(User, Option<String>)>> {
        let r = sqlx::query(
            "SELECT u.*, s.id_token AS s_id_token FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ?",
        )
        .bind(token_hash)
        .bind(now())
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.map(|r| (user_from_row(&r), r.get("s_id_token"))))
    }

    pub async fn delete_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn purge_sessions(&self) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now())
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    // -------------------------------------------------------------- clients

    pub async fn list_clients(&self, owner_id: &str) -> Result<Vec<ClientMeta>> {
        let rows = sqlx::query("SELECT * FROM clients WHERE owner_id = ? ORDER BY created_at")
            .bind(owner_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(meta_from_row).collect())
    }

    pub async fn client_meta(&self, id: &str) -> Result<ClientMeta> {
        let r = sqlx::query("SELECT * FROM clients WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        r.as_ref().map(meta_from_row).ok_or(DbError::NotFound)
    }

    pub async fn rename_client(&self, id: &str, name: &str) -> Result<()> {
        sqlx::query("UPDATE clients SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_client(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM clients WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn bump_version(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE clients SET data_version = data_version + 1, updated_at = ? WHERE id = ?",
        )
        .bind(now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Stores a complete client (e.g. after an import) as new client.
    pub async fn insert_client(
        &self,
        owner_id: &str,
        name: &str,
        c: &Client,
    ) -> Result<ClientMeta> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut tx = self.pool.begin().await?;
        let ts = now();
        sqlx::query(
            "INSERT INTO clients (id, owner_id, name, base_currency, pp_version, extra, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(owner_id)
        .bind(name)
        .bind(&c.base_currency)
        .bind(c.version as i64)
        .bind(serde_json::to_string(&c.extra)?)
        .bind(&ts)
        .bind(&ts)
        .execute(&mut *tx)
        .await?;

        for (pos, s) in c.securities.iter().enumerate() {
            upsert_security(&mut tx, &id, pos as i64, s).await?;
            for p in &s.prices {
                sqlx::query("INSERT OR REPLACE INTO security_prices (client_id, security_id, date, value) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(&s.id)
                    .bind(p.date.to_string())
                    .bind(p.value)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        for (pos, a) in c.accounts.iter().enumerate() {
            upsert_account(&mut tx, &id, pos as i64, a).await?;
            for t in &a.transactions {
                insert_tx(
                    &mut tx,
                    &id,
                    "account",
                    &a.id,
                    t.kind.as_str(),
                    &t.common,
                    t.ex_date.as_ref(),
                )
                .await?;
            }
        }
        for (pos, p) in c.portfolios.iter().enumerate() {
            upsert_portfolio(&mut tx, &id, pos as i64, p).await?;
            for t in &p.transactions {
                insert_tx(
                    &mut tx,
                    &id,
                    "portfolio",
                    &p.id,
                    t.kind.as_str(),
                    &t.common,
                    None,
                )
                .await?;
            }
        }
        for (pos, t) in c.taxonomies.iter().enumerate() {
            sqlx::query("INSERT INTO taxonomies (client_id, id, position, name, source, dimensions) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(&t.id)
                .bind(pos as i64)
                .bind(&t.name)
                .bind(&t.source)
                .bind(serde_json::to_string(&t.dimensions)?)
                .execute(&mut *tx)
                .await?;
            for (cpos, cl) in t.classifications.iter().enumerate() {
                sqlx::query(
                    "INSERT OR REPLACE INTO classifications (client_id, taxonomy_id, id, position, parent_id, name, note, color, weight, rank, assignments, data)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&t.id)
                .bind(&cl.id)
                .bind(cpos as i64)
                .bind(&cl.parent_id)
                .bind(&cl.name)
                .bind(&cl.note)
                .bind(&cl.color)
                .bind(cl.weight as i64)
                .bind(cl.rank as i64)
                .bind(serde_json::to_string(&cl.assignments)?)
                .bind(serde_json::to_string(&cl.data)?)
                .execute(&mut *tx)
                .await?;
            }
        }
        for (pos, p) in c.plans.iter().enumerate() {
            sqlx::query(
                "INSERT INTO investment_plans (client_id, id, position, data) VALUES (?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&p.id)
            .bind(pos as i64)
            .bind(serde_json::to_string(p)?)
            .execute(&mut *tx)
            .await?;
        }
        for (pos, w) in c.watchlists.iter().enumerate() {
            sqlx::query("INSERT INTO watchlists (client_id, position, name, securities) VALUES (?, ?, ?, ?)")
                .bind(&id)
                .bind(pos as i64)
                .bind(&w.name)
                .bind(serde_json::to_string(&w.securities)?)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.client_meta(&id).await
    }

    /// Loads the complete domain model of a client.
    pub async fn load_client(&self, id: &str) -> Result<Client> {
        let meta = self.client_meta(id).await?;
        let extra: String = sqlx::query_scalar("SELECT extra FROM clients WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;

        let mut prices: std::collections::HashMap<String, Vec<SecurityPrice>> = Default::default();
        let rows = sqlx::query("SELECT security_id, date, value FROM security_prices WHERE client_id = ? ORDER BY security_id, date")
            .bind(id)
            .fetch_all(&self.pool)
            .await?;
        for r in rows {
            prices
                .entry(r.get("security_id"))
                .or_default()
                .push(SecurityPrice {
                    date: parse_date(r.get::<&str, _>("date")),
                    value: r.get("value"),
                });
        }

        let securities =
            sqlx::query("SELECT * FROM securities WHERE client_id = ? ORDER BY position")
                .bind(id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let sid: String = r.get("id");
                    Security {
                        prices: prices.remove(&sid).unwrap_or_default(),
                        id: sid,
                        name: r.get("name"),
                        currency: r.get("currency"),
                        target_currency: r.get("target_currency"),
                        isin: r.get("isin"),
                        wkn: r.get("wkn"),
                        ticker: r.get("ticker"),
                        online_id: r.get("online_id"),
                        calendar: r.get("calendar"),
                        feed: r.get("feed"),
                        feed_url: r.get("feed_url"),
                        latest_feed: r.get("latest_feed"),
                        latest_feed_url: r.get("latest_feed_url"),
                        note: r.get("note"),
                        retired: r.get::<i64, _>("retired") != 0,
                        latest: r
                            .get::<Option<String>, _>("latest")
                            .and_then(|l| serde_json::from_str(&l).ok()),
                        events: serde_json::from_str(r.get::<&str, _>("events"))
                            .unwrap_or_default(),
                        attributes: serde_json::from_str(r.get::<&str, _>("attributes"))
                            .unwrap_or_default(),
                        updated_at: r.get("updated_at"),
                    }
                })
                .collect();

        let mut account_tx: std::collections::HashMap<String, Vec<AccountTransaction>> =
            Default::default();
        let mut portfolio_tx: std::collections::HashMap<String, Vec<PortfolioTransaction>> =
            Default::default();
        let rows =
            sqlx::query("SELECT * FROM transactions WHERE client_id = ? ORDER BY date_time, rowid")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        for r in rows {
            let common = TransactionCommon {
                id: r.get("id"),
                date_time: parse_dt(r.get::<&str, _>("date_time")),
                currency: r.get("currency"),
                amount: r.get("amount"),
                shares: r.get("shares"),
                security: r.get("security_id"),
                note: r.get("note"),
                source: r.get("source"),
                units: serde_json::from_str(r.get::<&str, _>("units")).unwrap_or_default(),
                cross_entry: r.get::<Option<String>, _>("cross_kind").and_then(|k| {
                    Some(CrossEntry {
                        kind: CrossEntryKind::parse(&k)?,
                        other_owner: r.get::<Option<String>, _>("cross_owner")?,
                        other_transaction: r.get::<Option<String>, _>("cross_tx")?,
                    })
                }),
                updated_at: r.get("updated_at"),
            };
            let owner: String = r.get("owner_id");
            let kind: &str = r.get("type");
            if r.get::<&str, _>("owner_kind") == "account" {
                let Some(kind) = AccountTransactionType::parse(kind) else {
                    continue;
                };
                let ex_date = r.get::<Option<String>, _>("ex_date").map(|s| parse_dt(&s));
                account_tx
                    .entry(owner)
                    .or_default()
                    .push(AccountTransaction {
                        kind,
                        common,
                        ex_date,
                    });
            } else {
                let Some(kind) = PortfolioTransactionType::parse(kind) else {
                    continue;
                };
                portfolio_tx
                    .entry(owner)
                    .or_default()
                    .push(PortfolioTransaction { kind, common });
            }
        }

        let accounts = sqlx::query("SELECT * FROM accounts WHERE client_id = ? ORDER BY position")
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|r| {
                let aid: String = r.get("id");
                Account {
                    transactions: account_tx.remove(&aid).unwrap_or_default(),
                    id: aid,
                    name: r.get("name"),
                    currency: r.get("currency"),
                    note: r.get("note"),
                    retired: r.get::<i64, _>("retired") != 0,
                    attributes: serde_json::from_str(r.get::<&str, _>("attributes"))
                        .unwrap_or_default(),
                    updated_at: r.get("updated_at"),
                }
            })
            .collect();
        let portfolios =
            sqlx::query("SELECT * FROM portfolios WHERE client_id = ? ORDER BY position")
                .bind(id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let pid: String = r.get("id");
                    Portfolio {
                        transactions: portfolio_tx.remove(&pid).unwrap_or_default(),
                        id: pid,
                        name: r.get("name"),
                        note: r.get("note"),
                        retired: r.get::<i64, _>("retired") != 0,
                        reference_account: r.get("reference_account"),
                        attributes: serde_json::from_str(r.get::<&str, _>("attributes"))
                            .unwrap_or_default(),
                        updated_at: r.get("updated_at"),
                    }
                })
                .collect();

        let mut classifications: std::collections::HashMap<String, Vec<Classification>> =
            Default::default();
        for r in sqlx::query("SELECT * FROM classifications WHERE client_id = ? ORDER BY position")
            .bind(id)
            .fetch_all(&self.pool)
            .await?
        {
            classifications
                .entry(r.get("taxonomy_id"))
                .or_default()
                .push(Classification {
                    id: r.get("id"),
                    parent_id: r.get("parent_id"),
                    name: r.get("name"),
                    note: r.get("note"),
                    color: r.get("color"),
                    weight: r.get::<i64, _>("weight") as i32,
                    rank: r.get::<i64, _>("rank") as i32,
                    assignments: serde_json::from_str(r.get::<&str, _>("assignments"))
                        .unwrap_or_default(),
                    data: r
                        .get::<Option<String>, _>("data")
                        .and_then(|d| serde_json::from_str(&d).ok())
                        .unwrap_or_default(),
                });
        }
        let taxonomies =
            sqlx::query("SELECT * FROM taxonomies WHERE client_id = ? ORDER BY position")
                .bind(id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| {
                    let tid: String = r.get("id");
                    Taxonomy {
                        classifications: classifications.remove(&tid).unwrap_or_default(),
                        id: tid,
                        name: r.get("name"),
                        source: r.get("source"),
                        dimensions: serde_json::from_str(r.get::<&str, _>("dimensions"))
                            .unwrap_or_default(),
                    }
                })
                .collect();
        let watchlists =
            sqlx::query("SELECT * FROM watchlists WHERE client_id = ? ORDER BY position")
                .bind(id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|r| Watchlist {
                    name: r.get("name"),
                    securities: serde_json::from_str(r.get::<&str, _>("securities"))
                        .unwrap_or_default(),
                })
                .collect();

        let plans = sqlx::query_scalar::<_, String>(
            "SELECT data FROM investment_plans WHERE client_id = ? ORDER BY position",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|d| serde_json::from_str(&d).ok())
        .collect();

        let mut client = Client {
            version: meta.pp_version as i32,
            base_currency: meta.base_currency,
            securities,
            accounts,
            portfolios,
            taxonomies,
            watchlists,
            plans,
            extra: serde_json::from_str(&extra).unwrap_or_default(),
        };
        client.normalize();
        Ok(client)
    }

    // ------------------------------------------------------- entity writes

    pub async fn save_security(&self, client_id: &str, s: &Security) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let pos: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM securities WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_one(&mut *tx)
        .await?;
        upsert_security(&mut tx, client_id, pos, s).await?;
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    pub async fn save_account(&self, client_id: &str, a: &Account) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let pos: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM accounts WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_one(&mut *tx)
        .await?;
        upsert_account(&mut tx, client_id, pos, a).await?;
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    pub async fn save_portfolio(&self, client_id: &str, p: &Portfolio) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let pos: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM portfolios WHERE client_id = ?",
        )
        .bind(client_id)
        .fetch_one(&mut *tx)
        .await?;
        upsert_portfolio(&mut tx, client_id, pos, p).await?;
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    /// Deletes an account, portfolio or security; fails if transactions exist.
    pub async fn delete_entity(&self, client_id: &str, table: &str, id: &str) -> Result<()> {
        let (table, col) = match table {
            "accounts" => ("accounts", "owner_id"),
            "portfolios" => ("portfolios", "owner_id"),
            "securities" => ("securities", "security_id"),
            _ => return Err(DbError::Invalid("unknown table".into())),
        };
        let n: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM transactions WHERE client_id = ? AND {col} = ?"
        ))
        .bind(client_id)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        if n > 0 {
            return Err(DbError::Invalid(format!(
                "{n} Buchungen verweisen noch auf diesen Eintrag"
            )));
        }
        let r = sqlx::query(&format!(
            "DELETE FROM {table} WHERE client_id = ? AND id = ?"
        ))
        .bind(client_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.bump_version(client_id).await
    }

    /// Deletes `delete` (with their counterparts) and inserts `insert`, in
    /// one database transaction.
    pub async fn replace_transactions(
        &self,
        client_id: &str,
        delete: &[String],
        insert: &[pp_core::booking::StoredTx],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for id in delete {
            let other: Option<Option<String>> = sqlx::query_scalar(
                "SELECT cross_tx FROM transactions WHERE client_id = ? AND id = ?",
            )
            .bind(client_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(other) = other else {
                return Err(DbError::NotFound);
            };
            for i in std::iter::once(id.clone()).chain(other) {
                sqlx::query("DELETE FROM transactions WHERE client_id = ? AND id = ?")
                    .bind(client_id)
                    .bind(i)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        for t in insert {
            match t {
                pp_core::booking::StoredTx::Account { owner, tx: t } => {
                    insert_tx(
                        &mut tx,
                        client_id,
                        "account",
                        owner,
                        t.kind.as_str(),
                        &t.common,
                        t.ex_date.as_ref(),
                    )
                    .await?
                }
                pp_core::booking::StoredTx::Portfolio { owner, tx: t } => {
                    insert_tx(
                        &mut tx,
                        client_id,
                        "portfolio",
                        owner,
                        t.kind.as_str(),
                        &t.common,
                        None,
                    )
                    .await?
                }
            }
        }
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    /// Deletes a transaction and its counterpart (cross entry).
    pub async fn delete_transaction(&self, client_id: &str, id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let other: Option<Option<String>> =
            sqlx::query_scalar("SELECT cross_tx FROM transactions WHERE client_id = ? AND id = ?")
                .bind(client_id)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(other) = other else {
            return Err(DbError::NotFound);
        };
        sqlx::query("DELETE FROM transactions WHERE client_id = ? AND id = ?")
            .bind(client_id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if let Some(o) = other {
            sqlx::query("DELETE FROM transactions WHERE client_id = ? AND id = ?")
                .bind(client_id)
                .bind(o)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    pub async fn upsert_prices(
        &self,
        client_id: &str,
        security_id: &str,
        prices: &[SecurityPrice],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for p in prices {
            sqlx::query("INSERT OR REPLACE INTO security_prices (client_id, security_id, date, value) VALUES (?, ?, ?, ?)")
                .bind(client_id)
                .bind(security_id)
                .bind(p.date.to_string())
                .bind(p.value)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    // ------------------------------------------------- watchlists/taxonomies

    pub async fn set_watchlists(&self, client_id: &str, lists: &[Watchlist]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM watchlists WHERE client_id = ?")
            .bind(client_id)
            .execute(&mut *tx)
            .await?;
        for (pos, w) in lists.iter().enumerate() {
            sqlx::query("INSERT INTO watchlists (client_id, position, name, securities) VALUES (?, ?, ?, ?)")
                .bind(client_id)
                .bind(pos as i64)
                .bind(&w.name)
                .bind(serde_json::to_string(&w.securities)?)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.bump_version(client_id).await
    }

    pub async fn update_classification(
        &self,
        client_id: &str,
        taxonomy_id: &str,
        c: &Classification,
    ) -> Result<()> {
        let r = sqlx::query(
            "UPDATE classifications SET name = ?, color = ?, weight = ?, note = ?, assignments = ?
             WHERE client_id = ? AND taxonomy_id = ? AND id = ?",
        )
        .bind(&c.name)
        .bind(&c.color)
        .bind(c.weight as i64)
        .bind(&c.note)
        .bind(serde_json::to_string(&c.assignments)?)
        .bind(client_id)
        .bind(taxonomy_id)
        .bind(&c.id)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.bump_version(client_id).await
    }

    pub async fn insert_classification(
        &self,
        client_id: &str,
        taxonomy_id: &str,
        c: &Classification,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO classifications (client_id, taxonomy_id, id, position, parent_id, name, note, color, weight, rank, assignments, data)
             VALUES (?, ?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM classifications WHERE client_id = ?), ?, ?, ?, ?, ?, ?, ?, 'null')",
        )
        .bind(client_id)
        .bind(taxonomy_id)
        .bind(&c.id)
        .bind(client_id)
        .bind(&c.parent_id)
        .bind(&c.name)
        .bind(&c.note)
        .bind(&c.color)
        .bind(c.weight as i64)
        .bind(c.rank as i64)
        .bind(serde_json::to_string(&c.assignments)?)
        .execute(&self.pool)
        .await?;
        self.bump_version(client_id).await
    }

    pub async fn delete_classification(
        &self,
        client_id: &str,
        taxonomy_id: &str,
        id: &str,
    ) -> Result<()> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM classifications WHERE client_id = ? AND taxonomy_id = ? AND parent_id = ?")
            .bind(client_id)
            .bind(taxonomy_id)
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        if n > 0 {
            return Err(DbError::Invalid(
                "Die Klassifizierung hat noch Unterkategorien".into(),
            ));
        }
        sqlx::query(
            "DELETE FROM classifications WHERE client_id = ? AND taxonomy_id = ? AND id = ?",
        )
        .bind(client_id)
        .bind(taxonomy_id)
        .bind(id)
        .execute(&self.pool)
        .await?;
        self.bump_version(client_id).await
    }

    // ---------------------------------------------------------------- plans

    pub async fn save_plan(
        &self,
        client_id: &str,
        p: &pp_core::plan::InvestmentPlan,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO investment_plans (client_id, id, position, data)
             VALUES (?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM investment_plans WHERE client_id = ?), ?)
             ON CONFLICT (client_id, id) DO UPDATE SET data = excluded.data",
        )
        .bind(client_id)
        .bind(&p.id)
        .bind(client_id)
        .bind(serde_json::to_string(p)?)
        .execute(&self.pool)
        .await?;
        self.bump_version(client_id).await
    }

    pub async fn delete_plan(&self, client_id: &str, plan_id: &str) -> Result<()> {
        let r = sqlx::query("DELETE FROM investment_plans WHERE client_id = ? AND id = ?")
            .bind(client_id)
            .bind(plan_id)
            .execute(&self.pool)
            .await?;
        if r.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }
        self.bump_version(client_id).await
    }

    // ---------------------------------------------------------- attachments

    pub async fn add_attachment(&self, client_id: &str, tx_id: &str, a: &Attachment) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO attachments (client_id, transaction_id, file_hash, file_name, content_type, size, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(client_id)
        .bind(tx_id)
        .bind(&a.file_hash)
        .bind(&a.file_name)
        .bind(&a.content_type)
        .bind(a.size)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Attachments of a client, keyed by transaction id.
    pub async fn attachments(
        &self,
        client_id: &str,
    ) -> Result<std::collections::HashMap<String, Vec<Attachment>>> {
        let rows = sqlx::query("SELECT * FROM attachments WHERE client_id = ? ORDER BY created_at")
            .bind(client_id)
            .fetch_all(&self.pool)
            .await?;
        let mut m: std::collections::HashMap<String, Vec<Attachment>> = Default::default();
        for r in rows {
            m.entry(r.get("transaction_id"))
                .or_default()
                .push(Attachment {
                    file_hash: r.get("file_hash"),
                    file_name: r.get("file_name"),
                    content_type: r.get("content_type"),
                    size: r.get("size"),
                });
        }
        Ok(m)
    }

    pub async fn attachment_by_hash(
        &self,
        client_id: &str,
        hash: &str,
    ) -> Result<Option<Attachment>> {
        let r =
            sqlx::query("SELECT * FROM attachments WHERE client_id = ? AND file_hash = ? LIMIT 1")
                .bind(client_id)
                .bind(hash)
                .fetch_optional(&self.pool)
                .await?;
        Ok(r.map(|r| Attachment {
            file_hash: r.get("file_hash"),
            file_name: r.get("file_name"),
            content_type: r.get("content_type"),
            size: r.get("size"),
        }))
    }

    // ---------------------------------------------------------- quote feeds

    pub async fn set_latest(
        &self,
        client_id: &str,
        security_id: &str,
        latest: &LatestSecurityPrice,
    ) -> Result<()> {
        sqlx::query("UPDATE securities SET latest = ? WHERE client_id = ? AND id = ?")
            .bind(serde_json::to_string(latest)?)
            .bind(client_id)
            .bind(security_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_feed_status(
        &self,
        client_id: &str,
        security_id: &str,
        ok: bool,
        message: Option<&str>,
        new_prices: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO feed_status (client_id, security_id, last_run, ok, message, new_prices) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(client_id)
        .bind(security_id)
        .bind(now())
        .bind(ok as i64)
        .bind(message)
        .bind(new_prices)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn feed_status(
        &self,
        client_id: &str,
    ) -> Result<std::collections::HashMap<String, FeedStatus>> {
        let rows = sqlx::query("SELECT * FROM feed_status WHERE client_id = ?")
            .bind(client_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("security_id"),
                    FeedStatus {
                        last_run: r.get("last_run"),
                        ok: r.get::<i64, _>("ok") != 0,
                        message: r.get("message"),
                        new_prices: r.get("new_prices"),
                    },
                )
            })
            .collect())
    }

    pub async fn all_client_ids(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT id FROM clients")
            .fetch_all(&self.pool)
            .await?)
    }

    // ------------------------------------------------------ exchange rates

    pub async fn upsert_exchange_rates(
        &self,
        series: &[ExchangeRateSeries],
        source: &str,
    ) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut n = 0;
        for s in series {
            for (date, rate) in &s.rates {
                sqlx::query("INSERT OR REPLACE INTO exchange_rates (base, term, date, rate, source) VALUES (?, ?, ?, ?, ?)")
                    .bind(&s.base)
                    .bind(&s.term)
                    .bind(date.to_string())
                    .bind(rate.to_string())
                    .bind(source)
                    .execute(&mut *tx)
                    .await?;
                n += 1;
            }
        }
        tx.commit().await?;
        Ok(n)
    }

    pub async fn exchange_rates(&self) -> Result<Vec<ExchangeRateSeries>> {
        let rows = sqlx::query(
            "SELECT base, term, date, rate FROM exchange_rates ORDER BY base, term, date",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out: Vec<ExchangeRateSeries> = Vec::new();
        for r in rows {
            let base: String = r.get("base");
            let term: String = r.get("term");
            if out
                .last()
                .map(|s| s.base != base || s.term != term)
                .unwrap_or(true)
            {
                out.push(ExchangeRateSeries::new(base, term));
            }
            let rate = Decimal::from_str(r.get::<&str, _>("rate")).unwrap_or(Decimal::ONE);
            out.last_mut()
                .unwrap()
                .rates
                .push((parse_date(r.get::<&str, _>("date")), rate));
        }
        Ok(out)
    }

    pub async fn exchange_rate_status(&self) -> Result<(i64, Option<String>)> {
        let r = sqlx::query("SELECT COUNT(*) AS n, MAX(date) AS last FROM exchange_rates")
            .fetch_one(&self.pool)
            .await?;
        Ok((r.get("n"), r.get("last")))
    }

    // ------------------------------------------------------------ settings

    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

type Tx<'a> = sqlx::Transaction<'a, sqlx::Sqlite>;

async fn upsert_security(tx: &mut Tx<'_>, client_id: &str, pos: i64, s: &Security) -> Result<()> {
    sqlx::query(
        "INSERT INTO securities (client_id, id, position, name, currency, target_currency, isin, wkn, ticker, online_id, calendar,
            feed, feed_url, latest_feed, latest_feed_url, note, retired, latest, events, attributes, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (client_id, id) DO UPDATE SET name = excluded.name, currency = excluded.currency,
            target_currency = excluded.target_currency, isin = excluded.isin, wkn = excluded.wkn, ticker = excluded.ticker,
            online_id = excluded.online_id, calendar = excluded.calendar, feed = excluded.feed, feed_url = excluded.feed_url,
            latest_feed = excluded.latest_feed, latest_feed_url = excluded.latest_feed_url, note = excluded.note,
            retired = excluded.retired, latest = excluded.latest, events = excluded.events, attributes = excluded.attributes,
            updated_at = excluded.updated_at",
    )
    .bind(client_id)
    .bind(&s.id)
    .bind(pos)
    .bind(&s.name)
    .bind(&s.currency)
    .bind(&s.target_currency)
    .bind(&s.isin)
    .bind(&s.wkn)
    .bind(&s.ticker)
    .bind(&s.online_id)
    .bind(&s.calendar)
    .bind(&s.feed)
    .bind(&s.feed_url)
    .bind(&s.latest_feed)
    .bind(&s.latest_feed_url)
    .bind(&s.note)
    .bind(s.retired as i64)
    .bind(s.latest.as_ref().map(serde_json::to_string).transpose()?)
    .bind(serde_json::to_string(&s.events)?)
    .bind(serde_json::to_string(&s.attributes)?)
    .bind(&s.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_account(tx: &mut Tx<'_>, client_id: &str, pos: i64, a: &Account) -> Result<()> {
    sqlx::query(
        "INSERT INTO accounts (client_id, id, position, name, currency, note, retired, attributes, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (client_id, id) DO UPDATE SET name = excluded.name, currency = excluded.currency, note = excluded.note,
            retired = excluded.retired, attributes = excluded.attributes, updated_at = excluded.updated_at",
    )
    .bind(client_id)
    .bind(&a.id)
    .bind(pos)
    .bind(&a.name)
    .bind(&a.currency)
    .bind(&a.note)
    .bind(a.retired as i64)
    .bind(serde_json::to_string(&a.attributes)?)
    .bind(&a.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_portfolio(tx: &mut Tx<'_>, client_id: &str, pos: i64, p: &Portfolio) -> Result<()> {
    sqlx::query(
        "INSERT INTO portfolios (client_id, id, position, name, note, retired, reference_account, attributes, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (client_id, id) DO UPDATE SET name = excluded.name, note = excluded.note, retired = excluded.retired,
            reference_account = excluded.reference_account, attributes = excluded.attributes, updated_at = excluded.updated_at",
    )
    .bind(client_id)
    .bind(&p.id)
    .bind(pos)
    .bind(&p.name)
    .bind(&p.note)
    .bind(p.retired as i64)
    .bind(&p.reference_account)
    .bind(serde_json::to_string(&p.attributes)?)
    .bind(&p.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_tx(
    tx: &mut Tx<'_>,
    client_id: &str,
    owner_kind: &str,
    owner_id: &str,
    kind: &str,
    t: &TransactionCommon,
    ex_date: Option<&NaiveDateTime>,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO transactions (client_id, id, owner_kind, owner_id, type, date_time, currency, amount, shares,
            security_id, note, source, ex_date, cross_kind, cross_owner, cross_tx, units, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(client_id)
    .bind(&t.id)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(kind)
    .bind(fmt_dt(&t.date_time))
    .bind(&t.currency)
    .bind(t.amount)
    .bind(t.shares)
    .bind(&t.security)
    .bind(&t.note)
    .bind(&t.source)
    .bind(ex_date.map(fmt_dt))
    .bind(t.cross_entry.as_ref().map(|c| c.kind.as_str()))
    .bind(t.cross_entry.as_ref().map(|c| c.other_owner.as_str()))
    .bind(t.cross_entry.as_ref().map(|c| c.other_transaction.as_str()))
    .bind(serde_json::to_string(&t.units)?)
    .bind(&t.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip_client() {
        let db = Db::connect("sqlite::memory:").await.unwrap();
        let u = db
            .create_user(NewUser {
                username: "admin",
                email: None,
                display_name: None,
                password_hash: Some("x"),
                is_admin: true,
                oidc_issuer: None,
                oidc_subject: None,
            })
            .await
            .unwrap();
        let mut c = Client {
            base_currency: "EUR".into(),
            version: 70,
            ..Default::default()
        };
        c.securities.push(Security {
            id: "s1".into(),
            name: "Test".into(),
            currency: Some("EUR".into()),
            prices: vec![SecurityPrice {
                date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                value: 100_00000000,
            }],
            ..Default::default()
        });
        c.accounts.push(Account {
            id: "a1".into(),
            name: "Konto".into(),
            currency: "EUR".into(),
            ..Default::default()
        });
        c.accounts[0].transactions.push(AccountTransaction {
            kind: AccountTransactionType::Deposit,
            ex_date: None,
            common: TransactionCommon {
                id: "t1".into(),
                date_time: NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                currency: "EUR".into(),
                amount: 1000_00,
                shares: 0,
                security: None,
                note: None,
                source: None,
                units: vec![],
                cross_entry: None,
                updated_at: None,
            },
        });
        let meta = db.insert_client(&u.id, "Test", &c).await.unwrap();
        let loaded = db.load_client(&meta.id).await.unwrap();
        assert_eq!(loaded.securities[0].prices.len(), 1);
        assert_eq!(loaded.accounts[0].transactions[0].amount, 1000_00);
        db.delete_transaction(&meta.id, "t1").await.unwrap();
        assert!(
            db.load_client(&meta.id).await.unwrap().accounts[0]
                .transactions
                .is_empty()
        );
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;

    #[tokio::test]
    async fn token_lookup() {
        let file = std::env::temp_dir().join(format!("pp-src-{}.db", uuid::Uuid::new_v4()));
        let db = Db::connect(&format!("sqlite://{}", file.display()))
            .await
            .unwrap();
        let u = db
            .create_user(NewUser {
                username: "a",
                email: None,
                display_name: None,
                password_hash: None,
                is_admin: false,
                oidc_issuer: None,
                oidc_subject: None,
            })
            .await
            .unwrap();
        db.create_token(&u.id, "t", "hash1", true).await.unwrap();
        let r = db.token_user("hash1").await;
        assert!(matches!(r, Ok(Some(_))), "{r:?}");
        let path = std::env::temp_dir().join(format!("pp-backup-{}.db", uuid::Uuid::new_v4()));
        let r = db.backup_to(&path.to_string_lossy()).await;
        eprintln!("backup result {r:?} path {}", path.display());
        assert!(
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 0,
            "backup file missing"
        );
        let _ = std::fs::remove_file(path);
    }
}
