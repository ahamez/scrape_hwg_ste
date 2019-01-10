extern crate lettre;
extern crate lettre_email;
extern crate native_tls;
extern crate regex;
extern crate reqwest;
extern crate serde_json;
#[macro_use] extern crate serde_derive;

use lettre::EmailTransport;
use lettre::smtp::authentication::Credentials;
use lettre::smtp::{ClientSecurity, SmtpTransportBuilder};
use lettre::smtp::client::net::ClientTlsParameters;
use lettre_email::EmailBuilder;
use lettre_email::Email;
use native_tls::TlsConnector;
use std::fs::File;
use std::net::ToSocketAddrs;

use std::error::Error;

/* --------------------------------------------------------------------------------------------- */

#[derive(Deserialize)]
struct Configuration {
  sensor_uri: String,
  smtp_domain: String,
  smtp_port: u16,
  smtp_login: String,
  smtp_password: String,
  max_temperature: f32,
  period: u64,
  mail_recipients: Vec<String>,
}

/* --------------------------------------------------------------------------------------------- */

fn make_email(conf: &Configuration, subject: &str) -> Email {
  let mut builder = EmailBuilder::new()
    .from("FROM@EXAMPLE.COM")
    .subject(subject);

    for recipient in &conf.mail_recipients {
      builder = builder.to(&recipient[..]);
    }

  builder
    .build()
    .expect("Unable to build email")
}

/* --------------------------------------------------------------------------------------------- */

fn main() {

  if std::env::args().len() != 2 {
    println!("Usage: {} /path/to/conf.json", std::env::args().next().unwrap());
    std::process::exit(1);
  }

  let conf : Configuration = {
    std::env::args().next();
    let file = File::open("configuration.json").expect("Can't open configuration file");
    serde_json::from_reader(file).expect("Can't parse configuration JSON data")
  };

  // <div class="value" id="s215">22.7&nbsp;°C</div>
  let re = regex::Regex::new(
    r###"id="s215">([+-]?([0-9]+([.][0-9]*)?|[.][0-9]+))"###
  ).unwrap();

  let mut mailer = {
    let mut smtp_server_sockaddr_iter = (&conf.smtp_domain[..], conf.smtp_port)
      .to_socket_addrs()
      .expect("Unable to get sockaddr");

    let smtp_server_sockaddr = smtp_server_sockaddr_iter
      .next()
      .unwrap();

    let tls = TlsConnector::builder()
      .expect("Unable to create TLS connector builder")
      .build()
      .expect("Unable to build tls");

    let tls_parameters = ClientTlsParameters::new(
        conf.smtp_domain.clone(),
        tls);

    SmtpTransportBuilder::new(
        smtp_server_sockaddr,
        ClientSecurity::Required(tls_parameters))
      .expect("Unable to create SMTP transport")
      .credentials(Credentials::new(conf.smtp_login.clone(), conf.smtp_password.clone()))
      .build()
    };

  let mut alert = false;

  loop {

    // https://stackoverflow.com/a/41129287/21584
    let res : Result<_, Box<Error>> = reqwest::get(&conf.sensor_uri)
      .map_err(Into::into)
      .and_then(|mut raw| raw.text()
      .map_err(Into::into))
      .and_then(|html|
        match re.captures(&html[..]) {
          Some(capture) => match capture[1].parse::<f32>() {
                             Ok(temperature) => Ok(temperature),
                             Err(_) => Err("Cannot parse temperature")
                           }
          None          => Err("Temperature regex failed")
        }
      .map_err(Into::into));

    let maybe_str = match res {
      Ok(temperature) => {
        println!("temperature={}°C, alert={}", temperature, alert);
        if temperature > conf.max_temperature {
          Some(format!("Temperature = {}!", temperature))
        }
        else {
          None
        }
      }

      Err(e) => {
        Some(format!("{:?}", e))
      }
    };

    match maybe_str {
      Some(s) => {
        if !alert {
          mailer.send(&make_email(&conf, &s)).unwrap();
          alert = true;
        }
        else {
          alert = false;
        }
      }
      _ => {}
    }

    std::thread::sleep(std::time::Duration::from_secs(conf.period));
  }
}
