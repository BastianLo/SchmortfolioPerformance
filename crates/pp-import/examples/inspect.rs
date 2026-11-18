fn main() {
    let path = std::env::args().nth(1).unwrap();
    let pwd = std::env::args().nth(2);
    let bytes = std::fs::read(&path).unwrap();
    let r = pp_import::read_client(&bytes, pwd.as_deref()).unwrap();
    let c = &r.client;
    println!("{:?}", r.info);
    println!("base={} securities={} accounts={} portfolios={} taxonomies={} watchlists={}", c.base_currency, c.securities.len(), c.accounts.len(), c.portfolios.len(), c.taxonomies.len(), c.watchlists.len());
    for a in &c.accounts { println!("  ACC {} {} tx={}", a.name, a.currency, a.transactions.len()); }
    for p in &c.portfolios { println!("  PF {} tx={}", p.name, p.transactions.len()); for t in p.transactions.iter().take(2) { println!("     {:?} {} {} {} sh={} units={:?}", t.kind, t.date_time, t.currency, t.amount, t.shares, t.units); } }
    for s in c.securities.iter().take(3) { println!("  SEC {} {:?} prices={} first={:?}", s.name, s.currency, s.prices.len(), s.prices.first()); }
}
