use chrono::DateTime;
use chrono::FixedOffset;
use core::iter;
use encoded_words::decode;
use imap::types::Flag;
use mailparse::MailHeaderMap;
use mailparse::ParsedMail;
use mailparse::parse_mail;
use native_tls::{TlsConnector, TlsStream};
use std::collections::HashMap;
use std::env;
use std::net::TcpStream;
use utf7_imap::decode_utf7_imap;
use utf7_imap::encode_utf7_imap;

fn split_and_parse_header(email: &ParsedMail, name: &str) -> Vec<String> {
    email
        .headers
        .get_first_value(name)
        .map(|from| from.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default()
}

type Session = imap::Session<TlsStream<TcpStream>>;

type Map = HashMap<String, (String, u32)>;

struct Email(Session, Map);

impl Email {
    fn parse_email(session: &mut Session, map: &Map, uid: u32) -> EmailData {
        let messages = session.uid_fetch(uid.to_string(), "RFC822").unwrap();
        let message = messages.iter().next().unwrap();
        let raw = message.body().unwrap();
        let parsed = parse_mail(raw).unwrap();

        let from = split_and_parse_header(&parsed, "From");
        let to = split_and_parse_header(&parsed, "To");
        let cc = split_and_parse_header(&parsed, "Cc");
        let bcc = split_and_parse_header(&parsed, "Bcc");
        let subject =
            parsed.headers.get_first_value("Subject").unwrap_or_default();
        let parent = parsed
            .headers
            .get_first_value("In-Reply-To")
            .map(|parent_mid| map.get(&parent_mid).unwrap().to_owned());

        let datetime = DateTime::parse_from_rfc2822(
            &parsed.headers.get_first_value("Date").unwrap(),
        )
        .unwrap();

        let mut txt = String::new();
        let mut html = String::new();

        for part in parsed.subparts.iter().chain(iter::once(&parsed)) {
            match part.ctype.mimetype.as_str() {
                "text/plain" => txt = part.get_body().unwrap_or_default(),
                "text/html" => html = part.get_body().unwrap_or_default(),
                _ => {}
            }
        }

        EmailData {
            txt,
            html,
            from,
            to,
            subject,
            cc,
            bcc,
            datetime,
            attachements: vec![],
            parent,
        }
    }

    fn new() -> Self {
        let domain = env::var("DOMAIN").unwrap();
        let username = env::var("USERNAME").unwrap();
        let password = env::var("PASSWORD").unwrap();

        let ssl_connector = TlsConnector::builder().build().unwrap();
        let addr = (domain.as_str(), 993);
        let client =
            imap::connect(addr, domain.as_str(), &ssl_connector).unwrap();

        let session = client.login(username, password).unwrap();

        Self(session, HashMap::new())
    }

    fn populate_mids(&mut self) {
        let boxes = self.list_boxes();

        for b in boxes {
            println!("Populating mids for box: {b}");
            if let Err(e) = self.0.select(encode_utf7_imap(b.clone())) {
                eprintln!("Failed to select box {b}: {e}");
                continue;
            }
            let fetches = self.0.uid_fetch("1:*", "(UID ENVELOPE)").unwrap();

            for msg in fetches.iter() {
                let uid = msg.uid.unwrap();
                let mid = msg.envelope().unwrap().message_id.unwrap();

                self.1.insert(
                    String::from_utf8(mid.to_vec()).unwrap(),
                    (b.clone(), uid),
                );
            }
        }

        self.0.close().unwrap();
    }

    fn first_inbox_message(&mut self) -> (bool, String) {
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

    fn find_with_subject(
        &mut self,
        subject: &str,
    ) -> Option<(String, u32, EmailData)> {
        println!("Searching for {subject}");
        for (_mid, (folder, uid)) in &self.1 {
            self.0.select(folder).unwrap();
            let data = Self::parse_email(&mut self.0, &self.1, *uid);
            if data.subject == subject {
                return Some((folder.to_owned(), *uid, data));
            }
        }
        self.0.close().unwrap();
        None
    }
}

impl Drop for Email {
    fn drop(&mut self) {
        self.0.logout().unwrap();
    }
}

#[derive(Debug)]
struct EmailData {
    txt: String,
    html: String,
    from: Vec<String>,
    to: Vec<String>,
    subject: String,
    cc: Vec<String>,
    bcc: Vec<String>,
    datetime: DateTime<FixedOffset>,
    parent: Option<(String, u32)>,
    attachements: Vec<Attachment>,
}

#[derive(Debug, Default)]
struct Attachment {
    filename: String,
    mime: String,
    data: Vec<u8>,
}

fn main() {
    let mut app = Email::new();
    app.populate_mids();

    let (_, puid, parent) = app.find_with_subject("Example subject").unwrap();
    dbg!(puid);
    dbg!(parent.parent);

    let (_, uid, reply) =
        app.find_with_subject("Reply to Example subject").unwrap();
    dbg!(uid);
    dbg!(reply.parent);

    app.0.select("INBOX").unwrap();
    dbg!(app.first_inbox_message());
    dbg!(app.list_boxes());
    dbg!(app.most_recent());
}
