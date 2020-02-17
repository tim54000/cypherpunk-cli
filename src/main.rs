use std::fs::{create_dir_all, File};
use std::io::{Read, stdin, Write};
use std::path::{Path, PathBuf};

use clap::arg_enum;
use failure::{err_msg, Fallible, ResultExt};
use failure::Error as FError;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_derive::Deserialize;
use structopt::StructOpt;

use crate::lib::{Cypherpunk, CypherpunkCore, PGPBackend};
#[cfg(feature = "back-gpg")]
use crate::pgp::gpg::GPGBackend;

mod lib;
mod pgp;

// TODO:
// - Make comments
// - Delete warnings =)))

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
    fn extension(&self) -> &'static str {
        match self {
            OutputFormat::EML => "eml",
            _ => "txt"
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "cypherpunk", about = "CLI tool to encrypt your messages between different remailers easily")]
struct Opt {
    /// Input file, stdin if not present
    #[structopt(short, long, parse(from_os_str))]
    input: Option<PathBuf>,

    /// Output dir, stdout if not present
    #[structopt(short, long, parse(from_os_str))]
    output: Option<PathBuf>,

    #[structopt(short, long, default_value = "1")]
    redundancy: u8,

    #[structopt(short, long)]
    chain: Vec<String>,

    #[structopt(short, long, possible_values = & OutputFormat::variants(), case_insensitive = true, default_value = "cypherpunk")]
    format: OutputFormat,

    #[structopt(short, long)]
    quiet: bool,
}


fn main() {
    // To init a PGP Back, here for the GPG one.
    #[cfg(feature = "back-gpg")]
    fn init_pgp_back(quiet: bool) -> impl PGPBackend {
        GPGBackend::new(None, quiet)
    }

    println!("Hello!");
    println!("Config loading...");

    // Load config and run all
    load_config("./remailers.json").and_then(|config| {
        let opts: Opt = Opt::from_args();

        let remailers: Vec<String> = config.remailers.iter().filter(|chain| chain.is_named(opts.chain.clone())).map(|r| r.email.clone()).collect();
        if remailers.is_empty() {
            eprintln!("No chain selected, the program will exit...");
            println!("usage: To select a remailer chain, use `-c <remailer>`");
            Err(err_msg("No chain selected"))?
        }

        // Init infra (the PGP backend)
        let pgp_back = init_pgp_back(opts.quiet);
        // Init the domain (the CypherpunkCore)
        let core = CypherpunkCore::new(Box::new(pgp_back));

        println!("Importing remailers' key...");

        // Retrieve remailers' keys from config
        let keys: Vec<Vec<u8>> = config.remailers.iter().filter_map(|remailer| {
            match remailer.is_enabled() {
                true => remailer.into_key().ok(),
                false => None,
            }
        }).collect();

        // Import keys in the Cypherpunk Core
        core.import_keys(keys)?;

        // Encrypt the mail
        let red = 0..opts.redundancy;

        let mut message: Vec<u8> = Vec::new();
        match &opts.input {
            Some(path) => {
                println!("Retrieving message from file...");
                let mut file = File::open(path)?;
                file.read_to_end(&mut message)?;
            }
            None => {
                println!("\nType your message:");
                stdin().lock().read_to_end(&mut message)?;
                println!();
            }
        };

        if let Some(path) = opts.output.clone() {
            create_dir_all(path)?;
        }

        println!("Encrypting...");

        red.map(|index| {
            println!("Encrypting message n°{}...", index + 1);
            Ok(core.encrypt_message(remailers.clone(), message.clone())?)
        }).enumerate().map(|(index, res): (_, Fallible<Vec<u8>>)| -> Fallible<()> {
            match res {
                Ok(msg) => {
                    if let Ok(msg) = String::from_utf8(msg) {
                        let msg = format_msg(&opts.format, msg)?;
                        match opts.output.clone() {
                            Some(mut path) => {
                                path.push(format!("redundancy_{}.{}", index + 1, &opts.format.extension()).as_str());
                                let mut file = File::create(path.clone())?;
                                file.write_all(msg.as_bytes())?;
                                println!("Encrypted message n°{} in {}", index + 1, path.to_string_lossy())
                            }
                            None => println!("Encrypted message n°{}:\n{}", index + 1, msg)
                        }
                    } else {
                        Err(err_msg("Internal Error, encrypted message is not a valid utf-8 string."))?;
                    }
                }
                err => {
                    err.context(format!("Message n°{}: Ignored, error occured before formatting!", index + 1).to_string())?;
                }
            }
            Ok(())
        }).collect::<Fallible<Vec<()>>>()?;
        Ok(())
    }).unwrap_or_else(print_errors);
}

fn format_msg(format: &OutputFormat, msg: String) -> Fallible<String> {
    match format {
        &OutputFormat::Cypherpunk => Ok(msg),
        &OutputFormat::Mailto => Ok(format_mailto(msg)?),
        &OutputFormat::EML => Ok(format_eml(msg)?),
        other => Err(err_msg(format!("Format {:?} not yet implemented!", other).to_string()))
    }
}

fn format_eml(message: String) -> Fallible<String> {
    let (addr, message) = format_helper(message)?;

    Ok(format!("MIME-Version: 1.0\n\
    Content-Type: text/plain; charset=utf-8\n\
    To: {}\n\
    \n\
    {}", addr, message).to_string())
}

fn format_mailto(message: String) -> Fallible<String> {
    let (addr, message) = format_helper(message)?;

    let body = utf8_percent_encode(message.as_str(), NON_ALPHANUMERIC).to_string(); // Encode message to UTF-8 Percent Encoding
    Ok(format!("mailto:{}?body={}", addr, body).to_string())
}

fn format_helper(mut message: String) -> Fallible<(String, String)> {
    let addr_start = message.find(": ").ok_or(err_msg("Invalid Cypherpunk message"))?; // Find the address start
    message.drain(..addr_start + 2); // Drop all chars before email address
    let addr_end = message.find("\n\n").ok_or(err_msg("Invalid Cypherpunk message"))?; // Find the address end
    let addr: String = message.drain(..addr_end).collect(); // Save the email address in the var, and drop all chars before message
    message.drain(..1);
    Ok((addr, message))
}

fn load_config<P: AsRef<Path>>(path: P) -> Fallible<RemailerConfig> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}

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

#[derive(Deserialize, Eq, PartialEq, Clone, Debug, Default)]
struct RemailerConfig {
    version: String,
    author: String,
    remailers: Vec<Remailer>,
}

#[derive(Deserialize, Eq, PartialEq, Clone, Debug, Default)]
struct Remailer {
    name: Vec<String>,
    email: String,
    enable: bool,
    key: String,
}

impl Remailer {
    fn is_named(&self, names: Vec<String>) -> bool {
        let mut found = false;

        for name in names {
            if found { break; }
            found = self.email == name || self.name.contains(&name);
        }

        found
    }

    fn is_enabled(&self) -> bool {
        self.enable
    }

    fn into_key(&self) -> Fallible<Vec<u8>> {
        Ok(base64::decode(self.key.split_at(7).1).context(format!("Can't decode the base64-encoded key `{}`!", self.name[0]))?)
    }
}