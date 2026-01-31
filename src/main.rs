use std::env;

use native_tls::TlsConnector;

const BODY: &str = "RFC822";

fn main() {
    let domain = env::var("DOMAIN").unwrap();
    let username = env::var("USERNAME").unwrap();
    let password = env::var("PASSWORD").unwrap();

    let ssl_connector = TlsConnector::builder().build().unwrap();
    let addr = (domain.as_str(), 993);
    let client = imap::connect(addr, domain.as_str(), &ssl_connector).unwrap();

    let mut imap_session = client.login(username, password).unwrap();

    imap_session.select("INBOX").unwrap();

    let messages = imap_session.fetch("1", BODY).unwrap();
    let message = messages.iter().next().unwrap();
    let body = str::from_utf8(message.body().unwrap()).unwrap();

    imap_session.logout().unwrap();

    print!("{body}");
}
