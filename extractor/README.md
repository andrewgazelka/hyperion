# Extractor

This is a fabric mod that extracts data from Minecraft and stores it in JSON files for use elsewhere.

## How to use

Run `just extract`. This will compile and briefly run a Minecraft server. Generated JSON is written to `../extracted`.

You'll need a Java environment set up to do this.

## Contributing

Run `./gradlew genSources` to generate Minecraft Java source files for your IDE.

## Updating to a new Minecraft version

The process goes something like this:

- Update `gradle.properties` to the new version of Minecraft using https://fabricmc.net/develop.
- Update `src/main/resources/fabric.mod.json` to reference the new version of Minecraft.
- Make sure `fabric-loom` is up to date in `build.gradle`.
- Attempt to run `just extract` and fix any errors that come up.

After running `./gradlew genSources` for the new MC version you might still have some old sources around, confusing your IDE.
Try deleting caches in `.gradle`, `bin`, `build`, `run`, `~/.gradle/caches/fabric-loom/`, and then reloading your java environment.
