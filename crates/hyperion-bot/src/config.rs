use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub max_number_of_bots: usize,
    pub host: String,
}
