package dev._00a.extractor.extractors;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;

import dev._00a.extractor.Main;
import net.minecraft.server.MinecraftServer;
import net.minecraft.util.Language;

import java.io.DataOutput;
import java.io.IOException;
import java.lang.reflect.Field;
import java.util.Map;

public class TranslationKeys implements Main.Extractor {

    @Override
    public String fileName() {
        return "translation_keys.json";
    }

    @Override
    public void extract(MinecraftServer server, DataOutput output, Gson gson) throws IOException {
        JsonArray translationsJson = new JsonArray();

        Map<String, String> translations = extractTranslations();
        for (var translation : translations.entrySet()) {
            String translationKey = translation.getKey();
            String translationValue = translation.getValue();

            var translationJson = new JsonObject();
            translationJson.addProperty("key", translationKey);
            translationJson.addProperty("english_translation", translationValue);

            translationsJson.add(translationJson);
        }

        Main.writeJson(output, gson, translationsJson);
    }

    @SuppressWarnings("unchecked")
    private static Map<String, String> extractTranslations() {
        Language language = Language.getInstance();

        Class<? extends Language> anonymousClass = language.getClass();
        for (Field field : anonymousClass.getDeclaredFields()) {
            try {
                Object fieldValue = field.get(language);
                if (fieldValue instanceof Map<?, ?>) {
                    return (Map<String, String>) fieldValue;
                }
            } catch (IllegalAccessException e) {
                throw new RuntimeException("Failed reflection on field '" + field + "' on class '" + anonymousClass + "'", e);
            }
        }

        throw new RuntimeException("Did not find anonymous map under 'net.minecraft.util.Language.create()'");
    }
}
