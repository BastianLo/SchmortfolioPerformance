//! Domain model, closely following the Portfolio Performance data model so
//! that imports are lossless.

use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::Money;

pub type Id = String;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Client {
    pub version: i32,
    pub base_currency: String,
    pub securities: Vec<Security>,
    pub accounts: Vec<Account>,
    pub portfolios: Vec<Portfolio>,
    pub taxonomies: Vec<Taxonomy>,
    pub watchlists: Vec<Watchlist>,
    #[serde(default)]
    pub plans: Vec<crate::plan::InvestmentPlan>,
    /// Data that is kept for lossless round trips but not (yet) interpreted:
    /// investment plans, dashboards, settings, properties.
    #[serde(default)]
    pub extra: serde_json::Value,
}

impl Client {
    pub fn security(&self, id: &str) -> Option<&Security> {
        self.securities.iter().find(|s| s.id == id)
    }

    pub fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn portfolio(&self, id: &str) -> Option<&Portfolio> {
        self.portfolios.iter().find(|p| p.id == id)
    }

    /// Sorts prices and transactions; must be called after construction.
    pub fn normalize(&mut self) {
        for s in &mut self.securities {
            s.prices.sort_by_key(|p| p.date);
            s.prices.dedup_by_key(|p| p.date);
        }
        for a in &mut self.accounts {
            a.transactions.sort_by_key(|t| t.date_time);
        }
        for p in &mut self.portfolios {
            p.transactions.sort_by_key(|t| t.date_time);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecurityPrice {
    pub date: NaiveDate,
    /// quote with factor 1e8
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestSecurityPrice {
    pub date: NaiveDate,
    pub value: i64,
    pub high: i64,
    pub low: i64,
    pub volume: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SecurityEventKind {
    StockSplit,
    Note,
    DividendPayment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub kind: SecurityEventKind,
    pub date: NaiveDate,
    pub details: Option<String>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Security {
    pub id: Id,
    pub name: String,
    /// `None` for indices (no currency).
    pub currency: Option<String>,
    pub target_currency: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    pub ticker: Option<String>,
    pub online_id: Option<String>,
    pub calendar: Option<String>,
    pub feed: Option<String>,
    pub feed_url: Option<String>,
    pub latest_feed: Option<String>,
    pub latest_feed_url: Option<String>,
    pub note: Option<String>,
    pub retired: bool,
    pub prices: Vec<SecurityPrice>,
    pub latest: Option<LatestSecurityPrice>,
    pub events: Vec<SecurityEvent>,
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub updated_at: Option<String>,
}

impl Security {
    pub fn currency_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.currency.as_deref().unwrap_or(fallback)
    }

    /// Same semantics as PP's `Security#getSecurityPrice`.
    pub fn price_at(&self, date: NaiveDate) -> SecurityPrice {
        let last_historic = self.prices.last();

        if let Some(latest) = &self.latest {
            let use_latest = match last_historic {
                None => true,
                Some(lh) => date >= latest.date && latest.date >= lh.date,
            };
            if use_latest {
                return SecurityPrice {
                    date: latest.date,
                    value: latest.value,
                };
            }
        }

        let Some(lh) = last_historic else {
            return SecurityPrice { date, value: 0 };
        };
        if lh.date <= date {
            return *lh;
        }
        match self.prices.binary_search_by_key(&date, |p| p.date) {
            Ok(i) => self.prices[i],
            Err(0) => self.prices[0],
            Err(i) => self.prices[i - 1],
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Account {
    pub id: Id,
    pub name: String,
    pub currency: String,
    pub note: Option<String>,
    pub retired: bool,
    pub transactions: Vec<AccountTransaction>,
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Portfolio {
    pub id: Id,
    pub name: String,
    pub note: Option<String>,
    pub retired: bool,
    pub reference_account: Option<Id>,
    pub transactions: Vec<PortfolioTransaction>,
    #[serde(default)]
    pub attributes: serde_json::Value,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountTransactionType {
    Deposit,
    Removal,
    Interest,
    InterestCharge,
    Dividends,
    Fees,
    FeesRefund,
    Taxes,
    TaxRefund,
    Buy,
    Sell,
    TransferIn,
    TransferOut,
}

impl AccountTransactionType {
    pub fn is_credit(self) -> bool {
        use AccountTransactionType::*;
        matches!(
            self,
            Deposit | Interest | Dividends | FeesRefund | TaxRefund | Sell | TransferIn
        )
    }

    pub fn as_str(self) -> &'static str {
        use AccountTransactionType::*;
        match self {
            Deposit => "DEPOSIT",
            Removal => "REMOVAL",
            Interest => "INTEREST",
            InterestCharge => "INTEREST_CHARGE",
            Dividends => "DIVIDENDS",
            Fees => "FEES",
            FeesRefund => "FEES_REFUND",
            Taxes => "TAXES",
            TaxRefund => "TAX_REFUND",
            Buy => "BUY",
            Sell => "SELL",
            TransferIn => "TRANSFER_IN",
            TransferOut => "TRANSFER_OUT",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        use AccountTransactionType::*;
        Some(match s {
            "DEPOSIT" => Deposit,
            "REMOVAL" => Removal,
            "INTEREST" => Interest,
            "INTEREST_CHARGE" => InterestCharge,
            "DIVIDENDS" | "DIVIDEND" => Dividends,
            "FEES" => Fees,
            "FEES_REFUND" => FeesRefund,
            "TAXES" => Taxes,
            "TAX_REFUND" => TaxRefund,
            "BUY" => Buy,
            "SELL" => Sell,
            "TRANSFER_IN" => TransferIn,
            "TRANSFER_OUT" => TransferOut,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PortfolioTransactionType {
    Buy,
    Sell,
    TransferIn,
    TransferOut,
    DeliveryInbound,
    DeliveryOutbound,
}

impl PortfolioTransactionType {
    pub fn is_purchase(self) -> bool {
        use PortfolioTransactionType::*;
        matches!(self, Buy | TransferIn | DeliveryInbound)
    }

    pub fn as_str(self) -> &'static str {
        use PortfolioTransactionType::*;
        match self {
            Buy => "BUY",
            Sell => "SELL",
            TransferIn => "TRANSFER_IN",
            TransferOut => "TRANSFER_OUT",
            DeliveryInbound => "DELIVERY_INBOUND",
            DeliveryOutbound => "DELIVERY_OUTBOUND",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        use PortfolioTransactionType::*;
        Some(match s {
            "BUY" => Buy,
            "SELL" => Sell,
            "TRANSFER_IN" => TransferIn,
            "TRANSFER_OUT" => TransferOut,
            "DELIVERY_INBOUND" => DeliveryInbound,
            "DELIVERY_OUTBOUND" => DeliveryOutbound,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnitType {
    GrossValue,
    Tax,
    Fee,
}

impl UnitType {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitType::GrossValue => "GROSS_VALUE",
            UnitType::Tax => "TAX",
            UnitType::Fee => "FEE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "GROSS_VALUE" => UnitType::GrossValue,
            "TAX" => UnitType::Tax,
            "FEE" => UnitType::Fee,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionUnit {
    pub kind: UnitType,
    /// amount in the currency of the transaction
    pub amount: Money,
    pub forex: Option<Money>,
    pub exchange_rate: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CrossEntryKind {
    BuySell,
    AccountTransfer,
    PortfolioTransfer,
}

impl CrossEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CrossEntryKind::BuySell => "BUY_SELL",
            CrossEntryKind::AccountTransfer => "ACCOUNT_TRANSFER",
            CrossEntryKind::PortfolioTransfer => "PORTFOLIO_TRANSFER",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "BUY_SELL" => CrossEntryKind::BuySell,
            "ACCOUNT_TRANSFER" => CrossEntryKind::AccountTransfer,
            "PORTFOLIO_TRANSFER" => CrossEntryKind::PortfolioTransfer,
            _ => return None,
        })
    }
}

/// Link to the counterpart of a paired transaction (buy/sell, transfers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossEntry {
    pub kind: CrossEntryKind,
    /// owner (account or portfolio) of the other transaction
    pub other_owner: Id,
    pub other_transaction: Id,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCommon {
    pub id: Id,
    pub date_time: NaiveDateTime,
    pub currency: String,
    /// amount (factor 100), always positive
    pub amount: i64,
    /// shares (factor 1e8)
    pub shares: i64,
    pub security: Option<Id>,
    pub note: Option<String>,
    pub source: Option<String>,
    pub units: Vec<TransactionUnit>,
    pub cross_entry: Option<CrossEntry>,
    pub updated_at: Option<String>,
}

impl TransactionCommon {
    pub fn date(&self) -> NaiveDate {
        self.date_time.date()
    }

    pub fn unit_sum(&self, kind: UnitType) -> i64 {
        self.units
            .iter()
            .filter(|u| u.kind == kind)
            .map(|u| u.amount.amount)
            .sum()
    }

    pub fn unit(&self, kind: UnitType) -> Option<&TransactionUnit> {
        self.units.iter().find(|u| u.kind == kind)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountTransaction {
    pub kind: AccountTransactionType,
    #[serde(flatten)]
    pub common: TransactionCommon,
    pub ex_date: Option<NaiveDateTime>,
}

impl std::ops::Deref for AccountTransaction {
    type Target = TransactionCommon;
    fn deref(&self) -> &TransactionCommon {
        &self.common
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioTransaction {
    pub kind: PortfolioTransactionType,
    #[serde(flatten)]
    pub common: TransactionCommon,
}

impl std::ops::Deref for PortfolioTransaction {
    type Target = TransactionCommon;
    fn deref(&self) -> &TransactionCommon {
        &self.common
    }
}

impl PortfolioTransaction {
    /// Value without fees and taxes (PP: `getGrossValueAmount`).
    pub fn gross_value_amount(&self) -> i64 {
        let tf = self.unit_sum(UnitType::Fee) + self.unit_sum(UnitType::Tax);
        if self.kind.is_purchase() {
            self.amount - tf
        } else {
            self.amount + tf
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taxonomy {
    pub id: Id,
    pub name: String,
    pub source: Option<String>,
    pub dimensions: Vec<String>,
    /// flat list; the root has `parent_id == None`
    pub classifications: Vec<Classification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub id: Id,
    pub parent_id: Option<Id>,
    pub name: String,
    pub note: Option<String>,
    pub color: Option<String>,
    /// weight of this classification within its parent (basis 10000)
    pub weight: i32,
    pub rank: i32,
    pub assignments: Vec<Assignment>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    /// security or account id
    pub vehicle: Id,
    /// basis 10000 = 100 %
    pub weight: i32,
    pub rank: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watchlist {
    pub name: String,
    pub securities: Vec<Id>,
}
