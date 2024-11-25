---
# https://vitepress.dev/reference/default-theme-home-page
layout: home

hero:
  name: "Hyperion"
  text: "The most advanced Minecraft game engine built in Rust"
  tagline: 170,000 players in one world at 20 TPS
  actions:
    - theme: brand
      text: Architecture
      link: /architecture/introduction
    - theme: alt
      text: 10,000 Player PvP
      link: /tag/introduction

features:
  - title: Run massive events with confidence
    details: Built in Rust, you can be highly confident your event will not crash from memory leaks or SEGFAULTS.
  - title: Vertical and horizontal scalability
    details: In our testing I/O is the main bottleneck in massive events. As such, we made it so I/O logic can be offloaded horizontally. The actual core game server is scaled vertically.
  - title: Easy debugging profiling
    details: All tasks are easily viewable in a tracing UI. All entities are viewable and modifiable from Flecs Explorer.
---

