//! Shared application state and per-client model cache.

use std::collections::HashMap;
use std::sync::Arc;

use pp_core::fx::ExchangeRateProvider;
use pp_core::model::{Client, UnitType};
use pp_core::{CurrencyConverter, ExchangeRateSeries};
use pp_db::{ClientMeta, Db, User};
use tokio::sync::{Mutex, RwLock};

use crate::auth::oidc::Oidc;
use crate::config::Config;
use crate::error::{ApiError, ApiResult};

pub struct LoadedClient {
    pub meta: ClientMeta,
    pub client: Client,
    pub provider: Arc<ExchangeRateProvider>,
    fx_version: u64,
}

impl LoadedClient {
    pub fn converter(&self) -> CurrencyConverter {
        CurrencyConverter::new(self.provider.clone(), self.client.base_currency.clone())
    }
}

pub struct AppState {
    pub config: Config,
    pub db: Db,
    pub oidc: Option<Oidc>,
    cache: RwLock<HashMap<String, Arc<LoadedClient>>>,
    fx: RwLock<(u64, Arc<Vec<ExchangeRateSeries>>)>,
    pub fx_update: Mutex<()>,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub async fn new(config: Config, db: Db, oidc: Option<Oidc>) -> anyhow::Result<Self> {
        let series = db.exchange_rates().await?;
        Ok(Self {
            config,
            db,
            oidc,
            cache: RwLock::new(HashMap::new()),
            fx: RwLock::new((1, Arc::new(series))),
            fx_update: Mutex::new(()),
        })
    }

    pub async fn reload_exchange_rates(&self) -> anyhow::Result<()> {
        let series = self.db.exchange_rates().await?;
        let mut fx = self.fx.write().await;
        *fx = (fx.0 + 1, Arc::new(series));
        Ok(())
    }

    pub async fn exchange_rate_series(&self) -> Arc<Vec<ExchangeRateSeries>> {
        self.fx.read().await.1.clone()
    }

    /// Loads a client (checking ownership), cached by data version.
    pub async fn client(&self, user: &User, id: &str) -> ApiResult<Arc<LoadedClient>> {
        let meta = self.db.client_meta(id).await?;
        if meta.owner_id != user.id {
            return Err(ApiError::NotFound);
        }
        let (fx_version, series) = {
            let fx = self.fx.read().await;
            (fx.0, fx.1.clone())
        };
        if let Some(c) = self.cache.read().await.get(id)
            && c.meta.data_version == meta.data_version
            && c.fx_version == fx_version
        {
            return Ok(c.clone());
        }
        let client = self.db.load_client(id).await?;
        let provider = Arc::new(build_provider(&series, &client));
        let loaded = Arc::new(LoadedClient {
            meta,
            client,
            provider,
            fx_version,
        });
        self.cache
            .write()
            .await
            .insert(id.to_string(), loaded.clone());
        Ok(loaded)
    }

    pub async fn invalidate(&self, id: &str) {
        self.cache.write().await.remove(id);
    }
}

/// Combines the global exchange rates with rates derived from the
/// transactions of the client (used if no global rate exists for a pair).
pub fn build_provider(global: &[ExchangeRateSeries], client: &Client) -> ExchangeRateProvider {
    let mut series: Vec<ExchangeRateSeries> = global.to_vec();
    let global_provider = ExchangeRateProvider::new(global.to_vec());
    let probe = chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let mut derived: HashMap<(String, String), ExchangeRateSeries> = HashMap::new();

    let mut add = |units: &[pp_core::model::TransactionUnit], date: chrono::NaiveDate| {
        for u in units.iter().filter(|u| u.kind == UnitType::GrossValue) {
            let (Some(fx), Some(rate)) = (&u.forex, u.exchange_rate) else {
                continue;
            };
            if fx.currency == u.amount.currency || rate.is_zero() {
                continue;
            }
            if global_provider
                .rate(probe, &fx.currency, &u.amount.currency)
                .is_some()
            {
                continue;
            }
            derived
                .entry((fx.currency.clone(), u.amount.currency.clone()))
                .or_insert_with(|| {
                    ExchangeRateSeries::new(fx.currency.clone(), u.amount.currency.clone())
                })
                .add(date, rate);
        }
    };
    for a in &client.accounts {
        for t in &a.transactions {
            add(&t.units, t.date());
        }
    }
    for p in &client.portfolios {
        for t in &p.transactions {
            add(&t.units, t.date());
        }
    }
    series.extend(derived.into_values());
    ExchangeRateProvider::new(series)
}
