package dev._00a.extractor.extractors;

import java.io.DataOutput;
import java.io.IOException;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import dev._00a.extractor.Main;
import net.minecraft.registry.Registries;
import net.minecraft.server.MinecraftServer;

public class Effects implements Main.Extractor {
    public Effects() {
    }

    @Override
    public String fileName() {
        return "effects.json";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws IOException {
        var effectsJson = new JsonArray();

        for (var effect : Registries.STATUS_EFFECT) {
            var effectJson = new JsonObject();

            effectJson.addProperty("id", Registries.STATUS_EFFECT.getRawId(effect));
            effectJson.addProperty("name", Registries.STATUS_EFFECT.getId(effect).getPath());
            effectJson.addProperty("translation_key", effect.getTranslationKey());
            effectJson.addProperty("color", effect.getColor());
            effectJson.addProperty("instant", effect.isInstant());
            effectJson.addProperty("category", Main.toPascalCase(effect.getCategory().name()));

            var attributeModifiersJson = new JsonArray();

            effect.forEachAttributeModifier(0, (attrRegistryEntry, modifier) -> {
                var attributeModifierJson = new JsonObject();

                var attr = attrRegistryEntry.getKeyOrValue().map(k -> Registries.ATTRIBUTE.get(k), v -> v);
                attributeModifierJson.addProperty("attribute_id", Registries.ATTRIBUTE.getRawId(attr));
                attributeModifierJson.addProperty("attribute_name", attr.getTranslationKey());
                attributeModifierJson.addProperty("operation", modifier.operation().getId());
                attributeModifierJson.addProperty("base_value", modifier.value());

                attributeModifiersJson.add(attributeModifierJson);
            });

            if (attributeModifiersJson.size() > 0) {
                effectJson.add("attribute_modifiers", attributeModifiersJson);
            }

            effectsJson.add(effectJson);
        }

        Main.writeJson(output, gson, effectsJson);
    }
}
