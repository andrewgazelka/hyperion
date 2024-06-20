

What is awesome is we have this proxy and everything is nice and dandy.
However, right now we have one buffer per thread.
In the cases when we have one buffer per thread
and we're encoding packets to each thread and we're sending one buffer at a time,
what's going to happen is packets get out of order.
Say packets that are encoded later are on a thread that accesses buffer 1 and threads that are encoded before access buffer 2 and then at the end of the game tick we're sending buffer 1 then buffer 2. That's going to make it so packets are out of order and that is not what we want.
I am quite sure at this point, after looking into the issue that I had before,
that this is actually the reason that when you had more than one player, all the packet errors were happening.
I think
they weren't happening prior
because Rayon just saw that there was one element and it wasn't actually spawning anything onto a thread.

