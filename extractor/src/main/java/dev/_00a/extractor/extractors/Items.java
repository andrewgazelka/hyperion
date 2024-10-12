package dev._00a.extractor.extractors;

import java.io.DataOutput;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import dev._00a.extractor.Main;
import net.minecraft.registry.Registries;
import net.minecraft.server.MinecraftServer;

public class Items implements Main.Extractor {
    public Items() {
    }

    @Override
    public String fileName() {
        return "items.json";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws Exception {
        var itemsJson = new JsonArray();

        for (var item : Registries.ITEM) {
            var itemJson = new JsonObject();
            itemJson.addProperty("id", Registries.ITEM.getRawId(item));
            itemJson.addProperty("name", Registries.ITEM.getId(item).getPath());
            itemJson.addProperty("translation_key", item.getTranslationKey());
            itemJson.addProperty("max_stack", item.getMaxCount());
            itemJson.addProperty("enchantability", item.getEnchantability());

            item.dam

//            var food = item.getComponents().get(DataComponentTypes.FOOD);
//            if (food != null) {
//                var foodJson = new JsonObject();
//
//                foodJson.addProperty("nutrition", food.nutrition());
//                foodJson.addProperty("saturation", food.saturation());
//                foodJson.addProperty("can_always_eat", food.canAlwaysEat());
//                foodJson.addProperty("eat_seconds", food.eatSeconds());
//
//                var effectsJson = new JsonArray();
//
//                for (var effect : food.effects()) {
//                    var effectJson = new JsonObject();
//
//                    effectJson.addProperty("probability", effect.probability());
//                    effectJson.addProperty("translation_key", effect.effect().getTranslationKey());
//
//                    effectsJson.add(effectJson);
//                }
//
//                foodJson.add("effects", effectsJson);
//
//                itemJson.add("food", foodJson);
//            }

            itemsJson.add(itemJson);
        }

        Main.writeJson(output, gson, itemsJson);
    }
}
