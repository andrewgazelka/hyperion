# Hyperion

[![Discord invite link](https://dcbadge.vercel.app/api/server/PBfnDtj5Wb)](https://discord.gg/PBfnDtj5Wb)
[![wakatime](https://wakatime.com/badge/github/andrewgazelka/hyperion.svg)](https://wakatime.com/badge/github/andrewgazelka/hyperion)

_From the creator of [SwarmBot](https://github.com/SwarmBotMC/SwarmBot):_

**How can we get 10k players to PvP at once on a Minecraft server to break the [Guinness World Record for largest PvP battle in any game](https://www.guinnessworldrecords.com/world-records/105603-largest-videogame-pvp-battle) of 8,825 players?** 

<p align="center">
  <em>The image below shows 30k zombies (with collisions) running at 8 ms/tick on an M2 MacBook Pro.</em>
  <img src="https://github.com/andrewgazelka/hyperion/assets/7644264/d842d7c9-ee0c-4df3-85d6-46d91e455be5"/>
  <br><br>
  <em>100k zombies can be run at 20 ms/tick</em>
  <img src=https://github.com/andrewgazelka/hyperion/assets/7644264/c02e720b-b878-418e-a949-ff7c24a53e82/>

</p>

# Running

## Manual
```bash
# Install Git LFS
pkg-manager install git-lfs

# Setup Git LFS
git lfs install

git clone https://github.com/andrewgazelka/hyperion
cd hyperion
cargo run --release
```

## Docker
Note: this is no longer recommended as [Docker blocks io_uring](https://github.com/moby/moby/commit/891241e7e74d4aae6de5f6125574eb994f25e169).
```bash
# Install Git LFS
pkg-manager install git-lfs

# Setup Git LFS
git lfs install

git clone https://github.com/andrewgazelka/hyperion
cd hyperion
docker compose up --build release

# if you want to run in debug
# docker compose up --build debug
```

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


# Calculations

There are many faction servers which have 500 players on Start of The World (SOTW).
Usually this is around the upper limit for the number of players that can be in one world in vanilla Minecraft.

## The world

Suppose there is a $10\text{k} \times 10\text{k}$ world.
This we can allocate every player $(10\text{k} \times 10\text{k})  / 10\text{k} = 10\text{k}$ blocks.

This is equivalent of a square of length $\sqrt{10\text{k}} = 100$. If we place the player in the middle, this will mean that 
we can allocate a square that stretches $50$ blocks NSEW of the center where we can place a player. 

A circle of radius $r$ has an area of $\pi  r^2$. If we allocate circles we will have

$$
\begin{align*}
\pi  r^2 &= 10\text{k} \\
r^2 &= 10\text{k}/\pi \\
r &= \sqrt{10\text{k}/\pi} \\
r &\approx 56.41
\end{align*}
$$

Which means the distance to the nearest player would be $2r = 112.82$

So if we spread players out equally, there will be $112.82$ blocks between them. Of course this is not 
possible as circles can not cover the entire map, but perhaps this would be the average distance 
to the nearest player if we chose random locations (not sure about maths).
If we assigned players to a grid, then there would be exactly $100$ blocks between them.

$r_c = 56.41$  is $3.525625$ chunks and

$r_s = 50$ is $3.125$ chunks

If players have > 3 chunk render distance, the entire map will be rendered at once.

## Memory

If we have a superflat world with one type of block, we would not have to store any blocks.
However, we probably do not want to do this.

Suppose the world is 20 blocks deep. This means the total volume of the map is

$10\text{k} \times 10\text{k} \times 20 \hspace{0.16667em} \text{blocks} = 2,000,000,000 \hspace{0.16667em} \text{blocks}$

If we have one byte per block (which is realistic if we restrict the number of blocks) we get this only taking

$2,000,000,000 \hspace{0.16667em} \text{bytes} = 2 \hspace{0.16667em} \text{GB}$

This is absolutely feasible. 

In fact, if we had a normal size world

$10\text{k} \times 10\text{k} \times 256$ and one byte per block this would only take $25.6 \hspace{0.16667em} \text{GB}$

## Core Count

Suppose we get a 64-core machine. This means that we can allocate 
$10\text{k} / 64 = 156.25 \hspace{0.16667em} \hspace{0.16667em} \text{players} / \text{core}$.
This is much under what a normal vanilla server can do on one core.

## Network

Network is very dependent on player packing.
A large factor of sending packets over network has to do with sending player updates.
The bandwidth will be $O(nm)$, where $m$ is a "packing factor" and the number of players within a given radius. 
Where all players can see all other players (i.e., there is a small radius), the bandwidth will be $O(n^2)$.

If we adjust the map size so that there is always a constant number of players m within a certain radius of a map, 
we will get the bandwidth will be $O(nm) = O(Cn) = CO(n) = O(n)$ for a constant $C$.
