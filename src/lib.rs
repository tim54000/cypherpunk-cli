use std::io::{Cursor, Read, Write};

use failure::Fallible;
use failure::ResultExt;

/// Representation of a capable Cypherpunk core
pub trait Cypherpunk {
    /// Import the keys given to the PGP backend
    fn import_keys(&self, keys: Vec<Vec<u8>>) -> Fallible<()>;
    /// Encrypt the given message for the given chain with additionnal headers
    fn encrypt_message(&self, chain: &Vec<String>, headers: &Vec<String>, message: Vec<u8>) -> Fallible<Vec<u8>>;
}

/// Representation of a PGP back-end usable by a Cypherpunk-capable core
pub trait PGPBackend {
    /// Import the given key in its keyring
    fn import_key(&self, key: Vec<u8>) -> Fallible<()>;
    /// Encrypt for recipient an input to an output
    fn encrypt(
        &self,
        input: &mut dyn Read,
        output: &mut dyn Write,
        recipients: Vec<String>,
    ) -> Fallible<()>;
}

/// The actual Cypherpunk core associated with a PGPBackend
pub struct CypherpunkCore<P: PGPBackend> {
    pgp: P,
}

impl<P: PGPBackend> CypherpunkCore<P> {
    /// Return a CypherpunkCore with P as PGPBackend
    pub fn new(pgp: P) -> Self {
        Self { pgp }
    }
}

impl<P: PGPBackend + Default> Default for CypherpunkCore<P> {
    fn default() -> Self {
        Self {
            pgp: P::default()
        }
    }
}

impl<P: PGPBackend> Cypherpunk for CypherpunkCore<P> {
    fn import_keys(&self, keys: Vec<Vec<u8>>) -> Fallible<()> {
        // Import each key in the PGP Backend
        for key in keys {
            self.pgp.import_key(key).context("Cannot import the key")?;
        }
        Ok(())
    }

    fn encrypt_message(&self, chain: &Vec<String>, headers: &Vec<String>, message: Vec<u8>) -> Fallible<Vec<u8>> {
        // Encrypt the message throught the remailer chain
        chain.iter().fold(Ok(message), |input, remailer| {
            // Pepare to encryption
            let mut readin = Cursor::new(input?);
            let mut writeout: Cursor<Vec<u8>> = Cursor::new(Vec::new());
            let recipients = vec![remailer.clone()];
            let addheaders : String = headers.join("\n");
            // Make the next message to which add the encrypted body
            let message = format!("\n::\nAnon-To: {}\n{}\n\n::\nEncrypted: PGP\n\n", remailer, addheaders);

            // Encrypt the message for the remailer
            self.pgp.encrypt(&mut readin, &mut writeout, recipients)
                .context("Encryption failed!")?;

            // Format the final message in Cypherpunk format
            let mut output: Vec<u8> = Vec::new();
            // Add the headers
            output
                .write_all(message.as_bytes())
                .context("Cannot add remailer headers to the output")?;
            // Add the encapsulated and now encrypted body
            output
                .write_all(writeout.into_inner().as_slice())
                .context("Cannot format final output message")?;
            Ok(output)
        })
    }
}
