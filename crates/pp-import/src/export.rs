//! Export as Portfolio Performance XML (XStream format with id references,
//! as written by PP itself with "ID_REFERENCES").
//!
//! XStream writes every object at its first occurrence and references it
//! afterwards. Because accounts are written before portfolios, a portfolio
//! is defined inline at the first cross entry pointing to it — exactly like
//! PP's own files.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use chrono::NaiveDateTime;
use pp_core::model::*;

/// Version written into the file (PP 0.70 data model).
pub const EXPORT_VERSION: i32 = 69;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn dt(d: &NaiveDateTime) -> String {
    if d.format("%S").to_string() == "00" {
        d.format("%Y-%m-%dT%H:%M").to_string()
    } else {
        d.format("%Y-%m-%dT%H:%M:%S").to_string()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key<'a> {
    Security(&'a str),
    Account(&'a str),
    Portfolio(&'a str),
    Tx(&'a str),
    Cross(&'a str, &'a str),
}

struct Writer<'a> {
    c: &'a Client,
    out: String,
    next_id: u32,
    ids: HashMap<Key<'a>, u32>,
    done: HashSet<Key<'a>>,
    depth: usize,
    acc_tx: HashMap<&'a str, (&'a Account, &'a AccountTransaction)>,
    pf_tx: HashMap<&'a str, (&'a Portfolio, &'a PortfolioTransaction)>,
}

impl<'a> Writer<'a> {
    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    fn line(&mut self, s: &str) {
        self.indent();
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn el(&mut self, tag: &str, value: &str) {
        let v = esc(value);
        self.line(&format!("<{tag}>{v}</{tag}>"));
    }

    fn opt(&mut self, tag: &str, value: &Option<String>) {
        if let Some(v) = value {
            self.el(tag, v);
        }
    }

    /// Opens an element for an object; returns false if it was written as a
    /// reference (nothing else to do).
    fn open(&mut self, tag: &str, key: Key<'a>, class: Option<&str>) -> bool {
        let cls = class.map(|c| format!(" class=\"{c}\"")).unwrap_or_default();
        if let Some(id) = self.ids.get(&key) {
            let id = *id;
            self.line(&format!("<{tag}{cls} reference=\"{id}\"/>"));
            return false;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.ids.insert(key, id);
        self.line(&format!("<{tag}{cls} id=\"{id}\">"));
        self.depth += 1;
        true
    }

    fn close(&mut self, tag: &str, key: Key<'a>) {
        self.depth -= 1;
        self.line(&format!("</{tag}>"));
        self.done.insert(key);
    }

    fn attributes(&mut self, attrs: &serde_json::Value) {
        self.line("<attributes>");
        self.depth += 1;
        let entries: Vec<(String, String)> = attrs
            .as_object()
            .map(|o| {
                o.iter()
                    .filter(|(k, _)| !k.starts_with('_'))
                    .filter_map(|(k, v)| match v {
                        serde_json::Value::String(s) => Some((k.clone(), s.clone())),
                        serde_json::Value::Number(n) => Some((k.clone(), n.to_string())),
                        serde_json::Value::Bool(b) => Some((k.clone(), b.to_string())),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if entries.is_empty() {
            self.line("<map/>");
        } else {
            self.line("<map>");
            self.depth += 1;
            for (k, v) in entries {
                self.line("<entry>");
                self.depth += 1;
                self.el("string", &k);
                self.el("string", &v);
                self.depth -= 1;
                self.line("</entry>");
            }
            self.depth -= 1;
            self.line("</map>");
        }
        self.depth -= 1;
        self.line("</attributes>");
    }

    fn security(&mut self, tag: &str, s: &'a Security) {
        if !self.open(tag, Key::Security(&s.id), None) {
            return;
        }
        self.el("uuid", &s.id);
        self.opt("onlineId", &s.online_id);
        self.el("name", &s.name);
        self.opt("currencyCode", &s.currency);
        self.opt("targetCurrencyCode", &s.target_currency);
        self.opt("note", &s.note);
        self.opt("isin", &s.isin);
        self.opt("tickerSymbol", &s.ticker);
        self.opt("wkn", &s.wkn);
        self.opt("calendar", &s.calendar);
        self.opt("feed", &s.feed);
        self.opt("feedURL", &s.feed_url);
        if s.prices.is_empty() {
            self.line("<prices/>");
        } else {
            self.line("<prices>");
            self.depth += 1;
            for p in &s.prices {
                self.line(&format!("<price t=\"{}\" v=\"{}\"/>", p.date, p.value));
            }
            self.depth -= 1;
            self.line("</prices>");
        }
        self.opt("latestFeed", &s.latest_feed);
        self.opt("latestFeedURL", &s.latest_feed_url);
        if let Some(l) = &s.latest {
            self.line(&format!("<latest t=\"{}\" v=\"{}\">", l.date, l.value));
            self.depth += 1;
            self.el("high", &l.high.to_string());
            self.el("low", &l.low.to_string());
            self.el("volume", &l.volume.to_string());
            self.depth -= 1;
            self.line("</latest>");
        }
        self.attributes(&s.attributes);
        if s.events.is_empty() {
            self.line("<events/>");
        } else {
            self.line("<events>");
            self.depth += 1;
            for e in &s.events {
                self.line("<event>");
                self.depth += 1;
                self.el("date", &e.date.to_string());
                let t = match e.kind {
                    SecurityEventKind::StockSplit => "STOCK_SPLIT",
                    SecurityEventKind::Note => "NOTE",
                    SecurityEventKind::DividendPayment => "DIVIDEND_PAYMENT",
                };
                self.el("type", t);
                self.opt("details", &e.details);
                self.depth -= 1;
                self.line("</event>");
            }
            self.depth -= 1;
            self.line("</events>");
        }
        self.el("isRetired", &s.retired.to_string());
        self.opt("updatedAt", &s.updated_at);
        self.close(tag, Key::Security(&s.id));
    }

    fn units(&mut self, units: &[TransactionUnit]) {
        if units.is_empty() {
            return;
        }
        self.line("<units>");
        self.depth += 1;
        for u in units {
            self.line(&format!("<unit type=\"{}\">", u.kind.as_str()));
            self.depth += 1;
            self.line(&format!(
                "<amount currency=\"{}\" amount=\"{}\"/>",
                esc(&u.amount.currency),
                u.amount.amount
            ));
            if let Some(f) = &u.forex {
                self.line(&format!(
                    "<forex currency=\"{}\" amount=\"{}\"/>",
                    esc(&f.currency),
                    f.amount
                ));
            }
            if let Some(r) = &u.exchange_rate {
                self.el("exchangeRate", &r.to_string());
            }
            self.depth -= 1;
            self.line("</unit>");
        }
        self.depth -= 1;
        self.line("</units>");
    }

    fn cross_key(t: &'a TransactionCommon) -> Option<Key<'a>> {
        let ce = t.cross_entry.as_ref()?;
        let (a, b) = if t.id.as_str() < ce.other_transaction.as_str() {
            (t.id.as_str(), ce.other_transaction.as_str())
        } else {
            (ce.other_transaction.as_str(), t.id.as_str())
        };
        Some(Key::Cross(a, b))
    }

    /// Writes the cross entry of transaction `t` (owned by `owner`).
    fn cross_entry(&mut self, t: &'a TransactionCommon, is_account: bool, kind: &str) {
        let Some(ce) = &t.cross_entry else { return };
        let Some(key) = Self::cross_key(t) else {
            return;
        };
        let class = match ce.kind {
            CrossEntryKind::BuySell => "buysell",
            CrossEntryKind::AccountTransfer => "account-transfer",
            CrossEntryKind::PortfolioTransfer => "portfolio-transfer",
        };
        if !self.open("crossEntry", key, Some(class)) {
            return;
        }
        let (this_id, other_id) = (t.id.as_str(), ce.other_transaction.as_str());
        match ce.kind {
            CrossEntryKind::BuySell => {
                let (ptx, atx) = if is_account {
                    (other_id, this_id)
                } else {
                    (this_id, other_id)
                };
                if let Some((p, _)) = self.pf_tx.get(ptx).copied() {
                    self.portfolio("portfolio", p);
                }
                self.tx_ref("portfolioTransaction", ptx);
                if let Some((a, _)) = self.acc_tx.get(atx).copied() {
                    self.account("account", a);
                }
                self.tx_ref("accountTransaction", atx);
            }
            CrossEntryKind::AccountTransfer => {
                let (from, to) = if kind == "TRANSFER_OUT" {
                    (this_id, other_id)
                } else {
                    (other_id, this_id)
                };
                if let Some((a, _)) = self.acc_tx.get(from).copied() {
                    self.account("accountFrom", a);
                }
                self.tx_ref("transactionFrom", from);
                if let Some((a, _)) = self.acc_tx.get(to).copied() {
                    self.account("accountTo", a);
                }
                self.tx_ref("transactionTo", to);
            }
            CrossEntryKind::PortfolioTransfer => {
                let (from, to) = if kind == "TRANSFER_OUT" {
                    (this_id, other_id)
                } else {
                    (other_id, this_id)
                };
                if let Some((p, _)) = self.pf_tx.get(from).copied() {
                    self.portfolio("portfolioFrom", p);
                }
                self.tx_ref("transactionFrom", from);
                if let Some((p, _)) = self.pf_tx.get(to).copied() {
                    self.portfolio("portfolioTo", p);
                }
                self.tx_ref("transactionTo", to);
            }
        }
        self.close("crossEntry", key);
    }

    /// A transaction inside a cross entry: reference or inline definition.
    fn tx_ref(&mut self, tag: &str, id: &'a str) {
        if let Some((_, t)) = self.acc_tx.get(id).copied() {
            self.account_tx(tag, t);
        } else if let Some((_, t)) = self.pf_tx.get(id).copied() {
            self.portfolio_tx(tag, t);
        }
    }

    fn common(&mut self, t: &'a TransactionCommon, is_account: bool, kind: &str) {
        self.el("uuid", &t.id);
        self.el("date", &dt(&t.date_time));
        self.el("currencyCode", &t.currency);
        self.el("amount", &t.amount.to_string());
        if let Some(sid) = &t.security
            && let Some(s) = self.c.security(sid)
        {
            self.security("security", s);
        }
        self.cross_entry(t, is_account, kind);
        self.el("shares", &t.shares.to_string());
        self.opt("note", &t.note);
        self.opt("source", &t.source);
        self.units(&t.units);
        self.opt("updatedAt", &t.updated_at);
    }

    fn account_tx(&mut self, tag: &str, t: &'a AccountTransaction) {
        if !self.open(tag, Key::Tx(&t.id), None) {
            return;
        }
        self.common(&t.common, true, t.kind.as_str());
        self.el("type", t.kind.as_str());
        if let Some(ex) = &t.ex_date {
            self.el("exDate", &dt(ex));
        }
        self.close(tag, Key::Tx(&t.id));
    }

    fn portfolio_tx(&mut self, tag: &str, t: &'a PortfolioTransaction) {
        if !self.open(tag, Key::Tx(&t.id), None) {
            return;
        }
        self.common(&t.common, false, t.kind.as_str());
        self.el("type", t.kind.as_str());
        self.close(tag, Key::Tx(&t.id));
    }

    fn account(&mut self, tag: &str, a: &'a Account) {
        if !self.open(tag, Key::Account(&a.id), None) {
            return;
        }
        self.el("uuid", &a.id);
        self.el("name", &a.name);
        self.el("currencyCode", &a.currency);
        self.opt("note", &a.note);
        self.el("isRetired", &a.retired.to_string());
        if a.transactions.is_empty() {
            self.line("<transactions/>");
        } else {
            self.line("<transactions>");
            self.depth += 1;
            for t in &a.transactions {
                self.account_tx("account-transaction", t);
            }
            self.depth -= 1;
            self.line("</transactions>");
        }
        self.attributes(&a.attributes);
        self.opt("updatedAt", &a.updated_at);
        self.close(tag, Key::Account(&a.id));
    }

    fn portfolio(&mut self, tag: &str, p: &'a Portfolio) {
        if !self.open(tag, Key::Portfolio(&p.id), None) {
            return;
        }
        self.el("uuid", &p.id);
        self.el("name", &p.name);
        self.opt("note", &p.note);
        self.el("isRetired", &p.retired.to_string());
        if let Some(r) = &p.reference_account
            && let Some(a) = self.c.account(r)
        {
            self.account("referenceAccount", a);
        }
        if p.transactions.is_empty() {
            self.line("<transactions/>");
        } else {
            self.line("<transactions>");
            self.depth += 1;
            for t in &p.transactions {
                self.portfolio_tx("portfolio-transaction", t);
            }
            self.depth -= 1;
            self.line("</transactions>");
        }
        self.attributes(&p.attributes);
        self.opt("updatedAt", &p.updated_at);
        self.close(tag, Key::Portfolio(&p.id));
    }

    fn classification(
        &mut self,
        c: &'a Classification,
        tag: &str,
        children: &HashMap<Option<&'a str>, Vec<&'a Classification>>,
    ) {
        self.line(&format!("<{tag}>"));
        self.depth += 1;
        self.el("id", &c.id);
        self.el("name", &c.name);
        self.opt("note", &c.note);
        self.opt("color", &c.color);
        let kids = children
            .get(&Some(c.id.as_str()))
            .cloned()
            .unwrap_or_default();
        if kids.is_empty() {
            self.line("<children/>");
        } else {
            self.line("<children>");
            self.depth += 1;
            for k in kids {
                self.classification(k, "classification", children);
            }
            self.depth -= 1;
            self.line("</children>");
        }
        if c.assignments.is_empty() {
            self.line("<assignments/>");
        } else {
            self.line("<assignments>");
            self.depth += 1;
            for a in &c.assignments {
                let (class, key) = if self.c.security(&a.vehicle).is_some() {
                    ("security", Key::Security(a.vehicle.as_str()))
                } else {
                    ("account", Key::Account(a.vehicle.as_str()))
                };
                let Some(id) = self.ids.get(&key).copied() else {
                    continue;
                };
                self.line("<assignment>");
                self.depth += 1;
                self.line(&format!(
                    "<investmentVehicle class=\"{class}\" reference=\"{id}\"/>"
                ));
                self.el("weight", &a.weight.to_string());
                self.el("rank", &a.rank.to_string());
                self.depth -= 1;
                self.line("</assignment>");
            }
            self.depth -= 1;
            self.line("</assignments>");
        }
        self.el("weight", &c.weight.to_string());
        self.el("rank", &c.rank.to_string());
        self.depth -= 1;
        self.line(&format!("</{tag}>"));
    }
}

/// Serialises a client as PP XML.
pub fn to_pp_xml(c: &Client) -> String {
    let mut w = Writer {
        c,
        out: String::with_capacity(1 << 20),
        next_id: 1,
        ids: HashMap::new(),
        done: HashSet::new(),
        depth: 1,
        acc_tx: HashMap::new(),
        pf_tx: HashMap::new(),
    };
    for a in &c.accounts {
        for t in &a.transactions {
            w.acc_tx.insert(&t.id, (a, t));
        }
    }
    for p in &c.portfolios {
        for t in &p.transactions {
            w.pf_tx.insert(&t.id, (p, t));
        }
    }
    w.out.push_str("<client id=\"1\">\n");
    w.el("version", &EXPORT_VERSION.to_string());
    w.el("baseCurrency", &c.base_currency);
    w.line("<securities>");
    w.depth += 1;
    for s in &c.securities {
        w.security("security", s);
    }
    w.depth -= 1;
    w.line("</securities>");

    w.line("<watchlists>");
    w.depth += 1;
    for wl in &c.watchlists {
        w.line("<watchlist>");
        w.depth += 1;
        w.el("name", &wl.name);
        w.line("<securities>");
        w.depth += 1;
        for sid in &wl.securities {
            if let Some(id) = w.ids.get(&Key::Security(sid.as_str())).copied() {
                w.line(&format!("<security reference=\"{id}\"/>"));
            }
        }
        w.depth -= 1;
        w.line("</securities>");
        w.depth -= 1;
        w.line("</watchlist>");
    }
    w.depth -= 1;
    w.line("</watchlists>");

    w.line("<accounts>");
    w.depth += 1;
    for a in &c.accounts {
        w.account("account", a);
    }
    w.depth -= 1;
    w.line("</accounts>");
    w.line("<portfolios>");
    w.depth += 1;
    for p in &c.portfolios {
        w.portfolio("portfolio", p);
    }
    w.depth -= 1;
    w.line("</portfolios>");
    if c.plans.is_empty() {
        w.line("<plans/>");
    } else {
        w.line("<plans>");
        w.depth += 1;
        for p in &c.plans {
            w.line("<investment-plan>");
            w.depth += 1;
            w.el("name", &p.name);
            w.opt("note", &p.note);
            for (tag, key) in [
                ("security", p.security.as_deref().map(Key::Security)),
                ("portfolio", p.portfolio.as_deref().map(Key::Portfolio)),
                ("account", p.account.as_deref().map(Key::Account)),
            ] {
                if let Some(id) = key.and_then(|k| w.ids.get(&k).copied()) {
                    w.line(&format!("<{tag} reference=\"{id}\"/>"));
                }
            }
            w.line("<attributes>");
            w.line("  <map/>");
            w.line("</attributes>");
            w.el("autoGenerate", &p.auto_generate.to_string());
            w.el("start", &dt(&p.start));
            w.el("interval", &p.interval.to_string());
            w.el("amount", &p.amount.to_string());
            w.el("fees", &p.fees.to_string());
            w.line("<transactions>");
            w.depth += 1;
            for t in &p.transactions {
                let Some(id) = w.ids.get(&Key::Tx(t.as_str())).copied() else {
                    continue;
                };
                let tag = if w.acc_tx.contains_key(t.as_str()) {
                    "account-transaction"
                } else {
                    "portfolio-transaction"
                };
                w.line(&format!("<{tag} reference=\"{id}\"/>"));
            }
            w.depth -= 1;
            w.line("</transactions>");
            w.el("taxes", &p.taxes.to_string());
            w.el("type", p.kind.as_str());
            w.depth -= 1;
            w.line("</investment-plan>");
        }
        w.depth -= 1;
        w.line("</plans>");
    }

    w.line("<taxonomies>");
    w.depth += 1;
    for t in &c.taxonomies {
        let mut children: HashMap<Option<&str>, Vec<&Classification>> = HashMap::new();
        for cl in &t.classifications {
            children
                .entry(cl.parent_id.as_deref())
                .or_default()
                .push(cl);
        }
        for v in children.values_mut() {
            v.sort_by_key(|c| c.rank);
        }
        w.line("<taxonomy>");
        w.depth += 1;
        w.el("id", &t.id);
        w.el("name", &t.name);
        w.opt("source", &t.source);
        if !t.dimensions.is_empty() {
            w.line("<dimensions>");
            w.depth += 1;
            for d in &t.dimensions {
                w.el("string", d);
            }
            w.depth -= 1;
            w.line("</dimensions>");
        }
        if let Some(root) = children.get(&None).and_then(|r| r.first().copied()) {
            w.classification(root, "root", &children);
        }
        w.depth -= 1;
        w.line("</taxonomy>");
    }
    w.depth -= 1;
    w.line("</taxonomies>");
    w.line("<dashboards/>");
    w.line("<properties/>");
    w.line("<settings>");
    w.depth += 1;
    w.line("<bookmarks/>");
    w.line("<attributeTypes/>");
    w.line("<configurationSets/>");
    w.depth -= 1;
    w.line("</settings>");
    let _ = writeln!(w.out, "</client>");
    w.out
}
