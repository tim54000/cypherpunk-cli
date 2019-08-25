use std::fmt::{Display, Error, Formatter};
use std::time::Duration;

use failure::Fallible;
use regex::Regex;
use sequoia::openpgp::TPK;

#[derive(Default, Clone, Debug)]
pub struct Remailer {
    pub name: String,
    pub email_address: String,
    pub options: Vec<String>,
    pub latency: Duration,
    pub keys: Vec<TPK>,
    pub uptime: f32,
}


impl Display for Remailer {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        Display::fmt(&self.name, f)
    }
}

impl Remailer {
    pub fn new(name: String, email_address: String, options: Vec<String>) -> Self {
        Self {
            name,
            email_address,
            options,
            ..Default::default()
        }
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_email(&self) -> &String {
        &self.email_address
    }

    pub fn get_options(&self) -> &Vec<String> {
        &self.options
    }

    pub fn get_keys(&self) -> &Vec<TPK> {
        &self.keys
    }

    pub fn get_latency(&self) -> &Duration {
        &self.latency
    }

    pub fn get_uptime(&self) -> f32 {
        *&self.uptime
    }

    pub fn set_options(&mut self, options: Vec<String>) {
        self.options = options;
    }

    pub fn set_keys(&mut self, keys: Vec<TPK>) {
        self.keys = keys;
    }

    pub fn add_key(&mut self, key: TPK) {
        self.keys.push(key);
    }

    pub fn set_latency(&mut self, duration: Duration) {
        self.latency = duration;
    }

    pub fn set_latency_from(&mut self, latency: String) -> Fallible<()> {
        let regex_latency = r#"^((?P<hour>\d+):)?(?P<minute>[0-5]?\d):(?P<second>[0-5]\d)$"#;
        let regex_latency = Regex::new(regex_latency)?;

        let mut seconds = 0;
        match regex_latency.captures_iter(latency.as_str()).next() {
            Some(capture) => {
                seconds += if capture.name("hour").is_some() { capture.name("hour").unwrap().as_str().parse::<u64>().unwrap() * 3600 } else { 0 };
                seconds += if capture.name("minute").is_some() { capture.name("minute").unwrap().as_str().parse::<u64>().unwrap() * 60 } else { 0 };
                seconds += if capture.name("second").is_some() { capture.name("second").unwrap().as_str().parse::<u64>().unwrap() } else { 0 };
            }
            None => { seconds = 0 }
        }
        self.set_latency(Duration::from_secs(seconds));
        Ok(())
    }


    pub fn set_uptime(&mut self, uptime: f32) {
        self.uptime = uptime;
    }
}