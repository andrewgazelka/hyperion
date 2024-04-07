pub fn bytes() -> &'static [u8] {
    include_bytes!("config.toml")
}
