# Project 10k

How can we get 10k players to PvP at once on a Minecraft server?

There are many faction servers which have 500 players on Start of The World (SOTW).
Usually this is around the upper limit for the number of players that can be in one world in vanilla Minecraft.

## The world

Suppose there is a 10k x 10k world.
This we can allocate every player (10k x 10k)  / 10k = 10k blocks.

This is equivalent of a square of length sqrt(10k) = 100. If we place the player in the middle, this will mean that 
we can allocate a square that stretches 50 blocks NSEW of the center where we can place a player. 

A circle of radius r has an area of pi * r^2. If we allocate circles we will have

pi * r^2 = 10k
r^2 = 10k/pi
r = sqrt(10k / pi)
r = 56.41

Which means the distance to the nearest player would be 2*r = 112.82

So if we spread players out equally, there will be 112.82 blocks between them. Of course this is not 
possible as circles can not cover the entire map, but perhaps this would be the average distance 
to the nearest player if we chose random locations (not sure about maths.
If we assigned players to a grid, then there would be exactly 100 blocks between them.

r_c = 56.41  is 3.525625 chunks and
r_s = 50 is 3.125 chunks

If players have > 3 chunk render distance, the entire map will be rendered at once.

## Memory

If we have a superflat world with one type of block, we would not have to store any blocks.
However, we probably do not want to do this.

Suppose the world is 20 blocks deep. This means the total volume of the map is

10k x 10k x 20 blocks = 2,000,000,000 (2 billion)

If we have one byte per block (which is realistic if we restrict the number of blocks) we get this only taking

2B bytes = 2 GB

This is absolutely feasible. 

In fact, if we had a normal size world

10k x 10k x 256 and one byte per block this would only take

25.6 GB