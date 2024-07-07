# The ZephyrVM Stack

Many of the host-guest actions in the ZephyrVM are memory-based (see multival in the intro).

However, we've also experimented with a single-val stack object on the host-side that the guest
calls loading instructions that are then interpreted by the host environment.

The stack proposes no significant advantages compared to just relying on allocation + multi-val
functions.
