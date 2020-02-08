use std::fs::{File, read};
use std::io::Read;
use std::path::{Path, PathBuf};

use failure::{err_msg, Fail, Fallible, ResultExt};
use failure::Error as FError;
use serde_derive::{Deserialize, Serialize};

use crate::lib::{Cypherpunk, CypherpunkCore, PGPBackend};
#[cfg(feature = "back-gpg")]
use crate::pgp::gpg::GPGBackend;

mod lib;
mod pgp;

/// TODO:
/// - Make comments
/// - Make the structopt to make a real CLI program
/// - Add the other output format (EML, Mailto, clear, ...)


fn main() {
    // To init a PGP Back, here for the GPG one.
    #[cfg(feature = "back-gpg")]
    fn init_pgp_back() -> impl PGPBackend {
        GPGBackend::new(None, false)
    }

    // Load config and run all
    load_config("./remailers.json").and_then(|config| {

        // Init infra (the PGP backend)
        let pgp_back = init_pgp_back();
        // Init the domain (the CypherpunkCore)
        let core = CypherpunkCore::new(Box::new(pgp_back));

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
        let encrypted = core.encrypt_message(vec!["mixmaster@remailer.privacy.at".to_string(), "remailer@dizum.com".to_string()], "::\nAnon-To: test@domain.tld\n\nHey!".as_bytes().to_vec());

        // Print the mail if itis build with success
        encrypted.map(|msg| -> Fallible<()> {
            println!("Final:\n{}", String::from_utf8(msg)?);
            Ok(())
        })?;

        Ok(())
    }).map_err(print_errors);
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
    fn is_enabled(&self) -> bool {
        self.enable
    }

    fn into_key(&self) -> Fallible<Vec<u8>> {
        Ok(base64::decode(self.key.split_at(7).1).context(format!("Can't decode the base64-encoded key `{}`!", self.name[0]))?)
    }
}