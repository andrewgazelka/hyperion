# Hyperion

# 10k player PvP

### JOIN THE DISCORD

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

[hyperion.webm](https://github.com/user-attachments/assets/5ea4bdec-25a8-4bb5-a670-0cb81bf88d7e)

![many](https://github.com/user-attachments/assets/e69f2c3a-f053-4361-a49d-336894f544ba)

Hyperion aims to have 10k players PvP simultaneously on one Minecraft world to break the Guinness World Record ([8825 by
EVE Online](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle)). The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust). To contribute,
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

Our event will be placed in NYC with the gracious support of [Build the Earth NYC](https://buildtheearth.net/teams/nyc).

# Running

## Debug mode

```bash
brew install just
just
```

## Release mode

```
brew install just
just release
```

# Feature Support Matrix

Feel free to PR if something is missing/incorrect.

| Feature                                                                              | Hyperion | Pumpkin | FerrumC |
|--------------------------------------------------------------------------------------|----------|---------|---------|
| Loading Java Worlds                                                                  | ✅        | ✅       | ✅       |
| Plugin API                                                                           | ✅        | ✅       | ❌       |
| Has been tested with thousands of player connections                                 | ✅        | ❌       | ❌       |
| Proximity Voice ([Simple Voice Chat](https://modrinth.com/plugin/simple-voice-chat)) | ✅        | ❌       | ❌       |
| Lighting                                                                             | ✅        | ❌       | ✅       |
| Placing blocks                                                                       | ✅        | ❌       | ❌       |
| Breaking blocks                                                                      | ✅        | ❌       | ❌       |
| Blocks physics                                                                       | ✅        | ❌       | ❌       |
| Entity-entity collisions                                                             | ✅        | ❌       | ❌       |
| Block-entity collisions                                                              | ✅        | ❌       | ❌       |
| World borders                                                                        | ✅        | ❌       | ❌       |
| Block Edit API (think WorldEdit)                                                     | ✅        | ❌       | ❌       |
| PvP                                                                                  | ✅        | ❌       | ❌       |
| Vertical scaling (fully multi-threaded)                                              | ✅        | ❌       | ❌       |
| Horizontal scaling (through proxies)                                                 | ✅        | ❌       | ❌       | 
| Tracing/profiling through [tracy](https://github.com/wolfpld/tracy))                 | ✅        | ❌       | ❌       | 
| [Flecs ECS](https://github.com/SanderMertens/flecs/tree/master)                      | ✅        | ❌       | ❌       |
| Set Resource Packets                                                                 | ❌        | ✅       | ?       |
| Configuration                                                                        | ✅        | ✅       | ✅       |
| Minecraft 1.20.1                                                                     | ✅        | ✅       | ✅       |
| Proxy Support (Velocity)                                                             | ✅        | ✅       | ?       |
| Inventory                                                                            | ✅        | ✅       | ?       |
| Particle Support                                                                     | ✅        | ✅       | ?       |
| RCON                                                                                 | ❌        | ✅       | ❌       |
| Chat Support                                                                         | ❌        | ✅       | ?       |
| Command Support                                                                      | ✅        | ✅       | ?       |
| Particle Support                                                                     | ✅        | ✅       | ?       |



