---
name: solidcraft-mechanisms
description: Use when building, driving or animating an assembly or mechanism in SolidCraft through its MCP server - components, grounding, joints and joint origins, rigid groups, motion links, closed loops such as an actuator on a hinged panel or a linkage, contact sets, motion studies, degrees of freedom - or when a drive moves nothing, stops early or a part jumps when it is joined. To model the parts themselves, use solidcraft-modeling.
---

# SolidCraft mechanisms

## Overview
An assembly is components placed by joints. Work the mechanism out as world points and axes first: every pin, slide and hinge, which part each one joins, and the value you expect at each pose. Then give every joint origin by those coordinates, and check each step against a number you computed yourself. A drive that reports success can still have moved nothing. Command shapes, a worked example, the traps and reply sizes are in `reference.md` next to this file. To build the parts themselves, use `solidcraft-modeling`.

Written against SolidCraft `main` after 0.11.1, which counts the degrees of freedom of closed loops and carries motion links through chains made in any order. The traps in `reference.md` say which answers are about to change.

## Before the first call
1. Read `reference.md`.
2. **Open:** if the design is not open yet, run `open {path}`. When the server is bridged to the app and the current design is `dirty`, ask before opening anything.
3. **Map it:** run `inspect_design {"measure":false,"sketches":false}`, then `execute joint.list`. `joint.list` shows `fixed` (the grounded occurrences, always with the root, 0), `dof` per occurrence, `conflicts`, the joints, links and named origins, at about 0.5 KB per joint. `execute motion.list` lists the motion studies.
4. **Existing design:** take `edit.checkpoint` in its own call before you move bodies into components. `edit.restore_checkpoint {checkpoint}` goes back exactly.
5. **Write the plan:**
   - the components, and which one is grounded
   - each joint: type, the two occurrences, its world point and axis, limits
   - each motion link and its ratio
   - for each loop, the relation you will check, such as actuator stroke against hinge angle, and where it should stop
   - the pose to return to before saving

## Build, in stages
1. **Components and grounding.**
   - Bodies made in the root go into components with `component.create {"name":..., "parent":"root", "activate":false}`, then `component.move_bodies {"bodies":[...], "component":...}`.
   - A part that moves in two pieces, such as an actuator's barrel and rod, becomes two components.
   - Ground the base: `occurrence.ground {"occurrence":"Frame:1", "grounded":true}`. Occurrences are named `<component>:<n>`. Joint commands also take a component name (its first occurrence); `occurrence.ground` does not.
2. **Open-chain joints in one batch.** Read `occurrence.properties` of each moving occurrence before the batch and after it. If something moved, split the batch to find the joint.
3. **Each joint that closes a loop, in a call of its own.** Then check `conflicts` in its reply.
4. **Motion links.**
5. **Contact sets and the motion study.**

## Joints
- **Origins by world coordinates:** `{"occurrence":..., "point":[x,y,z], "z":[ax,ay,az]}`. Always give `occurrence`; without it the nearest body is used. Use the same form on both sides; `frame`, `face`, `circle` and named origins are in `reference.md`.
- **Pass `flip: true`** when both origins are the same world point and axis, which is the case when you join parts where they sit. Without it, b's z mates against a's and part b turns 180 degrees.
- **Parts already in place:** `joint.as_built {"type":..., "a":..., "b":..., "at":{origin}, "name":..., "limits":[[min,max]]}` keeps them where they are.
- **Values and limits** are degrees for angles and mm for distances. An angle turns right-handed about the joint's z. A drive past a joint's own limits is clamped to them, which is not a failure. A new joint starts at 0, so its limits must include 0, or the part turns to the nearer limit as it is joined. On a joint with several values, drive with `values`: `value` resets all but the first to 0.
- **Every chain must reach something grounded.** A part that is neither grounded nor joined to a grounded part is never placed. A drive on a joint in that chain answers with the new value and no conflicts, and nothing moves. `dof: 6` on an occurrence in `joint.list` is the sign. Fix it with `joint.rigid_group {"occurrences":["Frame:1", ...]}`, or with a joint to the grounded structure.

## Linked and closed motion
- **Motion links:** `joint.motion_link {"a":..., "b":..., "ratio":r, "offset":o}` sets b = r·a + o, clamped to b's own limits. `offset` is in radians for an angle (mm for a distance), unlike every other joint value. A mirrored part takes `ratio: -1`, and limits mirrored too. Chains (a drives b, b drives c) work in any order of creation. Drive only the head of a chain.
- **Closed loops:** an actuator between a frame and a hinged panel takes three joints besides the hinge:
  - a revolute root pin (frame and barrel)
  - a slider (barrel and rod), with z along the actuator from root to tip on both origins, both at the root pin, and the stroke as its limits
  - a revolute tip pin (rod and panel), which closes the loop

  A four-bar or a slider-crank follows the same pattern. Drive the hinge; the pins and the slider follow.
- **Stops:** at the end of the stroke the drive stops, with `stopped_by_loop: {joint, fraction}` naming the joint that could not follow. A loop that cannot close at all is listed under `conflicts`, and nothing in it moves.
- **dof** counts the loop: here the panel, barrel and rod report 1 each. At a pose where the actuator lies along its tip pin's path, a part can report 0 and still move; drive a little and list again.

## Contact and motion studies
- **Contact:** `contact.create {"occurrences":[...], "name":...}` and `contact.enable_sets {}`. A drive then stops where two bodies of one set would pass through each other, with `stopped_by_contact: {a, b, fraction}`. `contact.check {}` lists the pairs that overlap now.
- **Motion study:** `motion.study {"name":..., "steps":N, "keys":[{"joint":..., "points":[[step, value], ...]}]}`. Values are degrees or mm, linear between points. Key only the driving joints; linked joints and loops follow.
- **Play:** `motion.play {"study":..., "step":s}` sets the joints there, which changes the design, and answers every occurrence's pose (about 0.2 KB per occurrence). `motion.export {"study":..., "samples":n}` gives the poses at every step without changing the design.

## Verify
- **Each joint:** `execute occurrence.properties {"occurrence":"Wing:1"}` (under 1 KB) gives the world bbox and centre of mass of one occurrence. Joining must not move them.
- **Each drive:** compute the expected value of every follower, and read it back from `joint.list` (degrees and mm, with `unit`). For an actuator from root R to tip T on a panel hinged at H: `L(θ) = |T(θ) - R|`, where `T(θ)` is T turned by θ about the hinge axis through H, and the stroke is `L(θ) - L(0)`. A closed loop matches to 1e-6 mm.
- **Each stop:** solve for where the limit is reached, and compare with the value the drive stopped at. Stops are found by bisection, so they land within about 0.01 degrees.
- **Links:** b must equal r·a + o exactly, unless that is outside b's limits; then b sits at the limit and the mirror stops following.
- **The whole motion:** `motion.play` at a few steps, or one `motion.export`, and compare the poses with the values you planned.
- **Look:** when the server is bridged, `screenshot` after `motion.play` shows the app window at that step.

## Save
- Drive the joints back to the pose you planned to keep (`motion.play` at step 0, or `joint.drive`), then `save {path}`.
- An `.urdf` export leaves out each joint that closes a loop, because URDF is a tree.

## Red flags

| Thought | Do instead |
|---|---|
| "The origins are at the same point, so `flip` doesn't matter" | Pass `flip: true`, or the part turns 180 degrees. |
| "The drive answered values, so it moved" | Look for `dof: 6` in `joint.list`; a floating chain answers and moves nothing. |
| "`joint.drive` says 0.349, the drive failed" | Its `values` are radians; `joint.list` gives degrees. (Once its replies carry `unit`, they are degrees.) |
| "Key every joint of the loop in the study" | Key the driving joint only; the loop follows. |
| "Check the pose with `component.list`" | `occurrence.properties` on the one occurrence. `component.list` is 140 KB on a 36-part assembly. |
| "Open the design with `file.open`" | The `open` tool, or `doc.open`; there is no `file.open`. |
