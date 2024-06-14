fn main() {
    let protos = ["src/schema.proto"];

    prost_build::Config::new()
        .compile_protos(&protos, &["src/"])
        .expect("Failed to compile Protobuf files");
}
