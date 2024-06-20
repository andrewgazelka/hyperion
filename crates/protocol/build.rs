fn main() {
    let protos = ["src/proxy_to_server.proto", "src/server_to_proxy.proto"];

    let bytes = [
        "PlayerPackets.data",
        "BroadcastGlobal.data",
        "BroadcastLocal.data",
        "Multicast.data",
        "Unicast.data",
    ];

    prost_build::Config::new()
        .bytes(bytes)
        .compile_protos(&protos, &["src/"])
        .expect("Failed to compile Protobuf files");
}
