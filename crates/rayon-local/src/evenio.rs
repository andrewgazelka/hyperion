use crate::RayonLocal;

/// Alias for `evenio::fetch::Single` with `RayonLocal`.
pub type RayonLocalSingle<'a, S> = evenio::fetch::Single<'a, RayonLocal<S>>;
