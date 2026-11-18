//! Reports: dashboard, holdings, performance, earnings, taxonomies.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{Datelike, NaiveDate};
use pp_core::index::{Interval, PerformanceIndex};
use pp_core::model::{AccountTransactionType, Client, Taxonomy};
use pp_core::performance::client_performance;
use pp_core::security::{SecurityPerformance, earnings};
use pp_core::snapshot::ClientSnapshot;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::{PeriodQuery, today};
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::{LoadedClient, SharedState};

fn nan_none(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

// ---------------------------------------------------------- performance kpis

#[derive(Serialize, ToSchema)]
pub struct PerformanceKpis {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub currency: String,
    pub initial_value: i64,
    pub final_value: i64,
    pub absolute_delta: i64,
    /// True time-weighted rate of return, cumulated
    pub ttwror: f64,
    pub ttwror_annualized: f64,
    pub irr: Option<f64>,
    pub capital_gains: i64,
    pub realized_capital_gains: i64,
    pub unrealized_capital_gains: i64,
    pub earnings: i64,
    pub dividends: i64,
    pub interest: i64,
    pub fees: i64,
    pub taxes: i64,
    pub transfers: i64,
    pub deposits: i64,
    pub removals: i64,
    pub max_drawdown: f64,
    pub max_drawdown_start: NaiveDate,
    pub max_drawdown_end: NaiveDate,
    pub max_drawdown_duration_days: i64,
    pub volatility: f64,
    pub semi_deviation: f64,
}

fn kpis(
    c: &LoadedClient,
    from: NaiveDate,
    to: NaiveDate,
    idx: &PerformanceIndex,
) -> PerformanceKpis {
    let conv = c.converter();
    let p = client_performance(&c.client, &conv, from, to);
    let dd = idx.drawdown();
    let vol = idx.volatility();
    PerformanceKpis {
        from,
        to,
        currency: p.currency.clone(),
        initial_value: p.initial_value,
        final_value: p.final_value,
        absolute_delta: p.absolute_delta,
        ttwror: idx.final_accumulated(),
        ttwror_annualized: idx.final_accumulated_annualized(),
        irr: nan_none(p.irr),
        capital_gains: p.capital_gains,
        realized_capital_gains: p.realized_capital_gains,
        unrealized_capital_gains: p.unrealized_capital_gains,
        earnings: p.earnings,
        dividends: p.dividends,
        interest: p.interest,
        fees: p.fees,
        taxes: p.taxes,
        transfers: p.transfers,
        deposits: p.deposits,
        removals: p.removals,
        max_drawdown: dd.max_drawdown,
        max_drawdown_start: dd.max_drawdown_interval.start,
        max_drawdown_end: dd.max_drawdown_interval.end,
        max_drawdown_duration_days: (dd.max_drawdown_duration.end - dd.max_drawdown_duration.start)
            .num_days(),
        volatility: vol.annualized_standard_deviation,
        semi_deviation: vol.semi_deviation,
    }
}

#[derive(Serialize, ToSchema)]
pub struct SeriesPoint {
    pub date: NaiveDate,
    /// total assets
    pub total: i64,
    /// initial value plus cumulated transfers
    pub invested: i64,
    /// cumulated TTWROR
    pub ttwror: f64,
    /// return since previous point
    pub delta: f64,
    pub drawdown: f64,
}

fn series(idx: &PerformanceIndex, interval: Interval) -> Vec<SeriesPoint> {
    let dd = idx.drawdown();
    let mut invested = Vec::with_capacity(idx.totals.len());
    let mut acc = idx.totals.first().copied().unwrap_or(0);
    for i in 0..idx.totals.len() {
        if i > 0 {
            acc += idx.inbound_transferals[i] - idx.outbound_transferals[i];
        }
        invested.push(acc);
    }
    let mut prev = 0.0;
    idx.sample_indices(interval)
        .into_iter()
        .map(|i| {
            let a = idx.accumulated[i];
            let delta = (1.0 + a) / (1.0 + prev) - 1.0;
            prev = a;
            SeriesPoint {
                date: idx.dates[i],
                total: idx.totals[i],
                invested: invested[i],
                ttwror: a,
                delta,
                drawdown: dd.series.get(i).copied().unwrap_or(0.0),
            }
        })
        .collect()
}

fn auto_interval(from: NaiveDate, to: NaiveDate) -> Interval {
    let days = (to - from).num_days();
    if days > 365 * 6 {
        Interval::Week
    } else {
        Interval::Day
    }
}

// --------------------------------------------------------------- allocation

#[derive(Serialize, ToSchema, Clone)]
pub struct AllocationSlice {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub value: i64,
    pub share: f64,
}

#[derive(Serialize, ToSchema)]
pub struct TaxonomyNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub color: Option<String>,
    pub depth: usize,
    /// value incl. children
    pub value: i64,
    /// share of total assets
    pub share: f64,
    /// share within parent
    pub share_of_parent: f64,
    /// target weight within parent (basis 10000)
    pub weight: i32,
    pub assignments: Vec<AssignmentValue>,
}

#[derive(Serialize, ToSchema)]
pub struct AssignmentValue {
    pub vehicle_id: String,
    pub name: String,
    pub weight: i32,
    pub value: i64,
}

fn vehicle_values(client: &Client, snap: &ClientSnapshot) -> HashMap<String, (String, i64)> {
    let mut m = HashMap::new();
    for p in snap.joint_positions() {
        let name = client
            .security(&p.security_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        m.insert(p.security_id.clone(), (name, p.value_converted));
    }
    for a in &snap.accounts {
        let name = client
            .account(&a.account_id)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        m.insert(a.account_id.clone(), (name, a.funds_converted));
    }
    m
}

fn taxonomy_tree(client: &Client, t: &Taxonomy, snap: &ClientSnapshot) -> Vec<TaxonomyNode> {
    let values = vehicle_values(client, snap);
    let total = snap.total();
    let mut children: HashMap<Option<&str>, Vec<usize>> = HashMap::new();
    for (i, c) in t.classifications.iter().enumerate() {
        children.entry(c.parent_id.as_deref()).or_default().push(i);
    }
    for v in children.values_mut() {
        v.sort_by_key(|i| t.classifications[*i].rank);
    }
    fn value_of(
        t: &Taxonomy,
        i: usize,
        children: &HashMap<Option<&str>, Vec<usize>>,
        values: &HashMap<String, (String, i64)>,
        memo: &mut HashMap<usize, i64>,
    ) -> i64 {
        if let Some(v) = memo.get(&i) {
            return *v;
        }
        let c = &t.classifications[i];
        let own: i64 = c
            .assignments
            .iter()
            .map(|a| {
                values
                    .get(&a.vehicle)
                    .map(|v| (v.1 as i128 * a.weight as i128 / 10000) as i64)
                    .unwrap_or(0)
            })
            .sum();
        let kids: i64 = children
            .get(&Some(c.id.as_str()))
            .map(|k| {
                k.iter()
                    .map(|j| value_of(t, *j, children, values, memo))
                    .sum()
            })
            .unwrap_or(0);
        memo.insert(i, own + kids);
        own + kids
    }
    let mut memo = HashMap::new();
    let mut out = Vec::new();
    fn walk(
        t: &Taxonomy,
        i: usize,
        depth: usize,
        parent_value: i64,
        total: i64,
        children: &HashMap<Option<&str>, Vec<usize>>,
        values: &HashMap<String, (String, i64)>,
        memo: &mut HashMap<usize, i64>,
        out: &mut Vec<TaxonomyNode>,
    ) {
        let c = &t.classifications[i];
        let value = value_of(t, i, children, values, memo);
        out.push(TaxonomyNode {
            id: c.id.clone(),
            parent_id: c.parent_id.clone(),
            name: c.name.clone(),
            color: c.color.clone(),
            depth,
            value,
            share: if total != 0 {
                value as f64 / total as f64
            } else {
                0.0
            },
            share_of_parent: if parent_value != 0 {
                value as f64 / parent_value as f64
            } else {
                0.0
            },
            weight: c.weight,
            assignments: c
                .assignments
                .iter()
                .map(|a| {
                    let (name, v) = values.get(&a.vehicle).cloned().unwrap_or_default();
                    AssignmentValue {
                        vehicle_id: a.vehicle.clone(),
                        name,
                        weight: a.weight,
                        value: (v as i128 * a.weight as i128 / 10000) as i64,
                    }
                })
                .collect(),
        });
        if let Some(k) = children.get(&Some(c.id.as_str())) {
            for j in k.clone() {
                walk(t, j, depth + 1, value, total, children, values, memo, out);
            }
        }
    }
    if let Some(roots) = children.get(&None).cloned() {
        for r in roots {
            walk(
                t, r, 0, total, total, &children, &values, &mut memo, &mut out,
            );
        }
    }
    out
}

fn allocation(
    client: &Client,
    snap: &ClientSnapshot,
    taxonomy: Option<&str>,
) -> (Option<String>, Vec<AllocationSlice>) {
    let total = snap.total();
    let share = |v: i64| {
        if total != 0 {
            v as f64 / total as f64
        } else {
            0.0
        }
    };
    let t = match taxonomy {
        Some(id) => client.taxonomies.iter().find(|t| t.id == id),
        None => client.taxonomies.first(),
    };
    if let Some(t) = t {
        let tree = taxonomy_tree(client, t, snap);
        let mut slices: Vec<AllocationSlice> = tree
            .iter()
            .filter(|n| n.depth == 1 && n.value != 0)
            .map(|n| AllocationSlice {
                id: n.id.clone(),
                name: n.name.clone(),
                color: n.color.clone(),
                value: n.value,
                share: share(n.value),
            })
            .collect();
        let assigned: i64 = slices.iter().map(|s| s.value).sum();
        if total - assigned > 100 {
            slices.push(AllocationSlice {
                id: "unassigned".into(),
                name: "Ohne Klassifizierung".into(),
                color: Some("#9ca3af".into()),
                value: total - assigned,
                share: share(total - assigned),
            });
        }
        return (Some(t.id.clone()), slices);
    }
    let cash = snap.total_cash();
    let sec = snap.total_securities();
    (
        None,
        vec![
            AllocationSlice {
                id: "securities".into(),
                name: "Wertpapiere".into(),
                color: None,
                value: sec,
                share: share(sec),
            },
            AllocationSlice {
                id: "cash".into(),
                name: "Barvermögen".into(),
                color: None,
                value: cash,
                share: share(cash),
            },
        ],
    )
}

// ------------------------------------------------------------------ holdings

#[derive(Serialize, ToSchema, Clone)]
pub struct HoldingRow {
    pub security_id: String,
    pub name: String,
    pub isin: Option<String>,
    pub ticker: Option<String>,
    pub currency: String,
    pub shares: i64,
    pub quote: i64,
    pub quote_date: NaiveDate,
    /// change of the quote against the previous quote
    pub quote_change: Option<f64>,
    /// in client currency
    pub market_value: i64,
    pub share: f64,
    pub fifo_cost: i64,
    pub gain: i64,
    pub gain_percent: f64,
    pub dividends: i64,
    pub realized_gains: i64,
    pub irr: Option<f64>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct CashRow {
    pub account_id: String,
    pub name: String,
    pub currency: String,
    pub balance: i64,
    pub balance_converted: i64,
    pub share: f64,
}

#[derive(Serialize, ToSchema)]
pub struct Holdings {
    pub date: NaiveDate,
    pub currency: String,
    pub total: i64,
    pub securities_value: i64,
    pub cash_value: i64,
    pub positions: Vec<HoldingRow>,
    pub cash: Vec<CashRow>,
}

fn holdings(c: &LoadedClient, date: NaiveDate) -> Holdings {
    let conv = c.converter();
    let snap = ClientSnapshot::create(&c.client, &conv, date);
    let records = SecurityPerformance::calculate(&c.client, &conv, NaiveDate::MIN, date);
    let total = snap.total();
    let share = |v: i64| {
        if total != 0 {
            v as f64 / total as f64
        } else {
            0.0
        }
    };
    let mut positions: Vec<HoldingRow> = snap
        .joint_positions()
        .iter()
        .filter_map(|p| {
            let s = c.client.security(&p.security_id)?;
            let r = records.iter().find(|r| r.security_id == p.security_id);
            let prev = s
                .prices
                .iter()
                .rev()
                .find(|x| x.date < p.price_date)
                .map(|x| x.value);
            let fifo = r.map(|r| r.fifo_cost).unwrap_or(0);
            Some(HoldingRow {
                security_id: s.id.clone(),
                name: s.name.clone(),
                isin: s.isin.clone(),
                ticker: s.ticker.clone(),
                currency: p.currency.clone(),
                shares: p.shares,
                quote: p.price,
                quote_date: p.price_date,
                quote_change: prev
                    .filter(|v| *v != 0)
                    .map(|v| p.price as f64 / v as f64 - 1.0),
                market_value: p.value_converted,
                share: share(p.value_converted),
                fifo_cost: fifo,
                gain: p.value_converted - fifo,
                gain_percent: if fifo != 0 {
                    (p.value_converted - fifo) as f64 / fifo as f64
                } else {
                    0.0
                },
                dividends: r.map(|r| r.dividends).unwrap_or(0),
                realized_gains: r.map(|r| r.realized_gains).unwrap_or(0),
                irr: r.and_then(|r| nan_none(r.irr)),
            })
        })
        .collect();
    positions.sort_by_key(|p| std::cmp::Reverse(p.market_value));
    let cash = snap
        .accounts
        .iter()
        .filter(|a| a.funds != 0)
        .map(|a| CashRow {
            account_id: a.account_id.clone(),
            name: c
                .client
                .account(&a.account_id)
                .map(|x| x.name.clone())
                .unwrap_or_default(),
            currency: a.currency.clone(),
            balance: a.funds,
            balance_converted: a.funds_converted,
            share: share(a.funds_converted),
        })
        .collect();
    Holdings {
        date,
        currency: conv.term().to_string(),
        total,
        securities_value: snap.total_securities(),
        cash_value: snap.total_cash(),
        positions,
        cash,
    }
}

#[derive(Deserialize, IntoParams)]
struct DateQuery {
    /// valuation date (default today)
    date: Option<NaiveDate>,
    /// restrict to portfolios / accounts (comma separated ids)
    filter: Option<String>,
}

#[utoipa::path(get, path = "/clients/{id}/reports/holdings", tag = "reports", params(("id" = String, Path), DateQuery), responses((status = 200, body = Holdings)))]
async fn get_holdings(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<DateQuery>,
) -> ApiResult<Json<Holdings>> {
    let c = state
        .client(&user, &id)
        .await?
        .filtered(q.filter.as_deref());
    let date = q.date.unwrap_or_else(today);
    Ok(Json(
        tokio::task::spawn_blocking(move || holdings(&c, date))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

// ----------------------------------------------------------------- dashboard

#[derive(Serialize, ToSchema)]
pub struct EarningRow {
    pub date: NaiveDate,
    pub kind: String,
    pub security_id: Option<String>,
    pub name: String,
    pub account_id: String,
    pub amount: i64,
    pub gross: i64,
    pub taxes: i64,
    pub fees: i64,
}

#[derive(Serialize, ToSchema)]
pub struct Dashboard {
    pub date: NaiveDate,
    pub currency: String,
    pub total: i64,
    pub securities_value: i64,
    pub cash_value: i64,
    /// change of total assets since the previous day (without transfers)
    pub day_change: i64,
    pub day_change_percent: f64,
    pub kpis: PerformanceKpis,
    pub series: Vec<SeriesPoint>,
    pub taxonomy_id: Option<String>,
    pub allocation: Vec<AllocationSlice>,
    pub top_positions: Vec<HoldingRow>,
    pub recent_earnings: Vec<EarningRow>,
    pub earnings_last_12_months: i64,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, IntoParams)]
struct DashboardQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    taxonomy: Option<String>,
    /// restrict to portfolios / accounts (comma separated ids)
    filter: Option<String>,
}

fn earning_rows(c: &Client, entries: &[pp_core::security::EarningEntry]) -> Vec<EarningRow> {
    entries
        .iter()
        .map(|e| EarningRow {
            date: e.date,
            kind: e.kind.as_str().into(),
            security_id: e.security_id.clone(),
            name: e
                .security_id
                .as_deref()
                .and_then(|s| c.security(s))
                .map(|s| s.name.clone())
                .or_else(|| c.account(&e.account_id).map(|a| a.name.clone()))
                .unwrap_or_default(),
            account_id: e.account_id.clone(),
            amount: e.amount,
            gross: e.gross,
            taxes: e.taxes,
            fees: e.fees,
        })
        .collect()
}

fn dashboard(
    c: &LoadedClient,
    from: NaiveDate,
    to: NaiveDate,
    taxonomy: Option<&str>,
) -> Dashboard {
    let conv = c.converter();
    let today = today();
    let idx = PerformanceIndex::for_client(&c.client, &conv, from, to, today);
    let k = kpis(c, from, to, &idx);
    let snap = ClientSnapshot::create(&c.client, &conv, to.min(today));
    let (taxonomy_id, allocation) = allocation(&c.client, &snap, taxonomy);
    let h = holdings(c, to.min(today));

    let n = idx.totals.len();
    let (day_change, day_change_percent) = if n >= 2 {
        let i = n - 1;
        let change = idx.totals[i] - idx.totals[i - 1] - idx.inbound_transferals[i]
            + idx.outbound_transferals[i];
        (change, idx.delta[i])
    } else {
        (0, 0.0)
    };

    let year_ago = to.with_year(to.year() - 1).unwrap_or(to);
    let e12 = earnings(&c.client, &conv, year_ago, to);
    let mut recent = earnings(&c.client, &conv, NaiveDate::MIN, to);
    recent.reverse();
    recent.truncate(8);

    let mut warnings: Vec<String> = idx.warnings.iter().take(5).cloned().collect();
    for m in conv.missing_pairs() {
        warnings.push(format!(
            "Kein Wechselkurs für {m} verfügbar, es wird 1:1 umgerechnet"
        ));
    }

    Dashboard {
        date: snap.date,
        currency: conv.term().to_string(),
        total: snap.total(),
        securities_value: snap.total_securities(),
        cash_value: snap.total_cash(),
        day_change,
        day_change_percent,
        series: series(&idx, auto_interval(from, to)),
        kpis: k,
        taxonomy_id,
        allocation,
        top_positions: h.positions.into_iter().take(6).collect(),
        recent_earnings: earning_rows(&c.client, &recent),
        earnings_last_12_months: e12.iter().map(|e| e.amount).sum(),
        warnings,
    }
}

#[utoipa::path(get, path = "/clients/{id}/reports/dashboard", tag = "reports", params(("id" = String, Path), DashboardQuery), responses((status = 200, body = Dashboard)))]
async fn get_dashboard(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<DashboardQuery>,
) -> ApiResult<Json<Dashboard>> {
    let c = state
        .client(&user, &id)
        .await?
        .filtered(q.filter.as_deref());
    let (from, to) = PeriodQuery {
        from: q.from,
        to: q.to,
        filter: None,
    }
    .resolve(&c.client)?;
    let taxonomy = q.taxonomy.clone();
    Ok(Json(
        tokio::task::spawn_blocking(move || dashboard(&c, from, to, taxonomy.as_deref()))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?,
    ))
}

// --------------------------------------------------------------- performance

#[derive(Serialize, ToSchema)]
pub struct PeriodReturn {
    pub year: i32,
    /// 1-12; 0 for the whole year
    pub month: u32,
    pub ttwror: f64,
}

#[derive(Serialize, ToSchema)]
pub struct PerformanceReport {
    pub kpis: PerformanceKpis,
    pub interval: String,
    pub series: Vec<SeriesPoint>,
    pub returns: Vec<PeriodReturn>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, IntoParams)]
struct PerformanceQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    /// day, week, month, year (default: automatic)
    interval: Option<String>,
    /// restrict to portfolios / accounts (comma separated ids)
    filter: Option<String>,
}

fn period_returns(idx: &PerformanceIndex) -> Vec<PeriodReturn> {
    let mut out = Vec::new();
    let mut month_start = 0.0f64;
    let mut year_start = 0.0f64;
    for i in 0..idx.dates.len() {
        let d = idx.dates[i];
        let last_of_month = i + 1 == idx.dates.len() || idx.dates[i + 1].month() != d.month();
        let last_of_year = i + 1 == idx.dates.len() || idx.dates[i + 1].year() != d.year();
        let a = idx.accumulated[i];
        if last_of_month {
            out.push(PeriodReturn {
                year: d.year(),
                month: d.month(),
                ttwror: (1.0 + a) / (1.0 + month_start) - 1.0,
            });
            month_start = a;
        }
        if last_of_year {
            out.push(PeriodReturn {
                year: d.year(),
                month: 0,
                ttwror: (1.0 + a) / (1.0 + year_start) - 1.0,
            });
            year_start = a;
        }
    }
    out
}

#[utoipa::path(get, path = "/clients/{id}/reports/performance", tag = "reports", params(("id" = String, Path), PerformanceQuery), responses((status = 200, body = PerformanceReport)))]
async fn get_performance(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<PerformanceQuery>,
) -> ApiResult<Json<PerformanceReport>> {
    let c = state
        .client(&user, &id)
        .await?
        .filtered(q.filter.as_deref());
    let (from, to) = PeriodQuery {
        from: q.from,
        to: q.to,
        filter: None,
    }
    .resolve(&c.client)?;
    let report = tokio::task::spawn_blocking(move || {
        let conv = c.converter();
        let idx = PerformanceIndex::for_client(&c.client, &conv, from, to, today());
        let interval = match q.interval.as_deref() {
            Some("day") => Interval::Day,
            Some("week") => Interval::Week,
            Some("month") => Interval::Month,
            Some("year") => Interval::Year,
            _ => auto_interval(from, to),
        };
        let mut warnings = idx.warnings.clone();
        warnings.extend(
            conv.missing_pairs()
                .into_iter()
                .map(|m| format!("Kein Wechselkurs für {m} verfügbar")),
        );
        PerformanceReport {
            kpis: kpis(&c, from, to, &idx),
            interval: format!("{interval:?}").to_lowercase(),
            series: series(&idx, interval),
            returns: period_returns(&idx),
            warnings,
        }
    })
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(report))
}

// ------------------------------------------------------------------ earnings

#[derive(Serialize, ToSchema)]
pub struct EarningsBucket {
    pub year: i32,
    pub month: u32,
    pub dividends: i64,
    pub interest: i64,
    pub taxes: i64,
}

#[derive(Serialize, ToSchema)]
pub struct EarningsReport {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub currency: String,
    pub total: i64,
    pub total_gross: i64,
    pub total_taxes: i64,
    pub by_month: Vec<EarningsBucket>,
    pub by_security: Vec<AllocationSlice>,
    pub entries: Vec<EarningRow>,
}

#[utoipa::path(get, path = "/clients/{id}/reports/earnings", tag = "reports", params(("id" = String, Path), PeriodQuery), responses((status = 200, body = EarningsReport)))]
async fn get_earnings(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<Json<EarningsReport>> {
    let c = state
        .client(&user, &id)
        .await?
        .filtered(q.filter.as_deref());
    let (from, to) = q.resolve(&c.client)?;
    let conv = c.converter();
    let entries = earnings(&c.client, &conv, from, to);
    let mut buckets: std::collections::BTreeMap<(i32, u32), EarningsBucket> = Default::default();
    // fill all months of the period so the chart has no gaps
    let mut d = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).unwrap();
    while d <= to {
        buckets.insert(
            (d.year(), d.month()),
            EarningsBucket {
                year: d.year(),
                month: d.month(),
                dividends: 0,
                interest: 0,
                taxes: 0,
            },
        );
        d = if d.month() == 12 {
            NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
        };
    }
    let mut by_sec: HashMap<String, i64> = HashMap::new();
    for e in &entries {
        let b = buckets
            .entry((e.date.year(), e.date.month()))
            .or_insert(EarningsBucket {
                year: e.date.year(),
                month: e.date.month(),
                dividends: 0,
                interest: 0,
                taxes: 0,
            });
        if e.kind == AccountTransactionType::Dividends {
            b.dividends += e.amount;
        } else {
            b.interest += e.amount;
        }
        b.taxes += e.taxes;
        *by_sec
            .entry(
                e.security_id
                    .clone()
                    .unwrap_or_else(|| format!("account:{}", e.account_id)),
            )
            .or_default() += e.amount;
    }
    let total: i64 = entries.iter().map(|e| e.amount).sum();
    let mut by_security: Vec<AllocationSlice> = by_sec
        .into_iter()
        .map(|(k, v)| {
            let name = match k.strip_prefix("account:") {
                Some(a) => format!(
                    "Zinsen {}",
                    c.client
                        .account(a)
                        .map(|x| x.name.as_str())
                        .unwrap_or_default()
                ),
                None => c
                    .client
                    .security(&k)
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
            };
            AllocationSlice {
                id: k,
                name,
                color: None,
                value: v,
                share: if total != 0 {
                    v as f64 / total as f64
                } else {
                    0.0
                },
            }
        })
        .collect();
    by_security.sort_by_key(|s| std::cmp::Reverse(s.value));
    let mut rows = earning_rows(&c.client, &entries);
    rows.reverse();
    Ok(Json(EarningsReport {
        from,
        to,
        currency: conv.term().to_string(),
        total,
        total_gross: entries.iter().map(|e| e.gross).sum(),
        total_taxes: entries.iter().map(|e| e.taxes).sum(),
        by_month: buckets.into_values().collect(),
        by_security,
        entries: rows,
    }))
}

// ---------------------------------------------------------------- taxonomies

#[derive(Serialize, ToSchema)]
pub struct TaxonomyReport {
    pub id: String,
    pub name: String,
    pub date: NaiveDate,
    pub total: i64,
    pub nodes: Vec<TaxonomyNode>,
}

#[derive(Serialize, ToSchema)]
pub struct TaxonomyInfo {
    pub id: String,
    pub name: String,
    pub classifications: usize,
}

#[utoipa::path(get, path = "/clients/{id}/taxonomies", tag = "reports", params(("id" = String, Path)), responses((status = 200, body = Vec<TaxonomyInfo>)))]
async fn list_taxonomies(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<TaxonomyInfo>>> {
    let c = state.client(&user, &id).await?;
    Ok(Json(
        c.client
            .taxonomies
            .iter()
            .map(|t| TaxonomyInfo {
                id: t.id.clone(),
                name: t.name.clone(),
                classifications: t.classifications.len(),
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/clients/{id}/reports/taxonomies/{tid}", tag = "reports", params(("id" = String, Path), ("tid" = String, Path), DateQuery), responses((status = 200, body = TaxonomyReport)))]
async fn get_taxonomy(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    Query(q): Query<DateQuery>,
) -> ApiResult<Json<TaxonomyReport>> {
    let c = state
        .client(&user, &id)
        .await?
        .filtered(q.filter.as_deref());
    let t = c
        .client
        .taxonomies
        .iter()
        .find(|t| t.id == tid)
        .ok_or(ApiError::NotFound)?;
    let date = q.date.unwrap_or_else(today);
    let snap = ClientSnapshot::create(&c.client, &c.converter(), date);
    Ok(Json(TaxonomyReport {
        id: t.id.clone(),
        name: t.name.clone(),
        date,
        total: snap.total(),
        nodes: taxonomy_tree(&c.client, t, &snap),
    }))
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(get_dashboard))
        .routes(routes!(get_holdings))
        .routes(routes!(get_performance))
        .routes(routes!(get_earnings))
        .routes(routes!(list_taxonomies))
        .routes(routes!(get_taxonomy))
}
