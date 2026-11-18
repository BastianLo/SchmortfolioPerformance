//! Pure domain logic of SchmortfolioPerformance: data model, valuation and
//! performance calculations. No IO, no clock, no database.

pub mod fx;
pub mod index;
pub mod math;
pub mod model;
pub mod money;
pub mod performance;
pub mod security;
pub mod snapshot;

pub use fx::{CurrencyConverter, ExchangeRateProvider, ExchangeRateSeries};
pub use model::*;
pub use money::Money;
