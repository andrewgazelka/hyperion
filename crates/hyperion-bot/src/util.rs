use antithesis::random::AntithesisRng;
use rand::Rng;

pub fn random_either<T>(left: impl FnOnce() -> T, right: impl FnOnce() -> T) -> T {
    let mut rng = AntithesisRng;
    if rng.r#gen::<bool>() {
        left()
    } else {
        right()
    }
}
