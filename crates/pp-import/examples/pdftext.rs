fn main() {
    let rules = pp_import::pdf::builtin_rules();
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let r = pp_import::pdf::extract(&text, &rules);
        println!("{path}: {:?} {:?} {:?}", r.bank, r.document, r.errors);
        for t in r.transactions {
            println!(
                "   {} {} isin={:?} wkn={:?} name={:?} sh={} amt={} {} fee={} tax={} sec={:?} fx={:?}",
                t.kind,
                t.date_time,
                t.isin,
                t.wkn,
                t.name,
                t.shares,
                t.amount,
                t.currency,
                t.fees,
                t.taxes,
                t.security_currency,
                t.exchange_rate
            );
        }
    }
}
