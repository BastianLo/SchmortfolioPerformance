//! Shared request/response types.

use chrono::{Local, NaiveDate};
use pp_core::model::Client;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::{ApiError, ApiResult};

#[derive(Serialize, ToSchema)]
pub struct ClientDto {
    pub id: String,
    pub name: String,
    pub base_currency: String,
    pub pp_version: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&pp_db::ClientMeta> for ClientDto {
    fn from(m: &pp_db::ClientMeta) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            base_currency: m.base_currency.clone(),
            pp_version: m.pp_version,
            created_at: m.created_at.clone(),
            updated_at: m.updated_at.clone(),
        }
    }
}

#[derive(Serialize, ToSchema, Default)]
pub struct Counts {
    pub securities: usize,
    pub prices: usize,
    pub accounts: usize,
    pub portfolios: usize,
    pub transactions: usize,
    pub taxonomies: usize,
    pub watchlists: usize,
}

impl Counts {
    pub fn of(c: &Client) -> Self {
        Self {
            securities: c.securities.len(),
            prices: c.securities.iter().map(|s| s.prices.len()).sum(),
            accounts: c.accounts.len(),
            portfolios: c.portfolios.len(),
            transactions: c
                .accounts
                .iter()
                .map(|a| a.transactions.len())
                .sum::<usize>()
                + c.portfolios
                    .iter()
                    .map(|p| p.transactions.len())
                    .sum::<usize>(),
            taxonomies: c.taxonomies.len(),
            watchlists: c.watchlists.len(),
        }
    }
}

/// Reporting period; defaults to "since the first transaction until today".
#[derive(Deserialize, IntoParams, Default)]
pub struct PeriodQuery {
    /// start date (exclusive for transactions, PP semantics), ISO format
    pub from: Option<NaiveDate>,
    /// end date (inclusive), ISO format
    pub to: Option<NaiveDate>,
    /// restrict to portfolios / accounts (comma separated ids)
    pub filter: Option<String>,
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn first_transaction_date(c: &Client) -> Option<NaiveDate> {
    let a = c
        .accounts
        .iter()
        .flat_map(|a| a.transactions.iter().map(|t| t.date()))
        .min();
    let p = c
        .portfolios
        .iter()
        .flat_map(|p| p.transactions.iter().map(|t| t.date()))
        .min();
    match (a, p) {
        (Some(a), Some(p)) => Some(a.min(p)),
        (a, p) => a.or(p),
    }
}

impl PeriodQuery {
    pub fn resolve(&self, c: &Client) -> ApiResult<(NaiveDate, NaiveDate)> {
        let to = self.to.unwrap_or_else(today);
        let from = self.from.unwrap_or_else(|| {
            first_transaction_date(c)
                .map(|d| d.pred_opt().unwrap_or(d))
                .unwrap_or(to)
        });
        if from > to {
            return Err(ApiError::BadRequest(
                "Startdatum liegt nach dem Enddatum".into(),
            ));
        }
        Ok((from, to))
    }
}
