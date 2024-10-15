# Hyperion

### JOIN THE DISCORD

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

[hyperion.webm](https://github.com/user-attachments/assets/5ea4bdec-25a8-4bb5-a670-0cb81bf88d7e)

![many](https://github.com/user-attachments/assets/e69f2c3a-f053-4361-a49d-336894f544ba)

Hyperion aims to have 10k players PvP simultaneously on one Minecraft world to break the Guinness World Record ([8825 by
EVE Online](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle)). The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust). To contribute,
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

Our current efforts are focused on making an event roughly similar to something that would be
on [Overcast Network](https://oc.tc/) (we are not affiliated with them).

| **Category**           | **Task**                                     | **Status**        | **Notes**                                                        |
|------------------------|----------------------------------------------|-------------------|------------------------------------------------------------------|
| **Lighting**           | Pre-loaded lighting                          | ✅ Done            |                                                                  |
|                        | Dynamic lighting updates                     | 📚 Backlogged | Not planning for MVP                       |
| **Block Mechanics**    | Placing blocks                               | ✅ Done | Existed pre-rewrite                                                                 |
|                        | Block breaking                               | ✅ Done            |                                                                  |
|                        | Block drops                                  | ✅ Done |                                                                  |
|                        | Block physics (doors, liquid, torches, etc.) | 📚 Backlogged | Not planning for MVP                                                                 |
| **World Generation**   | Pre-loaded chunks from Java world saves      | ✅ Done            | Uses pre-built maps                                              |
| **Rendering**          | Block animation/Frame API                    | ✅ Done            |                                                                  |
|            | Distant Horizons LoD protocol support                    | 🤞 Not implemented            | Stretch goal for MVP; will not be in open-source repository.                                                                  |
| **Inventory**          | Player inventory                             | ✅ Done | Existed to some extent pre-rewrite                                                                 |
|                        | Block inventory (chests, etc.)               | 🤞 Not implemented | Stretch goal for MVP                                                                  |
| **Combat**             | PvP                       | ✅ Done  |                                                                  |
|                        | Arrows                                       | 🤞 Not implemented | Stretch goal for MVP                                                                 |
|                        | Player health                      | ✅ Done |    |
| **World Persistence**  | Saving world                                 | 📚 Backlogged | Not planning for MVP                     |
|                        | Saving inventory / player state                                 | ⏳ WIP |                      |
| **Physics**            | Entity-block collisions (anti-cheat)         | ✅ Done            |                               |
|                        | Entity-entity collisions                     | ✅ Done            | Required for arrow-based combat                                  |
| **Gameplay Mechanics** | Day/night cycle                              | ✅ Done            |                                                                  |
| **Audio**              | Proximity voice chat (SimpleVoiceChat)                       | ✅ Done            | Not included in open-source repository                           |
| **Modularity**         | Mod/Plugin API                               | ✅ Done    | We want to make events extensions on top of the core game engine |

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

