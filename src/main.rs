extern crate failure;
#[cfg(feature = "back-gpgme", )]
extern crate gpgme;
extern crate percent_encoding;
extern crate rand;
extern crate regex;
extern crate reqwest;
#[cfg(feature = "back-sequoia", )]
extern crate sequoia;
extern crate structopt;

use std::fs;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use failure::Fallible;
use failure::err_msg;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rand::seq::SliceRandom;
use regex::{Regex};
use reqwest::{get, Url};
use structopt::StructOpt;

use crate::remailer::Remailer;
use crate::utils::IMatch;

mod remailer;
mod utils;
mod pgp;


#[derive(StructOpt, Debug)]
#[structopt(name = "Cypherpunk CLI")]
struct Args {
    /// The path to the final message for the recipient
    ///
    /// This message need to be valid Cypherpunk message.
    /// Cypherpunk format:
    /// ```
    /// ::
    /// Anon-To: <fianl_recipient>
    /// Header2: Value2
    /// Header3: Value3
    ///
    /// ##
    /// Subject: Subject of the message
    ///
    /// The message body
    /// ```
    ///
    /// If you omit the `Subject` special header, remove the line `##`
    #[structopt(parse(from_os_str))]
    message: PathBuf,

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
    #[structopt(short = "m", long = "mailto")]
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

    /// No output from GPG
    #[structopt(short = "q", long)]
    quiet: bool,
}

#[cfg(feature = "back-gpgme", )]
fn main() {
    use gpgme::Context;

    println!("Hello, world!");
    let args = Args::from_args();

    // Get remailers from the stats
    let remailers = get_stats(&args).unwrap();

    // Init a GPGContext needed by GPGME to import key and to encrypt message.
    let mut context = Context::from_protocol(gpgme::Protocol::OpenPgp).expect("Can't create a valid GPGME context");

    // Import keys & encrypt message using GPGME
    import_keys(|key| pgp::gpgme::import_key(&mut context, key)).unwrap();
    let messages = encrypt_message(&args, &remailers, |_, input, output, recipients| pgp::gpgme::encrypt(&mut context, input, output, recipients)).unwrap();

    messages.iter().for_each(|message| println!("{}", message));
}

#[cfg(feature = "back-gpg", )]
fn main() {
    println!("Hello, world!");
    let args = Args::from_args();

    // Get remailers from the stats
    let remailers = get_stats(&args).unwrap();

    // Import keys & encrypt message using GPG
    import_keys(|key| pgp::gpg::import_key(key)).unwrap();
    let messages = encrypt_message(&args, &remailers, |_, input, output, recipients| pgp::gpg::encrypt(input, output, recipients, (&args).quiet)).unwrap();

    messages.iter().for_each(|message| println!("{}", message));
}

#[cfg(feature = "back-sequoia", )]
const REALM_REMAILER: &'static str = "Remailers' keys";

#[cfg(feature = "back-sequoia", )]
fn main() {
    use sequoia::store::Store;
    use sequoia::core::Context;

    println!("Hello, world!");
    let args = Args::from_args();

    // Get remailers from the stats
    let remailers = get_stats(&args).unwrap();

    // Init a Store needed by Sequoia to import key, it's an equivalent of a keyring
    let mut store = Store::open(&Context::new().expect("Can't init a valid Sequoia context"), REALM_REMAILER, "remailer").expect("Can't open Sequoia store!"); // Open `remailer` store

    // Import keys & encrypt message using SequoiaPGP
    import_keys(|key| pgp::sequoia::import_key(key)).unwrap();
    let messages = encrypt_message(&args, &remailers, |_, input, output, recipients| pgp::sequoia::encrypt(&mut store, input, output, recipients)).unwrap();

    messages.iter().for_each(|message| println!("{}", message));
}

/// Retrieve remailers data from an echolot point like name, email, options, date, latency and uptimes
fn get_stats(args: &Args) -> Fallible<HashMap<String, Remailer>> {

    // Retrieve the text version of remailer list from the Args::stats_source
    let text: Fallible<String> = {
        let url = (&args.stats_source).clone() + "stats/rlist.txt";
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
                            }).unwrap();
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
                            }).unwrap();
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
    Ok(remailers)
}

/// Load known keys from the dir and import it in the PGP backend via the import_key function in args
fn import_keys<F>(mut import_key: F) -> Fallible<()> where
    F: FnMut(PathBuf) -> Fallible<()>
{
    println!("Let's import the keys...");

    // Retrieve the keys path and check it
    let keys_path = Path::new("./remailer-keys/");
    if !keys_path.exists() || !keys_path.is_dir() { // Show error if the `remailer-keys` directory doesn't exist
        eprintln!("The `remailer-keys` directory doesn't exist! It's required!");
    }
    let keys_dir = std::fs::read_dir(keys_path)?; // List the keys presents in the directory
    for entry in keys_dir {
        match entry {
            Ok(entry) => {
                if entry.path().is_dir() { // Ignoring dir
                    println!("Entry `{}` ignored!", entry.path().to_string_lossy());
                } else { // Importing "key" file using the given `import_key` function
                    import_key(entry.path())
                        // Map errors and don't panic or return the fail, just ignore this key!
                        .map_err(|err| eprintln!("An error occurred when importing the key, it'll be ignored: {:?}", err)).unwrap();
                }
            }
            // Ignore invalid DirEntry
            Err(err) => eprintln!("An error occurred on a dir entry, it'll be ignored: {:?}", err),
        }
    }

    println!("Keys successfully imported!");
    Ok(())
}

/// Encrypt message (and its redundancies) trough all the remailer chain (sometime randomly)
fn encrypt_message<F>(args: &Args, remailers: &HashMap<String, Remailer>, mut encrypt: F) -> Fallible<Vec<String>>
    where F: FnMut(&Remailer, &mut dyn Read, &mut dyn Write, Vec<&str>) -> Fallible<()> {
    let Args { message, chain, redundancy, .. } = args; // Get message path, chain and redundancy args

    let mut encrypted_messages = Vec::new(); // The message collector
    let message = fs::read_to_string(message)?.into_bytes(); // Read bytes from message file

    // Prepare thing needed when a remailer must be choose randomly
    let mut rng = rand::thread_rng();
    let mut random_remailers: Vec<&String> = remailers.keys().collect();

    for r_count in 0..(*redundancy as u8) {
        println!("Let's encrypt the message... [redundancy: {}]", r_count + 1);

        random_remailers.shuffle(&mut rng); // Shuffle the list of remailer

        // start encrypting the message; starting with the message (provided as an argument) and
        // continuing until you get the final message that the user should send to the first
        // remailer (here the last in the chain [which is reversed from the arguments provided])
        let encrypted_message: Fallible<Vec<u8>> = chain.iter().rev().fold(Ok(message.clone()), |input: Fallible<Vec<u8>>, remailer: &String| {

            // Retrieve the chosen remailer
            let remailer: Fallible<Remailer> = match remailer.as_str() {
                "*" => { // If the remailer provided is "*", it choose one randomly in the vector "random_remailer"
                    let remailer_id = random_remailers.choose(&mut rng).ok_or(err_msg("can't choose randomly a remailer_id"))?;
                    Ok(remailers.get(remailer_id.clone()).ok_or(err_msg("unknown remailer id ?!"))?.clone())
                }

                // In the case where it is not randomly selected and exists for the tool, a clone is created from it
                other if remailers.contains_key(&other.to_string()) => {
                    Ok(remailers.get(other).ok_or(err_msg(format!("unknown remailer: {}", other)))?.clone())
                }
                _ => Err(err_msg(format!("The remailer named `{}` is unknown!", remailer)))
            };
            let remailer = remailer?;

            println!("This message part will be encrypted for {} remailer...", remailer.get_name());

            // Create an input with Read trait => we choose the Cursor<Vec<u8>>
            // and embed the input (Vec<u8>) provided in arguments
            let mut pgp_input = Cursor::new(input?);

            // Create an output with Write trait => we choose again a Cursor<Vec<u8>>
            let mut pgp_output = Cursor::new(Vec::new());

            // Indicate who the recipients are
            let mut recipients = Vec::new();
            recipients.push(remailer.get_email().as_str()); // Push the remailer email

            // Encrypt the input (message) with the given function `encrypt`
            encrypt(&remailer, &mut pgp_input, &mut pgp_output, recipients)?;

            // Format the output into a Remailer-valid message
            let mut output: Vec<u8> = Vec::new();
            output.append(&mut Vec::from(format!("\n::\nAnon-To: {}\n\n::\nEncrypted: PGP\n\n", remailer.get_email())));

            output.append(&mut pgp_output.into_inner()); // Append the encrypted message part

            Ok(output) // Return it
        });

        // Check that the creation of the final message did not fail
        match encrypted_message {
            Ok(message) => {
                println!("Message encrypted!");

                let encrypted_message: String = String::from_utf8(message)?;
                match args.mailto_link { // Push in the message collector, if needed format the message before.
                    true => encrypted_messages.push(format_mailto(encrypted_message)),
                    false => encrypted_messages.push(encrypted_message),
                };
            }
            Err(err) => {
                eprintln!("Message ignored, error occurred: {:#?}", err);
            }
        }
    }
    Ok(encrypted_messages)
}

/// Format the final message (from the String) into a mailto URL
fn format_mailto(message: String) -> String {
    println!("Let's format the final message into mailto URL format!");

    let mut message = message.clone();
    let email_start = message.find(": ").expect("Invalid Cypherpunk message"); // Find the email start
    message.drain(..email_start + 2); // Drop all chars before email address
    let email_end = message.find("\n\n").expect("Invalid Cypherpunk message"); // Find the email end
    let email: String = message.drain(..email_end).collect(); // Save the email address in the var, and drop all chars before message
    message.drain(..1);

    let body = utf8_percent_encode(message.as_str(), NON_ALPHANUMERIC).to_string(); // Encode message to UTF-8 Percent Encoding
    format!("mailto:{}?body={}", email, body).to_string()
}