#[cfg(feature = "back-gpg")]
pub mod gpg {
    use std::env::temp_dir;
    use std::fs::File;
    use std::io;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::process::{Child, Command};

    use failure::{err_msg, Fallible, ResultExt};
    use tempfile;
    use tempfile::tempdir_in;

    use crate::lib::PGPBackend;

    /// A PGP Backend using the command-line gpg
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct GPGBackend {
        temp_dir: PathBuf,
        keyring: PathBuf,
        quiet: bool,
    }

    impl GPGBackend {
        /// Create a new GPGBackend instance
        pub fn new(temp: Option<PathBuf>, quiet: bool) -> Self {
            // Create a keyring file
            let tmp = temp.unwrap_or_else(temp_dir);
            let keyring = match tempdir_in(tmp.clone()) {
                Ok(dir) => dir.into_path().join("keyring.gpg"),
                Err(_err) => {
                    // "Keyring temp dir is unwrittable... Switch to fallback keyring;
                    tmp.clone().join("cypherpunk-cli_keyring.gpg")
                }
            };
            // Return the GPGBackend
            Self {
                quiet,
                temp_dir: tmp,
                keyring,
            }
        }
    }

    impl Default for GPGBackend {
        fn default() -> Self {
            Self::new(None, false)
        }
    }

    impl PGPBackend for GPGBackend {
        fn import_key(&self, key: Vec<u8>) -> Fallible<()> {
            // Retrieve keyring path and quiet flag
            let quiet = if self.quiet { "-q" } else { "" };
            let keyring = &self.keyring;

            // Create a temp dir and file who contain the key
            let tmp_path = tempdir_in(self.temp_dir.clone())
                .context("Cannot create a temporary directory to import the key!")?
                .into_path();

            let key_path: PathBuf = tmp_path.join("key.txt");
            {
                let mut tmp =
                    File::create(key_path.clone()).context("Cannot create key file to import")?;
                tmp.write_all(key.as_slice())
                    .context("Cannot copy key to temporary file")?;
                tmp.flush().context("Cannot save temporary key file")?;
            }

            // Import key from gpg command-line
            let mut child = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .args(&[
                        "/C",
                        format!(
                            "gpg --no-default-keyring --keyring={} --import --yes {} {}",
                            keyring.to_string_lossy(),
                            quiet,
                            key_path.to_string_lossy()
                        )
                        .as_str(),
                    ])
                    .spawn()
                    .context("Failed to execute GPG")?
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(
                        format!(
                            "gpg --no-default-keyring --keyring={} --import --yes {} {}",
                            keyring.to_string_lossy(),
                            quiet,
                            key_path.to_string_lossy()
                        )
                        .as_str(),
                    )
                    .spawn()
                    .context("Failed to execute GPG")?
            };

            // retrieve its exit status
            let exit_state = child.wait().context("GPG unexpected exit")?;

            // Check importation result
            match exit_state.code().unwrap_or(-1099) {
                0 => Ok(()),
                -1099 => Err(err_msg("GPG exited without any exit code!")),
                other => Err(err_msg(format!("GPG exited with code {}\nPlease check the GPG output for more information about the error.", other))),
            }
        }

        fn encrypt(
            &self,
            input: &mut dyn Read,
            output: &mut dyn Write,
            recipients: Vec<String>,
        ) -> Fallible<()> {
            // Retrieve keyring path and quiet flag
            let quiet = if self.quiet { "-q" } else { "" };
            let keyring = &self.keyring;

            // Creat a temp dir for input and output files
            let tmp_path = tempdir_in(self.temp_dir.clone())
                .context("Cannot create a temporary directory to encrypt your message!")?
                .into_path();

            let in_path: PathBuf = tmp_path.clone().join("input.txt");
            {
                let mut tmp = File::create(in_path.clone()).context(
                    "Cannot create an input file (which contains your message) to encrypt it",
                )?;
                io::copy(input, &mut tmp)
                    .context("Cannot copy your message in temporary input file")?;
                tmp.flush().context("Cannot save temporary message file")?;
            }

            let out_path: PathBuf = tmp_path.join("output.txt");
            // Create the recipients args
            let recipients = recipients.join(" -r ");

            // Run encryption from gpg command-line
            let mut child: Child = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .arg("/C")
                    .arg(format!(
                        r#"gpg --no-default-keyring --keyring={} -R "{}" -a -o {} {} --always-trust -e {}"#,
                        keyring.to_string_lossy(),
                        recipients,
                        out_path.to_string_lossy(),
                        quiet,
                        in_path.to_string_lossy()
                    ))
                    .spawn()
                    .context("Failed to execute GPG")?
            } else {
                Command::new("sh")
                    .arg("-c")
                    .arg(format!(
                        r#"gpg --no-default-keyring --keyring={} -R "{}" -a -o {} {} --always-trust -e {}"#,
                        keyring.to_string_lossy(),
                        recipients,
                        out_path.to_string_lossy(),
                        quiet,
                        in_path.to_string_lossy()
                    ))
                    .spawn()
                    .context("Failed to execute GPG")?
            };
            // retrieve its exit status
            let exit_state = child.wait().context("GPG unexpected exit")?;

            // Check encryption result
            match exit_state.code().unwrap_or(9999) {
                0 => { // Success

                    // Open output file and copy data from it to the given output
                    let mut output_file = File::open(&out_path).context("Cannot open your temporary encrypted message")?;
                    io::copy(&mut output_file, output).context("Cannot copy your encrypted message from temp file")?;

                    Ok(())
                }
                // Send error if the execution failed
                9999 => Err(err_msg("GPG exited without any exit code!")),
                other => Err(err_msg(format!("GPG exited with code {}\nPlease check the GPG output for more information about the error.", other))),
            }
        }
    }
}
