# Hyperion

### JOIN THE DISCORD

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

<img width="1529" alt="image" src="https://github.com/user-attachments/assets/180391a8-8d82-4d5e-8466-995a962ec643">

https://github.com/user-attachments/assets/b5213cc7-1be4-4241-8cb0-375cd2a8e981



Hyperion aims to have 10k players PvP simultaneously on one Minecraft world to break the Guinness World Record (8825 by
EVE Online). The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust). To contribute,
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

Our current efforts are focused on making an event roughly similar to something that would be
on [Overcast Network](https://oc.tc/) (we are not affiliated with them).

| **Category**           | **Task**                                     | **Status**        | **Notes**                                                        |
|------------------------|----------------------------------------------|-------------------|------------------------------------------------------------------|
| **Lighting**           | Pre-loaded lighting                          | ✅ Done            |                                                                  |
|                        | Dynamic lighting updates                     | ❌ Not implemented | May be unnecessary for Overcast-like modes                       |
| **Block Mechanics**    | Placing blocks                               | ❌ Not implemented |                                                                  |
|                        | Block breaking                               | ✅ Done            |                                                                  |
|                        | Block drops                                  | ❌ Not implemented |                                                                  |
|                        | Block physics (doors, liquid, torches, etc.) | ❌ Not implemented |                                                                  |
| **World Generation**   | Pre-loaded chunks from Java world saves      | ✅ Done            | Uses pre-built maps                                              |
| **Rendering**          | Block animation/Frame API                    | ✅ Done            |                                                                  |
| **Inventory**          | Player inventory                             | ❌ Not implemented |                                                                  |
|                        | Block inventory (chests, etc.)               | ❌ Not implemented |                                                                  |
| **Combat**             | PvP (Player vs. Player)                      | ❌ Not implemented |                                                                  |
|                        | Arrows                                       | ❌ Not implemented |                                                                  |
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

