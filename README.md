# Hyperion

### JOIN THE DISCORD

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

| Category           | Task                                            | Status            | Notes                              |
|--------------------|-------------------------------------------------|-------------------|------------------------------------|
| Lighting           | Pre-loaded lighting                             | ✅ Done            |                                    |
|                    | Dynamic lighting updates                        | ❌ Not implemented |                                    |
| Block Mechanics    | Placing blocks                                  | ❌ Not implemented |                                    |
|                    | Block physics                                   | ❌ Not implemented | Used to be implemented pre-rewrite |
|                    | - Doors opening/closing                         | ❌ Not implemented | Part of block physics              |
|                    | - Liquid physics                                | ❌ Not implemented | Part of block physics              |
|                    | - Stairs, etc. adjusting position               | ❌ Not implemented | Part of block physics              |
|                    | - Torches being destroyed                       | ❌ Not implemented | Part of block physics              |
|                    | Block breaking                                  | ✅ Done            |                                    |
|                    | Block drops                                     | ❌ Not implemented |                                    |
| World Generation   | Pre-loaded chunks from Java world saves (Anvil) | ✅ Done            |                                    |
|                    | Procedural terrain generation                   | 🔪 Not planned    |                                    |
| Rendering          | Block animation/Frame API                       | ✅ Done            |                                    |
| Inventory          | Player inventory                                | ❌ Not implemented |                                    |
|                    | Block inventory (chests, etc.)                  | ❌ Not implemented |                                    |
|                    | Crafting system                                 | ❌ Not implemented |                                    |
|                    | Item durability                                 | ❌ Not implemented |                                    |
| World Persistence  | Saving world                                    | ❌ Not implemented |                                    |
| Physics            | Entity-entity collisions                        | ❌ Not implemented | Used to exist pre-rewrite          |
|                    | Entity-block collisions (anti-cheat)            | ✅ Done            |                                    |
| Combat             | Arrows                                          | ❌ Not implemented | Used to exist pre-rewrite          |
|                    | PvP                                             | ❌ Not implemented | Used to exist pre-rewrite          |
|                    | Mob AI and pathfinding                          | 🔪 Not planned    |                                    |
|                    | Player health and hunger                        | ❌ Not implemented | Used to exist pre-rewrite          |
| Audio              | Proximity voice chat with Simple Voice Chat     | ✅ Done            | Not in open source repo            |
| Gameplay Mechanics | Day/night cycle                                 | ✅ Done            |                                    |
|                    | Player experience and leveling                  | ❌ Not implemented |                                    |
|                    | Enchanting system                               | ❌ Not implemented |                                    |
|                    | Farming and crop growth                         | 🔪 Not planned    |                                    |
| Modding Support    | Mod/Plugin API                                  | 🌊 In progress    |                                    |
|                    | Resource pack support                           | ❌ Not implemented |                                    |

![2024-07-11_15 37 33](https://github.com/user-attachments/assets/1d058da7-52fa-49e1-9d1e-4c368f3d623f)

Hyperion aims to have 10k players PvP simultaneously on a Minecraft server to break the Guinness World Record. The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust). To contribute,
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

# Running

## Step 1: The proxy

Go to `hyperion-proxy` and install it with `cargo install --path .`

## Step 2: The event (development)

```bash
brew install just
just debug
```

# Local CI

```
just
```

# Development

## Recommendations

- Wurst client
    - great for debugging and also rejoining with running `just debug`. I usually have an AutoReconnect time of 0
      seconds.
- Supermaven. great code completion.


