extern crate failure;
extern crate regex;
extern crate reqwest;
#[macro_use]
extern crate structopt;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use failure::{Fail, Fallible};
use failure::err_msg;
use regex::{Match, Regex};
use reqwest::{get, Url};
use sequoia::core::Context;
use sequoia::openpgp::parse::Parse;
use sequoia::openpgp::TPK;
use sequoia::store::Store;
use structopt::StructOpt;

use crate::remailer::Remailer;

mod remailer;

const REALM_REMAILER: &'static str = "Remailers' keys";

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

    /// Retrieve the remailer stats at this URL
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
    let stats = get_stats(&args).unwrap();
    let keys = get_tpks().unwrap();
    dbg!(&args);
    //dbg!(&stats);
    //dbg!(&keys);
    store_keys(keys);
}

/// IMatch represents all name captures of one match used in `crate::get_stats()` !
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

/// Retrieve remailers data from an echolot point like name, email, options, date, latency and uptimes
fn get_stats(args: &Args) -> Fallible<Vec<Remailer>> {

    // Retrieve the text version of remailer list from the Args::stats_source
    let text: Fallible<String> = {
        let url = (&args.stats_source).clone() + "rlist.txt";
        let mut response = get(Url::parse(&url)?)?;
        Ok(response.text()?)
    };
    let text = text?;

    // The future remailers list
    let mut remailers: HashMap<String, Remailer> = HashMap::new();

    // The magic regex to parse the rlist from Args::stats_source
    let regex = r#"(?m)^(\$remailer\{"(?P<name>[a-z]+)"\}\s=\s"<(?P<email_address>[a-z]+@[a-z0-9\-.]+)>(?P<options>(\s[a-z0-9]+)+)";|Last\supdate:\s(?P<date>[MonTueWdhFriSatun]{3}\s\d{2}\s[A-Z][a-z]{2}\s\d{4}\s\d{2}:\d{2}:\d{2} [A-Z]{3})|(?P<name2>[a-z]+)\s+(?P<email_address2>[\w\d]+@[\w\d.-]+)\s+[*? +\-#._]+\s+\s(?P<latency>[\d:]{0,2}[0-6]\d:[0-6]\d)\s+(?P<uptime>\d{1,3}.\d{1,2})%)$"#;
    let regex = Regex::new(regex)?;

    regex.captures_iter(text.as_str()).map(|capture| {
        // Fill the present captured group (via Option) in the match (or Capture in regex crate words)
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
            // For match on the remailer description at the format below:
            // `$remailer{"<name>"} = "<remailer address>" <option>..`
            // eg:
            // `$remailer{"austria"} = "<mixmaster@remailer.privacy.at> cpunk max mix pgp pgponly repgp remix latent hash cut test ekx inflt50 rhop5 reord klen1024";`
            IMatch {
                name: Some(name),
                email: Some(email_address),
                options: Some(options), ..
            } => {
                match remailers.get_mut(name.as_str()) {
                    // In case the remailer has already been created by a previous match
                    Some(remailer) => {
                        Remailer::set_options(remailer, options.as_str()
                            .split(" ").map(|s| s.to_string())
                            .collect())
                    }
                    // In case the remailer has not yet been created by a previous match
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
            // Match on the update date
            //
            // Formatted example:
            // `Last update: Sat 24 Aug 2019 16:00:01 GMT`
            IMatch { date: Some(date), .. } => {
                println!("Stats was upadated on {}", date.as_str());
            }
            // Match on uptime and latency data for a remailer
            //
            // Format:
            // `<name> <email> <ignored_latency> <latency_in_time_hh:mm:ss> <uptime_in_percent>`
            //
            // Example:
            // `austria  mixmaster@remailer.privacy.at    ************    25:59 100.00%`
            IMatch {
                name2: Some(name),
                email2: Some(email_address),
                latency: Some(latency),
                uptime: Some(uptime), ..
            } => {
                match remailers.get_mut(name.as_str()) {
                    // In case the remailer has already been created by a previous match
                    Some(remailer) => {
                        Remailer::set_latency_from(remailer, String::from(latency.as_str()))
                            .map_err(|err| {
                                eprintln!("Can't convert latency from stats URL in acceptable value!\nError: {}", err);
                            });
                        Remailer::set_uptime(remailer, uptime.as_str().parse::<f32>().unwrap_or(0f32));
                    }
                    // In case the remailer has not yet been created by a previous match
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
    // Return the final list of remailer (and convert the HashMap into a Vec)
    Ok(remailers.values().map(|remailer| remailer.clone()).collect())
}

fn get_tpks() -> Fallible<Vec<TPK>> {
    let mut tpks: Vec<TPK> = Vec::new(); // The future list of TPK (or public key)
    let keys_path = Path::new("./remailer-keys/");
    if !keys_path.exists() || !keys_path.is_dir() { // Show error if the `remailer-keys` directory doesn't exist
        eprintln!("The `remailer-keys` directory doesn't exist! It's required!");
    }
    let keys_dir = std::fs::read_dir(keys_path)?;
    for entry in keys_dir {
        let entry = entry?;
        if entry.path().is_dir() {
            println!("Entry `{}` ignored!", entry.path().to_string_lossy());
        } else {
            let tpk = TPK::from_file(entry.path()) // Load all public keys located in the dir
                .map_err(|err| err.context(format!("Failed to load key from file {:?}", entry.path())))?;
            tpks.push(tpk);
        }
    }
    Ok(tpks)
}

/// Store the TPKs in the Sequoia `remailer` store/keyring
///
/// Each key is registered by its emails
fn store_keys(keys: Vec<TPK>) -> Fallible<()> {
    let store = Store::open(&Context::new()?, REALM_REMAILER, "remailer")?; // Open `remailer` store

    for key in keys { // Add keys in the store
        for user in key.userids() {
            match user.userid().address_normalized()? {
                Some(email) => {
                    println!("Key for `{}` imported", email);
                    store.import(&email, &key); // Import them if it contains a valid email address
                }
                None => {
                    println!("Key ignored! {:?}", user.userid());
                }
                _ => {
                    eprintln!("Key userid is not a valid Option value!");
                }
            }
        }
    }
    Ok(())
}
