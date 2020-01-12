use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use failure::{err_msg, Fail, Fallible, ResultExt};
use failure::Error as FError;

use crate::lib::{Cypherpunk, CypherpunkCore, PGPBackend};
#[cfg(feature = "back-gpg")]
use crate::pgp::gpg::GPGBackend;

mod lib;
mod pgp;

fn main() {
    #[cfg(feature = "back-gpg")]
    fn init_pgp_back() -> impl PGPBackend {
        GPGBackend::new(None, false)
    }
    let pgp_back = init_pgp_back();
    let core = CypherpunkCore::new(Box::new(pgp_back));
    let keys = load_keys();
    core.import_keys(keys.unwrap()).map_err(print_errors);
    let encrypted = core.encrypt_message(vec!["mixmaster@remailer.privacy.at".to_string(), "remailer@dizum.com".to_string()], "::\nAnon-To: test@domain.tld\n\nHey!".as_bytes().to_vec());
    encrypted.map(|msg| -> Fallible<()> {
        println!("Final:\n{}", String::from_utf8(msg)?);
        Ok(())
    }).map_err(print_errors);
}

fn load_keys() -> Fallible<Vec<Vec<u8>>> {
    println!("Let's import the keys...");
    let mut keys: Vec<Vec<u8>> = Vec::new();

    // Retrieve the keys path and check it
    let keys_path = Path::new("./remailer-keys/");
    if !keys_path.exists() || !keys_path.is_dir() { // Show error if the `remailer-keys` directory doesn't exist
        return Err(err_msg("The `remailer-keys` directory doesn't exist!"));
    }
    let keys_dir = std::fs::read_dir(keys_path)?; // List the keys presents in the directory
    for entry in keys_dir {
        match entry {
            Ok(entry) => {
                if entry.path().is_dir() { // Ignoring dir
                    println!("Entry `{}` ignored!", entry.path().to_string_lossy());
                } else { // Importing "key" file using the given `import_key` function
                    let mut key_file = File::open(entry.path())?;
                    let mut buffer = Vec::new();
                    key_file.read_to_end(&mut buffer);
                    keys.push(buffer);
                }
            }
            // Ignore invalid DirEntry
            Err(err) => eprintln!("An error occurred on a dir entry, it'll be ignored: {:?}", err),
        }
    }
    return Ok(keys);
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