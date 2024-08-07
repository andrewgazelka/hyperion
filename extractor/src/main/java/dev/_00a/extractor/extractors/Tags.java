package dev._00a.extractor.extractors;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.datafixers.util.Pair;

import dev._00a.extractor.Main;
import dev._00a.extractor.RegistryKeyComparator;
import net.minecraft.registry.Registry;
import net.minecraft.registry.SerializableRegistries;
import net.minecraft.registry.entry.RegistryEntry;
import net.minecraft.registry.entry.RegistryEntryList;
import net.minecraft.server.MinecraftServer;
import net.minecraft.util.Identifier;

import java.io.DataOutput;
import java.util.Map;
import java.util.TreeMap;
import java.util.stream.Collectors;

public class Tags implements Main.Extractor {
    public Tags() {
    }

    @Override
    public String fileName() {
        return "tags.json";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws Exception {
        var dynamicRegistryManager = server.getCombinedDynamicRegistries();

        var tagsJson = new JsonObject();

        final var registryTags = SerializableRegistries.streamRegistryManagerEntries(dynamicRegistryManager)
                .map(registry -> Pair.of(registry.key(), serializeTags(registry.value())))
                .filter(pair -> !(pair.getSecond()).isEmpty())
                .collect(Collectors.toMap(Pair::getFirst, Pair::getSecond, (l, r) -> r,
                        () -> new TreeMap<>(new RegistryKeyComparator())));

        for (var registry : registryTags.entrySet()) {
            var registryIdent = registry.getKey().getValue().toString();
            var tagGroupTagsJson = new JsonObject();

            for (var tag : registry.getValue().entrySet()) {
                var ident = tag.getKey().toString();
                var rawIds = tag.getValue();
                tagGroupTagsJson.add(ident, rawIds);
            }

            tagsJson.add(registryIdent, tagGroupTagsJson);
        }

        Main.writeJson(output, gson, tagsJson);
    }

    private static <T> Map<Identifier, JsonArray> serializeTags(Registry<T> registry) {
        TreeMap<Identifier, JsonArray> map = new TreeMap<>();
        registry.streamTagsAndEntries().forEach(pair -> {
            RegistryEntryList<T> registryEntryList = pair.getSecond();
            JsonArray intList = new JsonArray(registryEntryList.size());
            for (RegistryEntry<T> registryEntry : registryEntryList) {
                if (registryEntry.getType() != RegistryEntry.Type.REFERENCE) {
                    throw new IllegalStateException("Can't serialize unregistered value " + registryEntry);
                }
                intList.add(registry.getRawId(registryEntry.value()));
            }
            map.put(pair.getFirst().id(), intList);
        });
        return map;
    }
}
