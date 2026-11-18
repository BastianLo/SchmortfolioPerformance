//! Reader for the XStream based XML format of Portfolio Performance.
//!
//! XStream serializes every object at its first occurrence and refers to it
//! afterwards, either via relative XPath (`reference="../../securities/security[2]"`)
//! or via ids (`id="12"` / `reference="12"`). We parse the document into a
//! read-only tree and resolve every element to its canonical (defining) node.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};
use pp_core::Money;
use pp_core::model::*;
use roxmltree::{Document, Node, NodeId};
use rust_decimal::Decimal;

use crate::ImportError;

pub(crate) struct XmlReader<'a, 'input> {
    doc: &'a Document<'input>,
    ids: HashMap<&'a str, NodeId>,
    id_mode: bool,
    generated_ids: RefCell<HashMap<NodeId, String>>,
    pub warnings: RefCell<Vec<String>>,
}

fn children<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(|c| c.is_element())
}

impl<'a, 'input> XmlReader<'a, 'input> {
    pub fn new(doc: &'a Document<'input>) -> Self {
        let root = doc.root_element();
        let id_mode = root.has_attribute("id");
        let mut ids = HashMap::new();
        if id_mode {
            for n in doc.descendants().filter(|n| n.is_element()) {
                if let Some(id) = n.attribute("id") {
                    ids.insert(id, n.id());
                }
            }
        }
        Self {
            doc,
            ids,
            id_mode,
            generated_ids: RefCell::new(HashMap::new()),
            warnings: RefCell::new(vec![]),
        }
    }

    fn warn(&self, msg: impl Into<String>) {
        self.warnings.borrow_mut().push(msg.into());
    }

    /// Follows `reference` attributes to the defining element.
    fn resolve(&self, node: Node<'a, 'input>) -> Node<'a, 'input> {
        let mut current = node;
        for _ in 0..64 {
            let Some(reference) = current.attribute("reference") else {
                return current;
            };
            let next = if self.id_mode {
                self.ids
                    .get(reference)
                    .and_then(|id| self.doc.get_node(*id))
            } else {
                self.resolve_path(current, reference)
            };
            match next {
                Some(n) => current = n,
                None => {
                    self.warn(format!("Unaufgelöste Referenz: {reference}"));
                    return current;
                }
            }
        }
        current
    }

    fn resolve_path(&self, start: Node<'a, 'input>, path: &str) -> Option<Node<'a, 'input>> {
        let mut node = start;
        for seg in path.split('/') {
            if seg.is_empty() || seg == "." {
                continue;
            }
            if seg == ".." {
                node = node.parent_element()?;
                continue;
            }
            let (name, index) = match seg.find('[') {
                Some(p) => (&seg[..p], seg[p + 1..seg.len() - 1].parse::<usize>().ok()?),
                None => (seg, 1),
            };
            node = node
                .children()
                .filter(|c| c.is_element() && c.tag_name().name() == name)
                .nth(index - 1)?;
        }
        Some(node)
    }

    fn child(&self, node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
        node.children()
            .find(|c| c.is_element() && c.tag_name().name() == name)
            .map(|n| self.resolve(n))
    }

    fn text(&self, node: Node<'a, 'input>, name: &str) -> Option<String> {
        self.child(node, name)
            .and_then(|n| n.text())
            .map(|s| s.to_string())
    }

    fn text_i64(&self, node: Node<'a, 'input>, name: &str) -> i64 {
        self.text(node, name)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    fn text_bool(&self, node: Node<'a, 'input>, name: &str) -> bool {
        self.text(node, name)
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
    }

    /// uuid of an entity node; generates (stable per node) ids for old files.
    fn uuid(&self, node: Node<'a, 'input>) -> String {
        let node = self.resolve(node);
        if let Some(u) = self.text(node, "uuid") {
            return u;
        }
        self.generated_ids
            .borrow_mut()
            .entry(node.id())
            .or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone()
    }

    pub fn read(&self) -> Result<Client, ImportError> {
        let root = self.doc.root_element();
        if root.tag_name().name() != "client" {
            return Err(ImportError::Format(
                "XML-Wurzelelement ist nicht <client>".into(),
            ));
        }
        let version = self.text_i64(root, "version") as i32;
        let base_currency = self
            .text(root, "baseCurrency")
            .unwrap_or_else(|| "EUR".into());

        let securities = self
            .child(root, "securities")
            .map(|s| {
                elements(s)
                    .map(|n| self.security(self.resolve(n)))
                    .collect()
            })
            .unwrap_or_default();
        let accounts = self
            .child(root, "accounts")
            .map(|s| elements(s).map(|n| self.account(self.resolve(n))).collect())
            .unwrap_or_default();
        let portfolios = self
            .child(root, "portfolios")
            .map(|s| {
                elements(s)
                    .map(|n| self.portfolio(self.resolve(n)))
                    .collect()
            })
            .unwrap_or_default();
        let taxonomies = self
            .child(root, "taxonomies")
            .map(|s| {
                elements(s)
                    .map(|n| self.taxonomy(self.resolve(n)))
                    .collect()
            })
            .unwrap_or_default();
        let watchlists = self
            .child(root, "watchlists")
            .map(|s| {
                elements(s)
                    .map(|n| {
                        let n = self.resolve(n);
                        Watchlist {
                            name: self.text(n, "name").unwrap_or_default(),
                            securities: self
                                .child(n, "securities")
                                .map(|s| elements(s).map(|x| self.uuid(x)).collect())
                                .unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let properties = self
            .child(root, "properties")
            .map(|p| self.map_entries(p))
            .unwrap_or_default();
        let plans = self
            .child(root, "plans")
            .map(|p| elements(p).count())
            .unwrap_or(0);

        Ok(Client {
            version,
            base_currency,
            securities,
            accounts,
            portfolios,
            taxonomies,
            watchlists,
            extra: serde_json::json!({ "properties": properties, "investment_plans": plans }),
        })
    }

    fn map_entries(&self, node: Node<'a, 'input>) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        let container = self.child(node, "map").unwrap_or(node);
        for e in children(container, "entry") {
            let mut it = elements(e);
            let (Some(k), Some(v)) = (it.next(), it.next()) else {
                continue;
            };
            let v = self.resolve(v);
            let key = k.text().unwrap_or_default().to_string();
            let value = match v.tag_name().name() {
                "boolean" => serde_json::Value::Bool(v.text() == Some("true")),
                "long" | "int" => v
                    .text()
                    .and_then(|t| t.parse::<i64>().ok())
                    .map(Into::into)
                    .unwrap_or_default(),
                "double" => v
                    .text()
                    .and_then(|t| t.parse::<f64>().ok())
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or_default(),
                _ => v
                    .text()
                    .map(|t| serde_json::Value::String(t.to_string()))
                    .unwrap_or_default(),
            };
            map.insert(key, value);
        }
        serde_json::Value::Object(map)
    }

    fn attributes(&self, node: Node<'a, 'input>) -> serde_json::Value {
        self.child(node, "attributes")
            .map(|a| self.map_entries(a))
            .unwrap_or(serde_json::json!({}))
    }

    fn security(&self, n: Node<'a, 'input>) -> Security {
        let prices =
            self.child(n, "prices")
                .map(|p| {
                    children(p, "price")
                        .filter_map(|pr| {
                            let pr = self.resolve(pr);
                            let date = pr.attribute("t").and_then(parse_date).or_else(|| {
                                self.text(pr, "time").as_deref().and_then(parse_date)
                            })?;
                            let value = pr
                                .attribute("v")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or_else(|| self.text_i64(pr, "value"));
                            Some(SecurityPrice { date, value })
                        })
                        .collect()
                })
                .unwrap_or_default();
        let latest = self.child(n, "latest").and_then(|l| {
            let date = l
                .attribute("t")
                .and_then(parse_date)
                .or_else(|| self.text(l, "time").as_deref().and_then(parse_date))?;
            let value = l
                .attribute("v")
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| self.text_i64(l, "value"));
            Some(LatestSecurityPrice {
                date,
                value,
                high: self.text_i64(l, "high"),
                low: self.text_i64(l, "low"),
                volume: self.text_i64(l, "volume"),
            })
        });
        let events = self
            .child(n, "events")
            .map(|e| {
                elements(e)
                    .filter_map(|ev| {
                        let ev = self.resolve(ev);
                        let date = self.text(ev, "date").as_deref().and_then(parse_date)?;
                        let kind = match self.text(ev, "type").as_deref() {
                            Some("STOCK_SPLIT") => SecurityEventKind::StockSplit,
                            Some("DIVIDEND_PAYMENT") => SecurityEventKind::DividendPayment,
                            _ => SecurityEventKind::Note,
                        };
                        Some(SecurityEvent {
                            kind,
                            date,
                            details: self.text(ev, "details"),
                            data: serde_json::Value::Null,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Security {
            id: self.uuid(n),
            name: self.text(n, "name").unwrap_or_default(),
            currency: self.text(n, "currencyCode"),
            target_currency: self.text(n, "targetCurrencyCode"),
            isin: self.text(n, "isin"),
            wkn: self.text(n, "wkn"),
            ticker: self.text(n, "tickerSymbol"),
            online_id: self.text(n, "onlineId"),
            calendar: self.text(n, "calendar"),
            feed: self.text(n, "feed"),
            feed_url: self.text(n, "feedURL"),
            latest_feed: self.text(n, "latestFeed"),
            latest_feed_url: self.text(n, "latestFeedURL"),
            note: self.text(n, "note"),
            retired: self.text_bool(n, "isRetired"),
            prices,
            latest,
            events,
            attributes: self.attributes(n),
            updated_at: self.text(n, "updatedAt"),
        }
    }

    fn account(&self, n: Node<'a, 'input>) -> Account {
        let id = self.uuid(n);
        let transactions = self
            .child(n, "transactions")
            .map(|t| {
                elements(t)
                    .filter_map(|tx| self.account_tx(self.resolve(tx), n))
                    .collect()
            })
            .unwrap_or_default();
        Account {
            id,
            name: self.text(n, "name").unwrap_or_default(),
            currency: self.text(n, "currencyCode").unwrap_or_else(|| "EUR".into()),
            note: self.text(n, "note"),
            retired: self.text_bool(n, "isRetired"),
            transactions,
            attributes: self.attributes(n),
            updated_at: self.text(n, "updatedAt"),
        }
    }

    fn portfolio(&self, n: Node<'a, 'input>) -> Portfolio {
        let transactions = self
            .child(n, "transactions")
            .map(|t| {
                elements(t)
                    .filter_map(|tx| self.portfolio_tx(self.resolve(tx), n))
                    .collect()
            })
            .unwrap_or_default();
        Portfolio {
            id: self.uuid(n),
            name: self.text(n, "name").unwrap_or_default(),
            note: self.text(n, "note"),
            retired: self.text_bool(n, "isRetired"),
            reference_account: self.child(n, "referenceAccount").map(|a| self.uuid(a)),
            transactions,
            attributes: self.attributes(n),
            updated_at: self.text(n, "updatedAt"),
        }
    }

    fn units(&self, n: Node<'a, 'input>, currency: &str) -> Vec<TransactionUnit> {
        let mut out: Vec<TransactionUnit> = self
            .child(n, "units")
            .map(|u| {
                children(u, "unit")
                    .filter_map(|un| {
                        let un = self.resolve(un);
                        let kind = UnitType::parse(un.attribute("type")?)?;
                        let money = |name: &str| {
                            self.child(un, name).map(|m| {
                                Money::new(
                                    m.attribute("currency").unwrap_or(currency),
                                    m.attribute("amount")
                                        .and_then(|a| a.parse().ok())
                                        .unwrap_or(0),
                                )
                            })
                        };
                        Some(TransactionUnit {
                            kind,
                            amount: money("amount")?,
                            forex: money("forex"),
                            exchange_rate: self
                                .text(un, "exchangeRate")
                                .and_then(|r| r.trim().parse::<Decimal>().ok()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        // very old files (version < 29): fees and taxes as plain fields
        for (field, kind) in [("fees", UnitType::Fee), ("taxes", UnitType::Tax)] {
            let v = self.text_i64(n, field);
            if v != 0 {
                out.push(TransactionUnit {
                    kind,
                    amount: Money::new(currency, v),
                    forex: None,
                    exchange_rate: None,
                });
            }
        }
        out
    }

    fn common(
        &self,
        n: Node<'a, 'input>,
        owner: Node<'a, 'input>,
        owner_currency: &str,
    ) -> Option<TransactionCommon> {
        let date_time = self.text(n, "date").as_deref().and_then(parse_date_time);
        let Some(date_time) = date_time else {
            self.warn("Buchung ohne Datum übersprungen");
            return None;
        };
        let currency = self
            .text(n, "currencyCode")
            .unwrap_or_else(|| owner_currency.to_string());
        let cross_entry = self.cross_entry(n, owner);
        Some(TransactionCommon {
            id: self.uuid(n),
            date_time,
            amount: self.text_i64(n, "amount"),
            shares: self.text_i64(n, "shares"),
            security: self.child(n, "security").map(|s| self.uuid(s)),
            note: self.text(n, "note"),
            source: self.text(n, "source"),
            units: self.units(n, &currency),
            currency,
            cross_entry,
            updated_at: self.text(n, "updatedAt"),
        })
    }

    fn cross_entry(&self, tx: Node<'a, 'input>, owner: Node<'a, 'input>) -> Option<CrossEntry> {
        let raw = tx
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "crossEntry")?;
        let ce = self.resolve(raw);
        let class = ce
            .attribute("class")
            .or(raw.attribute("class"))
            .unwrap_or("");
        let id = |name: &str| self.child(ce, name).map(|n| n.id());
        let pick = |a_owner: &str,
                    a_tx: &str,
                    b_owner: &str,
                    b_tx: &str|
         -> Option<(Node<'a, 'input>, Node<'a, 'input>)> {
            let this = tx.id();
            if id(a_tx) == Some(this) {
                Some((self.child(ce, b_owner)?, self.child(ce, b_tx)?))
            } else if id(b_tx) == Some(this) {
                Some((self.child(ce, a_owner)?, self.child(ce, a_tx)?))
            } else {
                None
            }
        };
        let (kind, other) = match class {
            "buysell" => (
                CrossEntryKind::BuySell,
                pick(
                    "portfolio",
                    "portfolioTransaction",
                    "account",
                    "accountTransaction",
                ),
            ),
            "account-transfer" => (
                CrossEntryKind::AccountTransfer,
                pick(
                    "accountFrom",
                    "transactionFrom",
                    "accountTo",
                    "transactionTo",
                ),
            ),
            "portfolio-transfer" => (
                CrossEntryKind::PortfolioTransfer,
                pick(
                    "portfolioFrom",
                    "transactionFrom",
                    "portfolioTo",
                    "transactionTo",
                ),
            ),
            _ => return None,
        };
        let _ = owner;
        let (other_owner, other_tx) = other?;
        Some(CrossEntry {
            kind,
            other_owner: self.uuid(other_owner),
            other_transaction: self.uuid(other_tx),
        })
    }

    fn account_tx(
        &self,
        n: Node<'a, 'input>,
        owner: Node<'a, 'input>,
    ) -> Option<AccountTransaction> {
        let type_str = self.text(n, "type")?;
        let Some(mut kind) = AccountTransactionType::parse(&type_str) else {
            self.warn(format!("Unbekannter Buchungstyp {type_str}"));
            return None;
        };
        let owner_currency = self
            .text(owner, "currencyCode")
            .unwrap_or_else(|| "EUR".into());
        let common = self.common(n, owner, &owner_currency)?;
        if kind == AccountTransactionType::Dividends && common.security.is_none() {
            kind = AccountTransactionType::Interest;
        }
        Some(AccountTransaction {
            kind,
            ex_date: self.text(n, "exDate").as_deref().and_then(parse_date_time),
            common,
        })
    }

    fn portfolio_tx(
        &self,
        n: Node<'a, 'input>,
        owner: Node<'a, 'input>,
    ) -> Option<PortfolioTransaction> {
        let type_str = self.text(n, "type")?;
        let Some(kind) = PortfolioTransactionType::parse(&type_str) else {
            self.warn(format!("Unbekannter Depotbuchungstyp {type_str}"));
            return None;
        };
        let common = self.common(n, owner, "EUR")?;
        if common.security.is_none() {
            self.warn("Depotbuchung ohne Wertpapier übersprungen");
            return None;
        }
        Some(PortfolioTransaction { kind, common })
    }

    fn taxonomy(&self, n: Node<'a, 'input>) -> Taxonomy {
        let mut classifications = Vec::new();
        if let Some(root) = self.child(n, "root") {
            self.classification(root, None, &mut classifications);
        }
        Taxonomy {
            id: self
                .text(n, "id")
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: self.text(n, "name").unwrap_or_default(),
            source: self.text(n, "source"),
            dimensions: self
                .child(n, "dimensions")
                .map(|d| {
                    elements(d)
                        .filter_map(|s| s.text().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            classifications,
        }
    }

    fn classification(
        &self,
        n: Node<'a, 'input>,
        parent: Option<String>,
        out: &mut Vec<Classification>,
    ) {
        let id = self
            .text(n, "id")
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let assignments = self
            .child(n, "assignments")
            .map(|a| {
                children(a, "assignment")
                    .filter_map(|asg| {
                        let asg = self.resolve(asg);
                        let vehicle = self.child(asg, "investmentVehicle")?;
                        Some(Assignment {
                            vehicle: self.uuid(vehicle),
                            weight: self.text_i64(asg, "weight") as i32,
                            rank: self.text_i64(asg, "rank") as i32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(Classification {
            id: id.clone(),
            parent_id: parent,
            name: self.text(n, "name").unwrap_or_default(),
            note: self.text(n, "note"),
            color: self.text(n, "color"),
            weight: self
                .text(n, "weight")
                .and_then(|w| w.parse().ok())
                .unwrap_or(10000),
            rank: self.text_i64(n, "rank") as i32,
            assignments,
            data: serde_json::Value::Null,
        });
        if let Some(ch) = self.child(n, "children") {
            for c in elements(ch) {
                let c = self.resolve(c);
                self.classification(c, Some(id.clone()), out);
            }
        }
    }
}

pub(crate) fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    NaiveDate::parse_from_str(&s[..s.len().min(10)], "%Y-%m-%d").ok()
}

pub(crate) fn parse_date_time(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    parse_date(s).and_then(|d| d.and_hms_opt(0, 0, 0))
}

pub(crate) fn read_xml(text: &str) -> Result<(Client, Vec<String>), ImportError> {
    let opts = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: u32::MAX,
        ..Default::default()
    };
    let doc = Document::parse_with_options(text, opts)
        .map_err(|e| ImportError::Format(format!("XML ungültig: {e}")))?;
    let reader = XmlReader::new(&doc);
    let client = reader.read()?;
    let warnings = reader.warnings.into_inner();
    Ok((client, warnings))
}
