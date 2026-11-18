//! Numerical helpers: IRR, drawdown, volatility, trading calendar.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Serialize;

fn npv(days: &[f64], values: &[f64], rate: f64) -> f64 {
    days.iter().zip(values).map(|(d, v)| v / rate.powf(d / 365.0)).sum()
}

fn halving(days: &[f64], values: &[f64], left: f64, right: f64, f_left: f64, f_right: f64) -> f64 {
    let (mut l, mut r, mut fl, mut fr) = (left, right, f_left, f_right);
    loop {
        let center = (l + r) / 2.0;
        if r - l < 0.001 {
            return center;
        }
        let fc = npv(days, values, center);
        if fc == 0.0 {
            return center;
        }
        if fc.signum() == fr.signum() {
            r = center;
            fr = fc;
        } else {
            l = center;
            fl = fc;
        }
        let _ = fl;
    }
}

/// Internal rate of return (annualised), same algorithm as PP.
pub fn irr(dates: &[NaiveDate], values: &[f64]) -> f64 {
    if dates.is_empty() || dates.len() != values.len() {
        return f64::NAN;
    }
    let first = dates[0];
    let days: Vec<f64> = dates.iter().map(|d| (*d - first).num_days() as f64).collect();

    let f_left = *values.last().unwrap();
    let f_right: f64 = values.iter().sum();

    let guess = if f_left.signum() == f_right.signum() {
        1.05
    } else {
        halving(&days, values, 0.0, 1.0, f_left, f_right)
    };

    // Newton with pseudo derivative
    let mut xi = guess;
    let mut delta = 1.0f64;
    let mut i = 0;
    while delta > 0.00001 && i < 500 {
        let fxi = npv(&days, values, xi);
        let h = xi.abs() / 1e6;
        let fdxi = (npv(&days, values, xi + h) - npv(&days, values, xi - h)) / (2.0 * h);
        let xi1 = xi - fxi / fdxi;
        delta = (xi1 - xi).abs();
        xi = xi1;
        i += 1;
    }
    xi - 1.0
}

#[derive(Debug, Clone, Serialize)]
pub struct DateInterval {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateInterval {
    fn days(&self) -> i64 {
        (self.end - self.start).num_days()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Drawdown {
    pub max_drawdown: f64,
    pub max_drawdown_interval: DateInterval,
    pub max_drawdown_duration: DateInterval,
    pub longest_recovery: DateInterval,
    pub series: Vec<f64>,
}

impl Drawdown {
    /// `accumulated` are cumulated returns (0.1 = +10 %).
    pub fn new(accumulated: &[f64], dates: &[NaiveDate], start_at: usize) -> Self {
        let n = accumulated.len();
        let start_at = start_at.min(n.saturating_sub(1));
        let mut series = vec![0.0; n];
        if n == 0 {
            let d = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
            let iv = DateInterval { start: d, end: d };
            return Self {
                max_drawdown: 0.0,
                max_drawdown_interval: iv.clone(),
                max_drawdown_duration: iv.clone(),
                longest_recovery: iv,
                series,
            };
        }
        let mut peak = accumulated[start_at] + 1.0;
        let mut bottom = peak;
        let mut last_peak = dates[start_at];
        let mut last_bottom = dates[start_at];
        let mut max_dd = 0.0;
        let mut interval_max_dd = DateInterval { start: last_peak, end: last_peak };
        let mut max_dd_duration = DateInterval { start: last_peak, end: last_peak };
        let mut recovery = DateInterval { start: last_bottom, end: last_peak };
        let mut current_recovery = None;

        for ii in start_at..n {
            let value = accumulated[ii] + 1.0;
            let current_dd = DateInterval { start: last_peak, end: dates[ii] };
            let cr = DateInterval { start: last_bottom, end: dates[ii] };

            if value > peak {
                peak = value;
                last_peak = dates[ii];
                series[ii] = 0.0;
                if cr.days() > recovery.days() {
                    recovery = cr.clone();
                }
                last_bottom = dates[ii];
                bottom = value;
            } else {
                let dd = (peak - value) / peak;
                series[ii] = -dd;
                if dd > max_dd {
                    max_dd = dd;
                    interval_max_dd = DateInterval { start: last_peak, end: dates[ii] };
                }
                if value < bottom {
                    bottom = value;
                    last_bottom = dates[ii];
                }
            }
            if current_dd.days() > max_dd_duration.days() {
                max_dd_duration = current_dd;
            }
            current_recovery = Some(cr);
        }
        if let Some(cr) = current_recovery
            && cr.days() > recovery.days()
        {
            recovery = cr;
        }
        Self {
            max_drawdown: max_dd,
            max_drawdown_interval: interval_max_dd,
            max_drawdown_duration: max_dd_duration,
            longest_recovery: recovery,
            series,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Volatility {
    pub standard_deviation: f64,
    pub semi_deviation: f64,
    pub annualized_standard_deviation: f64,
    pub count: usize,
}

impl Volatility {
    pub fn new(returns: &[f64], filter: impl Fn(usize) -> bool) -> Self {
        let selected: Vec<f64> = (0..returns.len()).filter(|i| filter(*i)).map(|i| (1.0 + returns[i]).ln()).collect();
        let count = selected.len();
        if count <= 1 {
            return Self { standard_deviation: 0.0, semi_deviation: 0.0, annualized_standard_deviation: 0.0, count };
        }
        let avg = selected.iter().sum::<f64>() / count as f64;
        let mut std = 0.0;
        let mut semi = 0.0;
        for l in &selected {
            let add = (l - avg).powi(2);
            std += add;
            if *l < avg {
                semi += add;
            }
        }
        let c = count as f64;
        Self {
            standard_deviation: (std / (c - 1.0) * c).sqrt(),
            semi_deviation: (semi / (c - 1.0) * c).sqrt(),
            annualized_standard_deviation: (std / (c - 1.0) * 252.0).sqrt(),
            count,
        }
    }
}

fn easter_sunday(year: i32) -> NaiveDate {
    // Anonymous Gregorian algorithm
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap()
}

/// PP "default" trading calendar: weekends, New Year, Good Friday, Easter
/// Monday, Labour Day, Christmas Eve and both Christmas days.
pub fn is_holiday(date: NaiveDate) -> bool {
    if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        return true;
    }
    let (m, d) = (date.month(), date.day());
    if matches!((m, d), (1, 1) | (5, 1) | (12, 24) | (12, 25) | (12, 26)) {
        return true;
    }
    let easter = easter_sunday(date.year());
    date == easter - Duration::days(2) || date == easter + Duration::days(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irr_simple() {
        let d0 = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let d1 = NaiveDate::from_ymd_opt(2021, 1, 1).unwrap();
        let r = irr(&[d0, d1], &[-100.0, 110.0]);
        // 366 days in 2020
        assert!((r - (1.1f64.powf(365.0 / 366.0) - 1.0)).abs() < 1e-4, "{r}");
    }

    #[test]
    fn easter() {
        assert_eq!(easter_sunday(2024), NaiveDate::from_ymd_opt(2024, 3, 31).unwrap());
        assert!(is_holiday(NaiveDate::from_ymd_opt(2024, 3, 29).unwrap()));
    }
}
