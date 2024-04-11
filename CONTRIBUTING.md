Oooh you are interested in contributing to this project? Great! 

This page is for Software Developers, Event Organizers / Sponsors, and Beta/Pen Testers.
Please [join our Discord server](https://discord.gg/PBfnDtj5Wb).

- [FAQ](#faq)
    - [What is the goal of this project?](#what-is-the-goal-of-this-project)
    - [What will the event look like?](#what-will-the-event-look-like)
    - [How can I join?](#how-can-i-join)
    - [Am I a good person to contribute to this project?](#am-i-a-good-person-to-contribute-to-this-project)
      - [As a software developer](#as-a-software-developer)
      - [As an event organizer/facilitator/sponsor](#as-an-event-organizerfacilitatorsponsor)
      - [As a beta/pen tester](#as-a-betapen-tester)
    - [How will anti-cheat be implemented?](#how-will-anti-cheat-be-implemented)
    - [As a software developer how should I contribute?](#as-a-software-developer-how-should-i-contribute)

# FAQ

### What is the goal of this project?
- Breaking the Guinness World Record for the largest PvP battle in any game (let's get 10k players as that is a nice number).
- All in one world.

### What will the event look like?
- TBD
- DEFINITELY NOT a 1:1 implementation of the vanilla server.
- Perhaps something similar to [Overcast Network](https://oc.tc/) (I am open to other ideas) but with 10k players.
    - I am breaking it into smaller, achievable [milestones](https://github.com/andrewgazelka/hyperion/milestones).
    - Likely have 10 YouTubers with 1k players each on their team. Alternatively, we could have 5 YouTubers with 10k players each on their team and then have 5 teams of 1k players each not assigned to an YouTuber. *None of this is not set in stone.*

### How can I join?
- Join our [Discord server](https://discord.gg/PBfnDtj5Wb) and ask for a link to the test server.
    - It is currently 1.20.1, but we want to upgrade to 1.20.4 as soon as [valence supports it](https://github.com/valence-rs/valence/pull/599).
- TBD for the actual event but see [as a beta/pen tester](#as-a-betapen-tester) if you are interested in testing prior to the event.

### Am I a good person to contribute to this project?

You are a good person to contribute to this project if you are:

- committed to the project
- wanting to get a Guinness World Record on your resume

#### As a software developer
- you plan to be somewhat addicted to trying to break this record.
- have decently deep knowledge of at **least one** of the following: 
  - Rust
  - C(++)
  - the Minecraft protocol
  - High performance computing
  - Infrastructure & Networking (load balancing, reverse proxy servers, etc)
- Have a macOS or Linux machine 
  - we are currently not supporting Windows due to `monoio`. However, if you are willing to help with that, we would still be very happy to have you on the team.
  - the actual server will be running on a Linux machine, but supporting macOS is nice because it is easier to test on.
- Willing to create high quality code that will run in a production environment with 10k players where the crashing because of an `unwrap` is not acceptable.
- Willing to challenge the status quo and make the project better & more performant.
- Also see [as a software developer](#as-a-software-developer-how-should-i-contribute) for more information.

#### As an event organizer/facilitator/sponsor
- Have connections to large YouTubers/Twitch streamers/etc who have 1M+ subscribers/followers.
- Able to make connections with Guinness World Records to determine the best path forward to obtain the record.
  - From information I have gathered, they often require a lot of money to obtain the record for virtual events.
  - I am aiming for the front page of Hacker News. Since this is a Rust project, I think there is a decent proability it will be there. If this occurs, it might put more pressure on Guinness World Records to verify the record.
  - We will be logging as much data as possible in order to have a better chance of GWR verifying the record. Ideally, we will have enough data to have a full replay of the event (log stream of incoming packets and able to replay them potentially).


#### As a beta/pen tester
- You think outside of the box and are determined to try to break the server.

### How will anti-cheat be implemented?
- Good question.
- Will there be some type of proxy server? hmmm maybe.
- We will probably want a decently naïve anti-cheat system that will be able to give us a confidence score for each player cheating.
  - Players who are doing "too well" will be flagged for moderators to take action.
  - We will have several moderations who will be able to take action on flagged players.



### As a software developer how should I contribute?
- Look at the [milestones](https://github.com/andrewgazelka/hyperion/milestones). Currently, we are on [PvP Sword test server](https://github.com/andrewgazelka/hyperion/milestone/1).
- If there is an issue that is not assigned to anyone, you can pick it up and assign it to yourself.
- **BENCH BENCH BENCH**
  - If you make a change that has a good chance of affecting performance, please benchmark it. 
    - If you want to see trends over the same codebase we use [divan](https://github.com/nvzqz/divan).
    - If you want to bench between two versions of the same code we use [tango](https://github.com/pnkfelix/tango)
  - At bare minimum, try to see how performance changes when launching several bots. You can do this easily with [rust-mc-bot](https://github.com/andrewgazelka/rust-mc-bot/tree/optimize).




