package dev._00a.extractor.extractors;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import dev._00a.extractor.Main;
import io.netty.buffer.ByteBuf;
import net.minecraft.network.NetworkPhase;
import net.minecraft.network.NetworkSide;
import net.minecraft.network.NetworkState;
import net.minecraft.network.PacketByteBuf;
import net.minecraft.network.listener.PacketListener;
import net.minecraft.network.state.ConfigurationStates;
import net.minecraft.network.state.HandshakeStates;
import net.minecraft.network.state.LoginStates;
import net.minecraft.network.state.PlayStateFactories;
import net.minecraft.network.state.QueryStates;
import net.minecraft.server.MinecraftServer;

import java.io.DataOutput;
import java.io.IOException;

public class Packets implements Main.Extractor {
    @Override
    public String fileName() {
        return "packets.json";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws IOException {
        var packetsJson = new JsonArray();

        serializeFactory(HandshakeStates.C2S_FACTORY, packetsJson);
        serializeFactory(QueryStates.C2S_FACTORY, packetsJson);
        serializeFactory(QueryStates.S2C_FACTORY, packetsJson);
        serializeFactory(LoginStates.C2S_FACTORY, packetsJson);
        serializeFactory(LoginStates.S2C_FACTORY, packetsJson);
        serializeFactory(ConfigurationStates.C2S_FACTORY, packetsJson);
        serializeFactory(ConfigurationStates.S2C_FACTORY, packetsJson);
        serializeFactory(PlayStateFactories.C2S, packetsJson);
        serializeFactory(PlayStateFactories.S2C, packetsJson);

        Main.writeJson(output, gson, packetsJson);
    }

    private static <T extends PacketListener, B extends ByteBuf> void serializeFactory(NetworkState.Factory<T, B> factory, JsonArray json) {
        factory.forEachPacketType((type, i) -> {
            var packetJson = new JsonObject();
            packetJson.addProperty("name", type.id().getPath());
            packetJson.addProperty("phase", factory.phase().getId());
            packetJson.addProperty("side", factory.side().getName());
            packetJson.addProperty("id", i);
            json.add(packetJson);
        });
    }
}
