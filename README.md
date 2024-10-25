# Hyperion

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)

Hyperion is a **Minecraft game engine** that aims to enable a 10k player PvP battle to break the Guinness World
Record ([8825 by
EVE Online](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle)). The
architecture is ECS-driven using [Flecs Rust](https://github.com/Indra-db/Flecs-Rust).

I would greatly appreciate the contribution.
To see what to work on check the [issues page](https://github.com/andrewgazelka/hyperion/issues) or
join [Hyperion's Discord](https://discord.gg/sTN8mdRQ) for the latest updates on development.

![2024-10-21_19 00 03](https://github.com/user-attachments/assets/5371c38f-5c56-4654-98d9-8d93f75ae2e0)



# Benchmarks

| Players | Tick Time (ms) | Core Usage (%) | Total CPU Utilization (%) |
|---------|----------------|----------------|---------------------------|
| 1       | 0.24           | 4.3            | 0.31                      |
| 10      | 0.30           | 10.3           | 0.74                      |
| 100     | 0.46           | 10.7           | 0.76                      |
| 1000    | 0.40           | 15.3           | 1.09                      |
| 5000    | 1.42           | 35.6           | 2.54                      |
| 10000   | 12.39*         | 100-200        |                           |

*= with UNIX sockets, not TCP sockets. Once I get better tests, I will fill in core usage and CPU utilization.

**Test Environment:**

- Machine: 2023 MacBook Pro Max 16" (14-cores)
- Chunk Render Distance: 32 (4225 total)
- Commit hash `faac9117` run with `just release`
- Bot Launch Command: `just bots {number}`

**Note on Performance:**
Most of the computational cost is fixed due to synchronization with all threads. A couple of $$O(1)$$ cost sync points (
with respect to player count) during each game tick. This explains why performance is not strongly correlated with the
number of players. The overhead of thread synchronization dominates the performance profile, resulting in relatively
stable tick times even as the player count increases significantly.

The primary burden relies on our proxy that can be horizontally scaled. A lot of logic including regional multicasting
is done in the proxy.

![image](https://github.com/user-attachments/assets/92448a00-43e3-4be6-ba52-1e348b3c7e49)

# Running

## Debug mode

```bash
docker compose -f docker-compose.debug.yml up --build
```

## Release mode

```bash
docker compose -f docker-compose.release.yml up --build
```

# Feature Support Matrix

This list is not comprehensive. Feel free to PR or file an issue if something is missing/incorrect.

| Feature                                                                              | Hyperion                                      | Pumpkin             | FerrumC             | Valence     | Minestom*        |
|--------------------------------------------------------------------------------------|-----------------------------------------------|---------------------|---------------------|-------------|------------------|
| Language                                                                             | Rust                                          | Rust                | Rust                | Rust        | Java             |
| Goal                                                                                 | game engine for massive events                | 1:1 vanilla re-impl | 1:1 vanilla re-impl | game engine | game engine      |
| Structure                                                                            | flecs ECS                                     | custom ECS          | custom ECS          | bevy ECS    | non-ECS paradigm |
| Major Dependencies                                                                   | valence                                       |                     |                     |             |                  |
| Can handle 10k players                                                               | ✅                                             | ❌                   | ❌                   | ❌           | ❌                |
| Used in production                                                                   | ❌                                             | ❌                   | ❌                   | ❌           | ✅                |
| Stable and large adoption                                                            | ❌                                             | ❌                   | ❌                   | ❌           | ✅                |
| Proximity Voice ([Simple Voice Chat](https://modrinth.com/plugin/simple-voice-chat)) | ✅                                             | ❌                   | ❌                   | ❌           | ✅                |
| Lighting                                                                             | ✅                                             | ❌                   | ✅                   | ❌           | ✅                |
| Placing blocks                                                                       | ✅                                             | ❌                   | ❌                   | ?           | ✅                |
| Breaking blocks                                                                      | ✅                                             | ❌                   | ❌                   | ?           | ✅                |
| Blocks physics                                                                       | ✅                                             | ❌                   | ❌                   | ❌           | ✅                |
| Entity-entity collisions                                                             | ✅                                             | ❌                   | ❌                   | ❌           | ✅                |
| Block-entity collisions                                                              | ✅                                             | ❌                   | ❌                   | ✅           | ✅                |
| World borders                                                                        | ✅                                             | ❌                   | ❌                   | ✅           | ✅                |
| Block Edit API (think WorldEdit)                                                     | ✅                                             | ❌                   | ❌                   | ✅           | ✅                |
| PvP                                                                                  | ✅                                             | ❌                   | ❌                   | ✅           | ✅                |
| Vertical scaling (fully multi-threaded)                                              | ✅                                             | ❌                   | ❌                   | ✅           | ✅                |
| Horizontal scaling                                                                   | ✅                                             | ❌                   | ❌                   | ❌           | ❌                |
| Advanced tracing support                                                             | ✅ ([tracy](https://github.com/wolfpld/tracy)) | ❌                   | ❌                   | ✅           | ❌                |
| Set Resource Packets                                                                 | ❌                                             | ❌                   | ?                   | ✅           | ✅                |
| Minecraft 1.20.1                                                                     | ✅                                             | ❌                   | ✅                   | ✅           | ✅                |
| Minecraft 1.21.x                                                                     | ❌                                             | ✅                   | ❌                   | ❌           | ✅                |
| Proxy Support (Velocity)                                                             | ✅                                             | ✅                   | ?                   | ✅           | ✅                |
| Inventory                                                                            | ✅                                             | ✅                   | ?                   | ✅           | ✅                |
| Particle Support                                                                     | ✅                                             | ✅                   | ?                   | ✅           | ✅                |
| RCON                                                                                 | ❌                                             | ✅                   | ❌                   | ?           | ✅                |
| Chat Support                                                                         | ❌                                             | ✅                   | ?                   | ✅           | ✅                |
| Command Support                                                                      | ✅                                             | ✅                   | ?                   | ✅           | ✅                |

`*` = Minestom has many more features than we've mentioned here. If you're comfortable using Java and want to run a
minigame Minecraft server in a production environment, Minestom is a good choice. It's especially recommended if you
don't need to support an extremely large number of players (like thousands).
