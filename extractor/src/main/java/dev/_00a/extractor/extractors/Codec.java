package dev._00a.extractor.extractors;

import java.io.DataOutput;

import com.google.gson.Gson;

import dev._00a.extractor.Main.Extractor;
import io.netty.handler.codec.EncoderException;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.registry.DynamicRegistryManager;
import net.minecraft.registry.Registries;
import net.minecraft.registry.RegistryOps;
import net.minecraft.registry.SerializableRegistries;
import net.minecraft.server.MinecraftServer;
import net.minecraft.util.Util;

public class Codec implements Extractor {

    @Override
    public String fileName() {
        return "registry_codec.dat";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws Exception {
        // TODO: Fix me
        /*
        var registryOps = RegistryOps.of(NbtOps.INSTANCE, DynamicRegistryManager.of(Registries.REGISTRIES));
        var registryManager = server.getRegistryManager();
        // var codec = SerializableRegistries.CODEC;

        var nbtElement = Util.getResult(codec.encodeStart(registryOps, registryManager), (error) -> new EncoderException("Failed to encode: " + error + " " + registryManager));

        NbtIo.write(nbtElement, output);
        */
    }
}
