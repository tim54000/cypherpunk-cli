#[cfg(feature = "back-sequoia", )]
pub mod sequoia {
    use std::fs::File;
    use std::io::{Read, Write};
    use std::io;
    use std::path::PathBuf;

    use failure::{Fail, Fallible};
    use sequoia::core::Context;
    use sequoia::openpgp::{armor, TPK};
    use sequoia::openpgp::constants::DataFormat;
    use sequoia::openpgp::crypto::Password;
    use sequoia::openpgp::parse::Parse;
    use sequoia::openpgp::serialize::stream::{EncryptionMode, Encryptor, LiteralWriter, Message};
    use sequoia::store::Store;

    /// Encrypt a given input to a given output using the given recipients with their key present in the given store
    pub fn encrypt(store: &mut Store, input: &mut dyn Read, output: &mut dyn Write, recipients: Vec<&str>) -> Fallible<()> {
        let mut tpks = Vec::new(); // Public key collector (contains all needed key for the encryption)
        for r in recipients {
            tpks.push(store.lookup(r)?.tpk()?) // Push public key of recipients
        }
        let recipients: Vec<&TPK> = tpks.iter().collect(); // Get a vector of reference

        // Init an Armored output
        let output = armor::Writer::new(io::stdout(), armor::Kind::Message, &[])?;

        // Wrap output in a Message
        let message = Message::new(output);

        // Create a sink linked to the Armored output [All data that enters, is encrypted at the output]
        let mut sink = Encryptor::new(message,
                                      &[],
                                      &recipients,
                                      EncryptionMode::ForTransport,
                                      None)?;

        // Wrap the sink in a LiteralWriter, it write the translated message packet needed by SequoiaPGP
        let mut literal_writer = LiteralWriter::new(sink, DataFormat::Binary,
                                                    None, None)?;

        io::copy(input, &mut literal_writer)?; // Write all input to SequoiaPGP

        literal_writer.finalize()?; // Close the writer

        Ok(())
    }

    /// Import a key from its path into the store
    pub fn import_key(key_path: PathBuf) -> Fallible<()> {
        let store = Store::open(&Context::new()?, crate::REALM_REMAILER, "remailer")?; // Open `remailer` store

        let key = TPK::from_file(&key_path) // Load all public keys located in the dir
            .map_err(|err| err.context(format!("Failed to load key from file {:?}", &key_path.to_string_lossy())))?;

        for user in key.userids() {
            match user.userid().address_normalized()? {
                // Import them if it contains a valid email address, otherwise ignore it
                Some(email) => {
                    println!("Key for `{}` imported", email);
                    store.import(&email, &key)?;
                }
                None => {
                    println!("Key ignored! {:?}", user.userid());
                }
                _ => {
                    eprintln!("Key userid is not a valid Option value!");
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "back-gpgme", )]
pub mod gpgme {
    use std::fs::File;
    use std::io::{Cursor, Read, Write};
    use std::io;
    use std::path::PathBuf;

    use failure::{err_msg, Fail, Fallible};
    use gpgme::{Context, Data, data};

    /// Encrypt a given input to a given output using the given recipients with their key present in the GPGME keyring (linked by the given context)
    pub fn encrypt(ctx: &mut Context, input: &mut dyn Read, output: &mut dyn Write, recipients: Vec<&str>) -> Fallible<()> {
        ctx.set_armor(true); // Enable the armored output

        // Collect the public keys of recipients who can encrypt
        let keys = if !recipients.is_empty() {
            ctx.find_keys(recipients)?
                .filter_map(|x| x.ok())
                .filter(|k| k.can_encrypt())
                .collect()
        } else {
            Vec::new()
        };

        // Create the input and output
        let mut gpg_input: Vec<u8> = Vec::new();
        let mut gpg_output: Vec<u8> = Vec::new();

        // Copy data message from the given `input` to the new gpgme reserved input `gpg_input`
        io::copy(input, &mut gpg_input)?;

        // Encrypt the given message from `gpg_input` to an armored output `gpg_output`
        ctx.encrypt(&keys, &mut gpg_input, &mut gpg_output)?;

        // Copy the message from gpg_output to the given `output`
        io::copy(&mut Cursor::new(gpg_output), output)?;

        Ok(())
    }

    /// Import a key from its path into the Context's keyring
    pub fn import_key(ctx: &mut Context, key_path: PathBuf) -> Fallible<()> {
        let file = File::open(key_path)?; // Open key file

        // Stream the armored key into a GPGME-readable object
        let mut data = Data::from_seekable_stream(file)?;
        data.set_encoding(data::Encoding::Armor);

        // Import the key
        ctx.import(&mut data)
            .map_err(|e| err_msg(format!("import failed {:?}", e)))?;
        Ok(())
    }
}

#[cfg(feature = "back-gpg", )]
pub mod gpg {
    use std::{fs, io, iter};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::process::{Child, Command};

    use failure::{err_msg, Fallible};
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    const GPG_OUTPUT_START: &'static str = "======== GPG STDOUT ========";
    const GPG_OUTPUT_END: &'static str = "====== END GPG STDOUT ======";

    /// Encrypt a given input to a given output using the given recipients with their key present in the user gpg keyring
    pub fn encrypt(input: &mut dyn Read, output: &mut dyn Write, recipients: Vec<&str>, quiet: bool) -> Fallible<()> {
        let mut rng = rand::thread_rng();

        // Create/Open a temp folder
        let mut input_path = PathBuf::from("./tmp/");
        if !input_path.exists() {
            fs::create_dir(&input_path)?;
        }

        // Create a random ID for the message
        let id: String = iter::repeat(())
            .map(|()| rng.sample(Alphanumeric))
            .take(7)
            .collect();

        // Create an input path
        input_path.push(format!("cypherpunk-{}.txt", id));

        // Create an output path
        let mut output_path = input_path.clone();
        output_path.set_extension("txt.asc");

        // Create the input file and write data from the given input to the temp file
        let mut file = File::create(&input_path)?;
        io::copy(input, &mut file)?;

        // Get the recipients in a String
        let recipients = recipients.iter().fold(String::new(), |str, recipient| str + " -r " + recipient);

        // Get the value of the quiet option
        let q_option = if quiet { "-q" } else { "" };

        if quiet { println!("{}", GPG_OUTPUT_START); }

        // Run encryption from gpg command-line
        let mut child: Child = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .arg("/C")
                .arg(format!(r#"gpg {}{} -a -o {} -e {}"#, recipients, q_option, output_path.to_string_lossy(), input_path.to_string_lossy()))
                .spawn()
                .expect("failed to execute process")
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(format!(r#"gpg {}{} -a -o {} -e {}"#, recipients, q_option, output_path.to_string_lossy(), input_path.to_string_lossy()))
                .spawn()
                .expect("failed to execute process")
        };

        let exit_state = child.wait()?;
        if quiet { println!("{}", GPG_OUTPUT_END); }

        // Check encryption result
        match exit_state.code().unwrap_or(9999) {
            0 => { // Success

                // Open output file and copy data from it to the given output
                let mut output_file = File::open(&output_path)?;
                io::copy(&mut output_file, output)?;

                Ok(())
            }
            // Send error if the execution failed
            9999 => Err(err_msg("GPG exited without any exit code!")),
            other => Err(err_msg(format!("GPG exited with code {}", other))),
        }
    }

    /// Import a key from its path into the user GPG's keyring
    pub fn import_key(key_path: PathBuf) -> Fallible<()> {
        println!("{}", GPG_OUTPUT_START);

        // Import key from gpg command-line
        let mut child = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(&["/C", format!("gpg --import {}", key_path.to_string_lossy()).as_str()])
                .spawn()
                .expect("failed to execute process")
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(format!("gpg --import {}", key_path.to_string_lossy()).as_str())
                .spawn()
                .expect("failed to execute process")
        };

        let exit_state = child.wait()?;
        println!("{}", GPG_OUTPUT_END);

        // Check importation result
        match exit_state.code().unwrap_or(9999) {
            0 => Ok(()),
            9999 => Err(err_msg("GPG exited without any exit code!")),
            other => Err(err_msg(format!("GPG exited with code {}", other))),
        }
    }
}