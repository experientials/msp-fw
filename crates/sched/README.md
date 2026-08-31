# `sched` — cooperative task scheduler

A tiny **run-to-completion** cooperative scheduler for the MSP430 firmware in this repo. One stack,
no preemption, no allocator, no RTOS: each task runs a short non-blocking step and says when it
wants to run next, so many time-based concerns (sampling at a fixed/variable rate, animation
output, talking to another MCU) interleave on a 1 MHz MCU.

It is **PAC-agnostic on purpose** — it never touches a chip, timer, or PAC. The caller supplies:
- a **context** `C` (its peripherals handle + a small cross-task blackboard), handed to every task;
- the current **time** `now` (monotonic ms), passed into `tick`.

So one copy of this crate is correct for **FR2433, FR2476, the examples, and host unit tests** —
matching the repo's "don't depend on FR2476-only features" and "no HAL" conventions.

## Use it

Add the path dependency (from `diag/`, an `examples/*/`, or the product firmware crate):

```toml
[dependencies]
sched = { path = "../crates/sched" }   # from an examples/* crate: "../../crates/sched"
```

Define a context, implement `Task` for each concern, list them in `Slot`s, and loop `tick`:

```rust
struct Cx<'a> { p: &'a Peripherals, motion: bool }   // hardware + blackboard

struct Sampler { prev: bool }                         // task-private state
impl<'a> sched::Task<Cx<'a>> for Sampler {
    fn poll(&mut self, cx: &mut Cx<'a>, _now: u32) -> Option<u32> {
        cx.motion = read_pin(cx.p);                  // publish to the blackboard
        Some(if cx.motion { 5 } else { 50 })         // variable cadence
    }
}

let mut s = Sampler { prev: false };
let mut cx = Cx { p: &p, motion: false };
let mut slots = [sched::Slot::every(50, &mut s)];
loop {
    let now = clock.now_ms(&p);                       // caller's time base
    sched::tick(&mut slots, &mut cx, now);
}
```

See `diag/src/tasks.rs` + `diag/src/clock.rs` for the live example (radar sampling + POST).

## The one rule

**Never block in `poll`.** A task that spins stalls every other task — there is no preemption.
Anything that waits (an I²C reply, a frame delay, a comms ACK) becomes *state held in the task* plus
a `now`-based deadline check, not a spin. The hardware watchdog is the backstop for a task that
breaks this; it is not a licence to block.

Task-private state lives in the task (`&mut self`); state two tasks share lives in the context `C`
(the explicit blackboard) — never in globals.
