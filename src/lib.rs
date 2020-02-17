use std::io::{Cursor, Read, Write};

use failure::Fallible;
use failure::ResultExt;

pub trait Cypherpunk {
    fn import_keys(&self, keys: Vec<Vec<u8>>) -> Fallible<()>;
    fn encrypt_message(&self, chain: Vec<String>, message: Vec<u8>) -> Fallible<Vec<u8>>;
}

pub trait PGPBackend {
    fn import_key(&self, key: Vec<u8>) -> Fallible<()>;
    fn encrypt(&self, input: &mut dyn Read, output: &mut dyn Write, recipients: Vec<String>) -> Fallible<()>;
}

pub struct CypherpunkCore {
    pgp: Box<dyn PGPBackend>
}

impl CypherpunkCore {
    pub fn new(pgp: Box<dyn PGPBackend>) -> Self {
        Self {
            pgp
        }
    }
}

impl Cypherpunk for CypherpunkCore {
    fn import_keys(&self, keys: Vec<Vec<u8>>) -> Fallible<()> {
        let pgp = self.pgp.as_ref();
        for key in keys {
            pgp.import_key(key).context("Cannot import the key")?;
        }
        Ok(())
    }

    fn encrypt_message(&self, chain: Vec<String>, message: Vec<u8>) -> Fallible<Vec<u8>> {
        let pgp = self.pgp.as_ref();
        return chain.iter().fold(Ok(message), |input, remailer| {
            let recipients = vec![remailer.clone()];
            let mut readin = Cursor::new(input?);

            let headers = format!("\n::\nAnon-To: {}\n\n::\nEncrypted: PGP\n\n", remailer);
            let mut writeout: Cursor<Vec<u8>> = Cursor::new(Vec::new());

            pgp.encrypt(&mut readin, &mut writeout, recipients).context("Encryption failed!")?;

            let mut output: Vec<u8> = Vec::new();
            output.write_all(headers.as_bytes()).context("Cannot add remailer headers to the output")?;
            output.write_all(writeout.into_inner().as_slice()).context("Cannot format final output message")?;
            Ok(output)
        });
    }
}