use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::{stdin, Read, Write};
use std::path::{Path, PathBuf};

use clap::arg_enum;
use failure::Error as FError;
use failure::{err_msg, Fallible, ResultExt};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rand::prelude::ThreadRng;
use rand::seq::IteratorRandom;
use rand::thread_rng;
use serde_derive::Deserialize;
use structopt::StructOpt;

use crate::lib::{Cypherpunk, CypherpunkCore, PGPBackend};
#[cfg(feature = "back-gpg")]
use crate::pgp::gpg::GPGBackend;

mod lib;
mod pgp;

// Possible output formats
arg_enum! {
    #[non_exhaustive]
    #[derive(PartialEq, Debug, Copy, Clone)]
    pub enum OutputFormat {
        Cypherpunk,
        Mailto,
        EML,
    }
}

impl OutputFormat {
    /// Get the specific extension for this particular format
    fn extension(self) -> &'static str {
        match self {
            OutputFormat::EML => "eml",
            _ => "txt",
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "cypherpunk",
    author,
    about = "CLI tool to encrypt your messages between different remailers easily"
)]
struct Opt {
    /// Messsage input file, stdin if not present; the message must be readable by the last Cypherpunk
    /// remailer in the chain.
    #[structopt(short, long, parse(from_os_str))]
    input: Option<PathBuf>,

    /// Output dir, stdout if not present; all the encrypted message for remailer will be there.
    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    /// Number of redundancy message to encrypted
    ///
    /// Because the Cypherpunk remailers can forget messages, it's a good idea to send several messages
    /// to different remailers to avoid the loss of the message.
    /// Tips: If you use a "*" for remailer it will be randomly choose for each redundancy message.
    #[structopt(short, long, default_value = "1")]
    redundancy: u8,

    /// The remailer chain through which your message will pass. [required]
    ///
    /// Tips: You can use a joker "*" to randomly choose one remailer in the config. It will change
    /// with each redundant message.
    #[structopt(short, long)]
    chain: Vec<String>,

    /// Remailer headers to add for each remailer message. Only one key-value per string.
    ///
    /// This can be useful to add `Inflate` header to each message.
    ///
    /// Examples:
    /// `--header "Key: Value"`
    /// `--header "Key1: Value1" "Key2: Value2"`
    #[structopt(short="H", long="header")]
    headers: Vec<String>,

    /// The output message format.
    #[structopt(short, long, possible_values = & OutputFormat::variants(), case_insensitive = true, default_value = "cypherpunk")]
    format: OutputFormat,

    /// The path to the remailer config, useful if you have install this tool.
    #[structopt(long, default_value="./remailers.json")]
    config: PathBuf,

    /// The quiet flag to make the PGP backend quiet and soon more...
    #[structopt(short, long)]
    quiet: bool,
}

fn main() {
    // Get the CLI args
    let opts: Opt = Opt::from_args();

    // To init a PGP Back, here for the back-gpg one.
    #[cfg(feature = "back-gpg")]
    fn init_pgp_back(quiet: bool) -> impl PGPBackend {
        GPGBackend::new(None, quiet)
    }

    println!("Hello!");
    println!("Config loading...");

    // Load config (from path arg) and run all
    load_config(&opts.config)
        .and_then(|config| {
            // Init a random thread and the remailer map from config
            let mut rng = thread_rng();
            let remmap = remailer_map(config.remailers.clone());

            // Init infra (the PGP backend)
            let pgp_back = init_pgp_back(opts.quiet);
            // Init the domain (the CypherpunkCore)
            let core = CypherpunkCore::new(pgp_back);

            // Import remailers' key
            println!("Importing remailers' key...");
            import_keys(&core, &config.remailers)?;

            // Preparing the mail encrypting
            // Select number of redundancy messages
            let red = 0..opts.redundancy;

            // Retrieve the message to send
            let mut message: Vec<u8> = Vec::new();
            match &opts.input {
                // from path, if given
                Some(path) => {
                    println!("Retrieving message from file...");
                    let mut file = File::open(path)?;
                    file.read_to_end(&mut message)?;
                }
                // from stdin, otherwise
                None => {
                    println!("\nType your message:");
                    stdin().lock().read_to_end(&mut message)?;
                    println!();
                }
            };

            // if an output path is given, create the directory
            if let Some(path) = opts.output.clone() {
                create_dir_all(path)?;
            }

            println!("Encrypting...");

            // Reverse the chain, we start the encryption for the farther remailer, etc...
            let mut chain = (&opts.chain).clone();
            chain.reverse();

            // Encrypting...
            red.map(|index| {
                println!("Encrypting message n°{}...", index + 1);
                // Build a remailer chain
                let chain =
                    make_chain(&chain, &remmap, &mut rng).context("Can't build a chain!")?;
                println!("Selected chain: {}", chain.join(", "));
                // Encrypt the message for this chain + given headers
                Ok(core.encrypt_message(&chain, &opts.headers,message.clone())?)
            })
            .enumerate()
            .map(|(index, res): (_, Fallible<Vec<u8>>)| -> Fallible<()> {
                match res {
                    // Case of valid message
                    Ok(msg) => {
                        // Case of valid utf-8 message (it should because it is an arbored PGP message)
                        if let Ok(msg) = String::from_utf8(msg) {
                            // Format the final message
                            let msg = format_msg(&opts.format, msg)?;

                            // Write the formatted message into stdout or file
                            match opts.output.clone() {
                                // Case of file output
                                Some(mut path) => {
                                    // Make the output file path
                                    path.push(
                                        format!(
                                            "redundancy_{}.{}",
                                            index + 1,
                                            &opts.format.extension()
                                        )
                                        .as_str(),
                                    );
                                    // Write the message
                                    let mut file = File::create(path.clone())?;
                                    file.write_all(msg.as_bytes())?;
                                    // Write the output path into stdout
                                    println!(
                                        "Encrypted message n°{} in {}",
                                        index + 1,
                                        path.to_string_lossy()
                                    )
                                }
                                // Case of stdout output - Just print the message
                                None => println!("Encrypted message n°{}:\n{}", index + 1, msg),
                            }
                        } else {
                            // Return error if the message is not utf-8 encoded
                            Err(err_msg(
                                "Internal Error, encrypted message is not a valid utf-8 string.",
                            ))?;
                        }
                    }
                    // Case of error during encryption
                    err => {
                        err.context(
                            format!(
                                "Message n°{}: Ignored, error occured before formatting!",
                                index + 1
                            )
                            .to_string(),
                        )?;
                    }
                }
                Ok(())
            })
            .collect::<Fallible<Vec<()>>>()?; // Collect all errors in one fallible
            Ok(())
        }) // In all errors case, don't panic just print the errors
        .unwrap_or_else(print_errors);
}

/// Import remailers' key in the Cypherpunk core from a vec of remailer.
/// It will only import enabled remailers.
fn import_keys(core: &impl Cypherpunk, remailers: &Vec<Remailer>) -> Fallible<()> {
    // Retrieve enabled remailers' keys
    let keys: Vec<Vec<u8>> = remailers
        .iter()
        .filter_map(|remailer| match remailer.is_enabled() {
            true => remailer.into_key().ok(),
            false => None,
        })
        .collect();

    // Import keys in the Cypherpunk Core
    Ok(core.import_keys(keys)?)
}

/// Format a message for a particular OutputFormat, can fail.
fn format_msg(format: &OutputFormat, msg: String) -> Fallible<String> {
    match format {
        &OutputFormat::Cypherpunk => Ok(msg),
        &OutputFormat::Mailto => Ok(format_mailto(msg)?),
        &OutputFormat::EML => Ok(format_eml(msg)?),
        // In the future case of unimplemented format...
        other => Err(err_msg(
            format!("Format {:?} not yet implemented!", other).to_string(),
        )),
    }
}

/// Format a given message to an EML-formatted email
fn format_eml(message: String) -> Fallible<String> {
    // Get address and message body
    let (addr, message) = format_helper(message)?;

    // Format it and return!
    Ok(format!(
        "MIME-Version: 1.0\n\
    Content-Type: text/plain; charset=utf-8\n\
    To: {}\n\
    \n\
    {}",
        addr, message
    )
    .to_string())
}

/// Format a given message to an mailto URL
fn format_mailto(message: String) -> Fallible<String> {
    // Get address and message body
    let (addr, message) = format_helper(message)?;

    // Encode body into utf-8 percent encode (to avoid special URL token)
    let body = utf8_percent_encode(message.as_str(), NON_ALPHANUMERIC).to_string();
    // Make it URL and return!
    Ok(format!("mailto:{}?body={}", addr, body).to_string())
}

/// Get from a given message, the message's recipient and the message's body
fn format_helper(mut message: String) -> Fallible<(String, String)> {
    // Find the `Anon-To` header
    let addr_start = message
        .find("Anon-To: ")
        .ok_or(err_msg("Invalid Cypherpunk message (Anon-To header missing)"))?; // Find the address start
    message.drain(..addr_start + 9); // Drop all chars before email address
    let addr_end = message
        .find("\n")
        .ok_or(err_msg("Invalid Cypherpunk message (Anon-To header is the only line in message)"))?; // Find the address end
    let addr: String = message.drain(..addr_end).collect(); // Save the email address in the var, and drop all chars before message
    // Look for a body (always separated by two line return to the headers)
    let body_start = message.find("\n\n").ok_or(err_msg("Invalid Cypherpunk message (Body not found)"))? + 2;
    message.drain(..body_start); // Drop all the char before the body
    // Return the recipient address and the remaining message
    Ok((addr, message))
}

/// Retrieve from path given the remailer config (using serde-json)
fn load_config<P: AsRef<Path>>(path: P) -> Fallible<RemailerConfig> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

/// Make a chain of remailers with the given "user-defined" chain
fn make_chain(
    chain: &Vec<String>,
    remmap: &HashMap<String, String>,
    rng: &mut ThreadRng,
) -> Fallible<Vec<String>> {
    // New chain holder
    let mut rchain = Vec::new();
    // For all remailers in the actual chain:
    for rem in chain {
        // Case of "randomly chosen" remailer
        if rem == "*" {
            // Return one remailer address from the map
            match remmap.values().choose(rng) {
                Some(email) => rchain.push(email.clone()),
                None => Err(err_msg("Can't choose a remailer randomly..."))?,
            }
        // Case of a named remailer
        } else {
            // If the remailer name is known in the map, we add it in the chain, otherwise we ignore
            // it and print a message in the stderr
            match remmap.get(rem) {
                Some(email) => rchain.push(email.clone()),
                None => eprintln!("Ignored remailer `{}` in the chain!", rem),
            }
        }
    }
    // If the produced chain is empty, we make an error!
    if rchain.is_empty() {
        eprintln!("No chain selected, the program will exit...");
        println!("usage: To select a remailer chain, use `-c <remailer>`");
        Err(err_msg("No chain selected"))?;
    }
    Ok(rchain)
}

/// Print error, causes and backtrace from an error.
fn print_errors(err: FError) {
    println!();
    eprintln!("Error occured: {}\n\ncauses:", err);
    err.iter_chain().enumerate().for_each(|(index, fail)| {
        eprintln!("\u{2001}{}: {}", index + 1, fail);
    });
    if let trace = err.backtrace() {
        eprintln!("\n{}", trace)
    }
}

/// A representation for the JSON config needed.
#[derive(Deserialize, Eq, PartialEq, Clone, Debug, Default)]
struct RemailerConfig {
    version: String,
    authors: Vec<String>,
    remailers: Vec<Remailer>,
}

/// A representation for a remailer value in the JSON config needed
#[derive(Deserialize, Eq, PartialEq, Clone, Debug, Default)]
struct Remailer {
    name: Vec<String>,
    email: String,
    enable: bool,
    key: String,
}

impl Remailer {
    /// Return if this remailer is enable in the config
    fn is_enabled(&self) -> bool {
        self.enable
    }

    /// Return and decode the key of this remailer
    fn into_key(&self) -> Fallible<Vec<u8>> {
        Ok(base64::decode(self.key.split_at(7).1).context(format!(
            "Can't decode the base64-encoded key `{}`!",
            self.name[0]
        ))?)
    }
}

/// Make a map of name-to-remailer from a list of remailers
fn remailer_map(remailers: Vec<Remailer>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if remailers.is_empty() {
        eprintln!("Without any remailer, the program will panic soon...")
    }
    // For each remailers
    for remailer in remailers {
        // We check if it is enabled
        if remailer.enable {
            // For each alias and email of this remailer, we add it to the map
            map.insert(remailer.email.clone(), remailer.email.clone());
            for alias in remailer.name {
                map.insert(alias, remailer.email.clone());
            }
        }
    }
    map
}
