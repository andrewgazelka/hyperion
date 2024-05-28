# Hyperion

[![Star History Chart](https://api.star-history.com/svg?repos=andrewgazelka/hyperion&type=Date)](https://star-history.com/#andrewgazelka/hyperion&Date)

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)
[![wakatime](https://wakatime.com/badge/github/andrewgazelka/hyperion.svg)](https://wakatime.com/badge/github/andrewgazelka/hyperion)

_From the creator of [SwarmBot](https://github.com/SwarmBotMC/SwarmBot):_

**How can we get 10k players to PvP at once on a Minecraft server to break the [Guinness World Record for largest PvP battle in any game](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle) of 8,825 players?** 

<div align="center">    
  <img src="https://github.com/andrewgazelka/hyperion/assets/7644264/ac11d0f5-cabf-41af-ad83-50496d00e391"/>

  <img src="https://github.com/andrewgazelka/hyperion/assets/7644264/59b3af59-7326-46ad-9f24-47725ebc1b61"/>
  <em>The image below shows 100k zombies (with collisions) running at ~8 ms/tick on an M2 MacBook Pro.</em>
  <img src="https://github.com/andrewgazelka/hyperion/assets/7644264/092d077b-32ce-4b91-8a4f-56403d2c4ea6"/>
</div>

# Running

## Step 1: The event
```bash
git clone https://github.com/andrewgazelka/hyperion
cd hyperion
cargo run --release -p infection
```

When joining the server downloads a map and loads it.  

## Step 2: The proxy

1. Join the [Discord server](https://discord.gg/c99jFRtPc5)
2. Look in the `#build` channel for the latest proxy release
3. Run it with `./{executable_name}`. You will likely need to make it executable first with `chmod +x ./{exeuctable_name}`


# FAQ

**Q: How is hyperion so fast?**

- Hyperion generally does a good job at utilizing all cores of your device. On an M2 MBP, CPU utilization is over 1000% when spawning hundreds of thousands of zombies.
- We *aim* to utilize as much SIMD as possible. This is still a work in progress, but as it is built out we are aiming to use SIMD-friendly data structures. Make sure to compile with `RUSTFLAGS='-C target-cpu=native'` to allow the compiler to use SIMD intrinsics.
- A lot of work has been done in reducing synchronization and limiting context switching. For instance, `#[thread_local]` is heavily relied upon.

**Q: Aren't you re-inventing the wheel?**

- No, we rely on [valence-protocol](https://github.com/valence-rs/valence/tree/main/crates/valence_protocol)  and [evenio](https://github.com/rj00a/evenio), an ECS framework created by [rj00a](https://github.com/rj00a) the creator of valence.
  - Although we rely on this work, we do not completely depend upon valence as it is currently [being rewritten](https://github.com/valence-rs/valence/issues/596) to use `evenio` due to (among other things) performance limitations using [bevy](https://github.com/bevyengine/bevy).

**Q: What is the goal of this project? Making valence 2.0?**

- Nope, the goal of this project is to break the [Guinness World Record](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle). Everything else is secondary.
- We are not implementing a 1:1 with a vanilla Minecraft server. We are only implementing enough to support the event which will have 10k players.

**Q: How will this handle 10k players given the network requirements?**

- The current idea is to have load balancers which do encryption/decryption and compression/decompression with a direct link to hyperion.

**Q: Why not just use a distributed server?**

- This adds a lot of complexity and there are always trade-offs. Of course given an event with 10k players real-world players are needed to see if a server can truly handle them (bots only are so realistic). I suppose if there is some inherent limiting factor, this could be distributed, but given current performance estimations, I highly doubt making the server distributed will be the best path of action—in particular because there will most likely not be isolated regions in the world.
