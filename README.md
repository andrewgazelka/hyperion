# Hyperion

### JOIN THE DISCORD

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

[hyperion.webm](https://github.com/user-attachments/assets/5ea4bdec-25a8-4bb5-a670-0cb81bf88d7e)

Hyperion aims to have 10k players PvP simultaneously on one Minecraft world to break the Guinness World Record ([8825 by
EVE Online](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle)). The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust). To contribute,
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

Our current efforts are focused on making an event roughly similar to something that would be
on [Overcast Network](https://oc.tc/) (we are not affiliated with them).

| **Category**           | **Task**                                     | **Status**        | **Notes**                                                        |
|------------------------|----------------------------------------------|-------------------|------------------------------------------------------------------|
| **Lighting**           | Pre-loaded lighting                          | ✅ Done            |                                                                  |
|                        | Dynamic lighting updates                     | ❌ Not implemented | May be unnecessary for Overcast-like modes                       |
| **Block Mechanics**    | Placing blocks                               | ❌ Not implemented | Existed pre-rewrite                                                                 |
|                        | Block breaking                               | ✅ Done            |                                                                  |
|                        | Block drops                                  | ❌ Not implemented | Existed to some extent pre-rewrite                                                                 |
|                        | Block physics (doors, liquid, torches, etc.) | ❌ Not implemented | Existed pre-rewrite                                                                 |
| **World Generation**   | Pre-loaded chunks from Java world saves      | ✅ Done            | Uses pre-built maps                                              |
| **Rendering**          | Block animation/Frame API                    | ✅ Done            |                                                                  |
| **Inventory**          | Player inventory                             | ❌ Not implemented | Existed to some extent pre-rewrite                                                                 |
|                        | Block inventory (chests, etc.)               | ❌ Not implemented |                                                                  |
| **Combat**             | PvP (Player vs. Player)                      | ❌ Not implemented | Existed pre-rewrite                                                                 |
|                        | Arrows                                       | ❌ Not implemented | Existed to some extent pre-rewrite                                                                 |
|                        | Player health and hunger                     | ❌ Not implemented | Health is necessary; hunger less important                       |
| **World Persistence**  | Saving world                                 | ❌ Not implemented | Most useful in case the event server crashes                     |
| **Physics**            | Entity-block collisions (anti-cheat)         | ✅ Done            | Required for arrow-based combat                                  |
| **Gameplay Mechanics** | Day/night cycle                              | ✅ Done            |                                                                  |
| **Audio**              | Proximity voice chat                         | ✅ Done            | Not included in open-source repository                           |
| **Modularity**         | Mod/Plugin API                               | 🌊 In progress    | We want to make events extensions on top of the core game engine |

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

