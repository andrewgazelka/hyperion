use std::sync::LazyLock;

use antithesis::random::AntithesisRng;
use rand::Rng;

#[derive(Clone, Debug)]
pub struct Name {
    pub value: String,
    pub is_valid: bool,
}

pub fn generate() -> Name {
    static NAME_REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9_]+$").unwrap());

    let mut rng = AntithesisRng;

    let len = rng.gen_range(0..20);
    let name: String = (0..len).map(|_| rng.r#gen::<char>()).collect();

    // name is max 16 characters

    let is_valid = name.len() <= 16 && { NAME_REGEX.is_match(&name) };

    Name {
        value: name,
        is_valid,
    }
}
