use std::env;
use std::net::TcpStream;

use encoded_words::decode;
use imap::types::Flag;
use native_tls::{TlsConnector, TlsStream};
use utf7_imap::decode_utf7_imap;

struct Email(imap::Session<TlsStream<TcpStream>>);

impl Email {
    fn new() -> Self {
        let domain = env::var("DOMAIN").unwrap();
        let username = env::var("USERNAME").unwrap();
        let password = env::var("PASSWORD").unwrap();

        let ssl_connector = TlsConnector::builder().build().unwrap();
        let addr = (domain.as_str(), 993);
        let client =
            imap::connect(addr, domain.as_str(), &ssl_connector).unwrap();

        let session = client.login(username, password).unwrap();
        Self(session)
    }

    fn first_inbox_message(&mut self) -> (bool, String) {
        self.0.select("INBOX").unwrap();

        let messages = self.0.fetch("1", "(FLAGS BODY.PEEK[])").unwrap();
        let message = messages.iter().next().unwrap();

        let seen = message.flags().contains(&Flag::Seen);
        let body = str::from_utf8(message.body().unwrap()).unwrap().to_owned();
        (seen, body)
    }

    fn list_boxes(&mut self) -> Vec<String> {
        self.0
            .list(None, Some("*"))
            .unwrap()
            .into_iter()
            .map(|b| decode_utf7_imap(b.name().to_owned()))
            .collect()
    }

    fn most_recent(&mut self) -> Vec<String> {
        self.0.select("INBOX").unwrap();
        let uids = self.0.uid_search("ALL").unwrap();
        let mut uids: Vec<u32> = uids.into_iter().collect();
        uids.sort_unstable();
        let recent = uids
            .iter()
            .rev()
            .take(100)
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.0
            .uid_fetch(recent.as_str(), "ENVELOPE")
            .unwrap()
            .into_iter()
            .map(|m| {
                let envelope = m.envelope().unwrap();
                let string =
                    String::from_utf8_lossy(envelope.subject.unwrap_or(&[]))
                        .to_string();
                decode(&string).map(|x| x.decoded).unwrap_or(string)
            })
            .collect()
    }
}

impl Drop for Email {
    fn drop(&mut self) {
        self.0.logout().unwrap();
    }
}

fn main() {
    let mut app = Email::new();

    dbg!(app.first_inbox_message());
    dbg!(app.list_boxes());
    dbg!(app.most_recent());
}
