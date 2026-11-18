use std::path::PathBuf;

use pp_core::model::Client;
use pp_import::{FileFormat, ImportError, read_client};

fn testdata(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pp")
        .join(rel)
}

fn load(rel: &str, pwd: Option<&str>) -> pp_import::ImportResult {
    let bytes = std::fs::read(testdata(rel)).unwrap();
    read_client(&bytes, pwd).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// A summary that must be identical for the same client stored in different
/// formats.
fn fingerprint(c: &Client) -> String {
    let mut out = String::new();
    out += &format!(
        "base={} sec={} acc={} pf={} tax={}\n",
        c.base_currency,
        c.securities.len(),
        c.accounts.len(),
        c.portfolios.len(),
        c.taxonomies.len()
    );
    let mut secs: Vec<_> = c
        .securities
        .iter()
        .map(|s| {
            format!(
                "{} {} prices={} latest={:?}",
                s.id,
                s.name,
                s.prices.len(),
                s.latest.as_ref().map(|l| l.value)
            )
        })
        .collect();
    secs.sort();
    out += &secs.join("\n");
    for a in &c.accounts {
        let mut txs: Vec<_> = a
            .transactions
            .iter()
            .map(|t| {
                format!(
                    "{} {:?} {} {} {} sh={} sec={:?} units={}",
                    t.id,
                    t.kind,
                    t.date_time,
                    t.currency,
                    t.amount,
                    t.shares,
                    t.security,
                    t.units.len()
                )
            })
            .collect();
        txs.sort();
        out += &format!(
            "\nACC {} {} {}\n{}",
            a.id,
            a.name,
            a.currency,
            txs.join("\n")
        );
    }
    for p in &c.portfolios {
        let mut txs: Vec<_> = p
            .transactions
            .iter()
            .map(|t| {
                format!(
                    "{} {:?} {} {} {} sh={} sec={:?} units={} cross={:?}",
                    t.id,
                    t.kind,
                    t.date_time,
                    t.currency,
                    t.amount,
                    t.shares,
                    t.security,
                    t.units.len(),
                    t.cross_entry.as_ref().map(|c| &c.other_transaction)
                )
            })
            .collect();
        txs.sort();
        out += &format!(
            "\nPF {} {} ref={:?}\n{}",
            p.id,
            p.name,
            p.reference_account,
            txs.join("\n")
        );
    }
    out
}

#[test]
fn reads_all_historic_file_versions_identically() {
    for v in ["client52", "client53", "client69"] {
        let xml = load(&format!("fileversions/{v}.xml"), None);
        assert_eq!(xml.info.format, FileFormat::Xml);
        let bin = load(&format!("fileversions/{v}.binary.portfolio"), None);
        assert_eq!(bin.info.format, FileFormat::Binary);
        let bin_pwd = load(
            &format!("fileversions/{v}.binary+pwd.portfolio"),
            Some("123456"),
        );
        assert!(bin_pwd.info.encrypted);

        let f = fingerprint(&xml.client);
        assert!(
            !xml.client.accounts.is_empty() || !xml.client.securities.is_empty(),
            "{v} empty"
        );
        assert_eq!(f, fingerprint(&bin.client), "{v}: xml vs binary");
        assert_eq!(
            f,
            fingerprint(&bin_pwd.client),
            "{v}: xml vs encrypted binary"
        );

        let xml_pwd = testdata(&format!("fileversions/{v}.xml+pwd.portfolio"));
        if xml_pwd.exists() {
            let r = load(
                &format!("fileversions/{v}.xml+pwd.portfolio"),
                Some("123456"),
            );
            assert_eq!(f, fingerprint(&r.client), "{v}: xml vs encrypted xml");
        }
    }
}

#[test]
fn encrypted_requires_correct_password() {
    let bytes = std::fs::read(testdata("fileversions/client69.binary+pwd.portfolio")).unwrap();
    assert!(matches!(
        read_client(&bytes, None),
        Err(ImportError::PasswordRequired)
    ));
    assert!(matches!(
        read_client(&bytes, Some("wrong")),
        Err(ImportError::WrongPassword)
    ));
}

#[test]
fn id_and_relative_references_resolve_to_same_model() {
    let a = load("fileversions/client_with_id_references.xml", None);
    assert_eq!(a.info.format, FileFormat::XmlIdReferences);
    let b = load("fileversions/client_with_relative_references.xml", None);
    assert_eq!(a.client.securities.len(), 2);
    // the 'latest' price is shared via reference in both files
    assert!(a.client.securities.iter().all(|s| s.latest.is_some()));
    assert_eq!(fingerprint(&a.client), fingerprint(&b.client));
}

#[test]
fn reads_all_scenario_and_issue_files() {
    for dir in ["scenarios", "issues"] {
        for entry in std::fs::read_dir(testdata(dir)).unwrap() {
            let path = entry.unwrap().path();
            let rel = format!("{dir}/{}", path.file_name().unwrap().to_string_lossy());
            let r = load(&rel, None);
            assert!(
                r.info.warnings.iter().all(|w| !w.contains("Unaufgelöste")),
                "{rel}: {:?}",
                r.info.warnings
            );
            // cross entries must point to existing transactions
            for p in &r.client.portfolios {
                for t in &p.transactions {
                    if let Some(ce) = &t.cross_entry {
                        let found = r.client.accounts.iter().any(|a| {
                            a.id == ce.other_owner
                                && a.transactions.iter().any(|x| x.id == ce.other_transaction)
                        }) || r.client.portfolios.iter().any(|a| {
                            a.id == ce.other_owner
                                && a.transactions.iter().any(|x| x.id == ce.other_transaction)
                        });
                        assert!(found, "{rel}: dangling cross entry {}", t.id);
                    }
                }
            }
        }
    }
}

#[test]
fn reads_demo_file_with_inline_definitions() {
    let r = load("kommer.xml", None);
    let c = &r.client;
    assert_eq!(c.securities.len(), 15, "{:?}", r.info.warnings);
    assert!(c.accounts.len() >= 4);
    assert_eq!(c.portfolios.len(), 2);
    assert_eq!(c.taxonomies.len(), 5);
    let buys: usize = c.portfolios.iter().map(|p| p.transactions.len()).sum();
    assert!(buys > 0);
    // every security referenced by a transaction exists
    for p in &c.portfolios {
        for t in &p.transactions {
            assert!(c.security(t.security.as_deref().unwrap()).is_some());
        }
    }
}

#[test]
fn export_roundtrip_is_lossless() {
    for f in [
        "kommer.xml",
        "fileversions/client52.xml",
        "scenarios/protobuf.xml",
        "scenarios/currency_sample.xml",
        "fileversions/client69.binary.portfolio",
        "scenarios/client_performance_snapshot.xml",
        "issues/Issue1498FifoCrossPortfolio.xml",
    ] {
        let a = load(f, None).client;
        let xml = pp_import::export::to_pp_xml(&a);
        assert!(xml.starts_with("<client id=\"1\">"));
        let b = read_client(xml.as_bytes(), None)
            .unwrap_or_else(|e| panic!("{f}: {e}"))
            .client;
        assert_eq!(fingerprint(&a), fingerprint(&b), "{f}");
        // investment plans survive (ids are regenerated)
        let plans = |c: &Client| {
            c.plans
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        p.kind,
                        p.security.clone(),
                        p.account.clone(),
                        p.amount,
                        p.transactions.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(plans(&a), plans(&b), "{f}");
        // taxonomies with assignments survive
        let count = |c: &Client| {
            c.taxonomies
                .iter()
                .flat_map(|t| t.classifications.iter())
                .map(|c| c.assignments.len())
                .sum::<usize>()
        };
        assert_eq!(count(&a), count(&b), "{f}");
        // units and cross entries survive
        let units = |c: &Client| {
            c.portfolios
                .iter()
                .flat_map(|p| p.transactions.iter())
                .map(|t| format!("{:?}", t.units))
                .collect::<Vec<_>>()
        };
        assert_eq!(units(&a), units(&b), "{f}");
        let x = |c: &Client| {
            c.accounts
                .iter()
                .flat_map(|p| p.transactions.iter())
                .filter_map(|t| {
                    t.cross_entry
                        .as_ref()
                        .map(|c| (c.kind, c.other_transaction.clone()))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(x(&a), x(&b), "{f}");
    }
}

#[test]
fn reads_investment_plans() {
    let xml = load("fileversions/client52.xml", None).client;
    let bin = load("fileversions/client52.binary.portfolio", None).client;
    assert_eq!(xml.plans.len(), 3);
    let summary = |c: &Client| {
        c.plans
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.kind,
                    p.interval,
                    p.amount,
                    p.fees,
                    p.transactions.len(),
                    p.start,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(summary(&xml), summary(&bin));
    assert!(
        xml.plans
            .iter()
            .any(|p| p.kind == pp_core::plan::PlanType::PurchaseOrDelivery && p.account.is_none())
    );
}
