// Assuming the build script generates a Rust file named hello.rs in OUT_DIR
include!(concat!(env!("OUT_DIR"), "/generator-output.rs"));

#[cfg(feature = "valence_protocol")]
mod valence;
