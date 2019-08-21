#[macro_use]
extern crate structopt;
extern crate reqwest;
extern crate failure;
extern crate regex;

use structopt::StructOpt;
use reqwest::{get, Url};
use failure::Fallible;
use std::collections::HashMap;
use regex::{Regex, Match};
use crate::remailer::Remailer;
use failure::err_msg;
use std::time::Duration;

mod remailer;

#[derive(StructOpt, Debug)]
#[structopt(name = "Cypherpunk CLI")]
struct Args {
    /// The final message for the recipient
    //#[structopt(long)]
    message: String,

    /// Append header for the final sender/remailer
    /// eg: `--header "X-Header: Me"`
    /// eg2: `--header "Header1: Value1" "Header2: Value2"`
    #[structopt(short = "h", long = "header")]
    headers: Vec<String>,

    /// The remailer chain of your message
    /// The can up to 8 remailers
    ///
    /// eg: `--chain paranoia dizum banana`
    /// eg2: `--chain * * *`
    #[structopt(short = "c", long = "chain")]
    chain: Vec<String>,

    /// Remailers can sometimes forgive mail, for that reason you can send multiple mails to ensure they will be recieved
    #[structopt(short = "r", long = "redundancy", default_value = "1")]
    redundancy: i8,

    /// Format the final mail into a mailto URL
    ///
    /// mailto URL seems that: `mailto:<email>?body=<body email>`
    ///
    /// Example output:
    ///
    /// ```
    /// mailto:remailer@redjohn.net?body=%3A%3A%0AEncrypted%3A%20PGP%0A%0A-----BEGIN%20PGP%20MESSAGE-----<PGP ENCRYPTED MESSAGE HERE>-----END%20PGP%20MESSAGE-----
    /// ```
    #[structopt(short = "m", long = "mailto_link")]
    mailto_link: bool,

    /// Update the remailer stats with this URL
    ///
    /// How to get it ?
    /// 1. Search for "remailer statistics"
    /// 2. Remove the latest part of the url.
    /// (eg: `https://remailer.paranoici.org/rlist.html` => `https://remailer.paranoici.org/`
    /// Use this url with the `--stats <URL>` arguments
    #[structopt(short = "s", long = "stats", default_value = "https://remailer.paranoici.org/")]
    stats_source: String,
}

fn main() {
    println!("Hello, world!");
    let args = Args::from_args();
    dbg!(&args);
    dbg!(get_stats(&args));
}

/// IMatch represents all name captures of one match!
#[derive(Default, Debug)]
struct IMatch<'a> {
    name: Option<Match<'a>>,
    email: Option<Match<'a>>,
    options: Option<Match<'a>>,
    date: Option<Match<'a>>,
    name2: Option<Match<'a>>,
    email2: Option<Match<'a>>,
    latency: Option<Match<'a>>,
    uptime: Option<Match<'a>>,
}


fn get_stats(args: &Args) -> Fallible<Vec<Remailer>> {
    let text: Fallible<String> = {
        let url = (&args.stats_source).clone() + "rlist.txt";
        let mut response = get(Url::parse(&url)?)?;
        Ok(response.text()?)
    };
    let text = text?;

    let mut remailers: HashMap<String, Remailer> = HashMap::new();

    let regex = r#"(?m)^(\$remailer\{"(?P<name>[a-z]+)"\}\s=\s"<(?P<email_address>[a-z]+@[a-z0-9\-.]+)>(?P<options>(\s[a-z0-9]+)+)";|Last\supdate:\s(?P<date>[MonTueWdhFriSatun]{3}\s\d{2}\s[A-Z][a-z]{2}\s\d{4}\s\d{2}:\d{2}:\d{2} [A-Z]{3})|(?P<name2>[a-z]+)\s+(?P<email_address2>[\w\d]+@[\w\d.-]+)\s+[*? +\-#._]+\s+\s(?P<latency>[\d:]{0,2}[0-6]\d:[0-6]\d)\s+(?P<uptime>\d{1,3}.\d{1,2})%)$"#;
    let regex = Regex::new(regex)?;

    regex.captures_iter(text.as_str()).map(|capture| {
        IMatch {
            name: capture.name("name"),
            email: capture.name("email_address"),
            options: capture.name("options"),
            date: capture.name("date"),
            name2: capture.name("name2"),
            email2: capture.name("email_address2"),
            latency: capture.name("latency"),
            uptime: capture.name("uptime"),
        }
    }).for_each(|imatch| {
        match imatch {
            IMatch {
                name: Some(name),
                email: Some(email_address),
                options: Some(options), ..
            } => {
                match remailers.get_mut(name.as_str()) {
                    Some(remailer) => {
                        Remailer::set_options(remailer, options.as_str()
                            .split(" ").map(|s| s.to_string())
                            .collect())
                    }
                    None => {
                        let remailer = Remailer {
                            name: String::from(name.as_str()),
                            email_address: String::from(email_address.as_str()),
                            options: options.as_str()
                                .split(" ").map(|s| s.to_string())
                                .collect(),
                            ..Default::default()
                        };
                        remailers.insert(remailer.get_name().clone(), remailer);
                    }
                }
            }
            IMatch { date: Some(date), .. } => {
                println!("Stats was upadated on {}", date.as_str());
            }
            IMatch {
                name2: Some(name),
                email2: Some(email_address),
                latency: Some(latency),
                uptime: Some(uptime), ..
            } => {
                match remailers.get_mut(name.as_str()) {
                    Some(remailer) => {
                        Remailer::set_latency_from(remailer, String::from(latency.as_str()))
                            .map_err(|err| {
                                eprintln!("Can't convert latency from stats URL in acceptable value!\nError: {}", err);
                            });
                        Remailer::set_uptime(remailer, uptime.as_str().parse::<f32>().unwrap_or(0f32));
                    }
                    None => {
                        let mut remailer = Remailer {
                            name: String::from(name.as_str()),
                            email_address: String::from(email_address.as_str()),
                            uptime: uptime.as_str().parse::<f32>().unwrap_or(0f32),
                            ..Default::default()
                        };
                        remailer.set_latency_from(String::from(latency.as_str()))
                            .map_err(|err| {
                                eprintln!("Can't convert latency from stats URL in acceptable value!\nError: {}", err);
                            });
                        remailers.insert(remailer.get_name().clone(), remailer);
                    }
                }
            }
            others => {
                dbg!(others);
            }
        }
    });
    Ok(remailers.values().map(|remailer| remailer.clone()).collect())
}