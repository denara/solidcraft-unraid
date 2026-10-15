# Components and joints

A design is a tree of **components**. **New Component** (`component.create`) adds one, and
**Create Components from Bodies** (`component.from_bodies`) turns bodies into components. Each
component has its own origin, bodies, sketches and timeline entries. The **active component**
(the radio button in the browser) receives new sketches and features; the others are drawn
faded.

A component is placed by its **occurrences**. Paste creates another instance of the same
component, and **Paste New** creates an independent copy. Move an occurrence with the move
triad, then **Capture Position** to keep it or **Revert** to put it back. SolidCraft asks before
the next command if a move wasn't captured. **Ground** fixes an occurrence in place.

Paste New copies nested component definitions, sketches, feature histories and generated
dimension parameters. Repeated child instances remain shared within the new copy. Materials,
appearances and wholly internal joints/motion records receive copied references. Design-wide
user parameters remain shared; retained external links and omitted cross-boundary motion
records are reported. Unsupported destructive links refuse the entire copy. See
[individual-instance and copy details](https://github.com/bherbruck/solidcraft/blob/main/docs/occurrence-workflows.md).

**Joints** (`joint.create`, J) connect two components by joint origins that snap to faces, edges,
circles and vertices: rigid, revolute, slider, cylindrical, pin-slot, planar and ball, with
offsets, flip and limits. **As-built Joint** keeps the current position, **Rigid Group** locks
components together, **Drive Joints** moves a joint through its values, and **Motion Link**
couples two joints. Links can be chained (one joint drives a second, which drives a
third), and a value is carried through a chain of up to 64 links in whatever order they were
made. Contact sets stop motion where bodies would pass through each other,
**Motion Study** keys joint values along a timeline, and **Interference** shows overlapping
volumes in red.

Joints may close a loop, as in a four-bar linkage or a slider-crank. When one joint of the loop
is driven, the other joints in the loop follow so that every joint still meets, and the
mechanism stays in the assembly mode it started in (an open four-bar does not flip to the
crossed one). If the motion would take a loop past the point where it can still close, for
example driving a rocker beyond its swing, the motion stops at the last position where the loop
closes, and the drive reports `stopped_by_loop`. A loop that cannot close at all is listed
under `conflicts` and nothing in it moves. `joint.list` reports how many degrees of freedom each
part still has with its loops closed: the moving parts of a four-bar have one between them, and a
loop locked by a rigid joint has none. The Drive Joints dialog says when a drive stops short and
why. While it previews, a row under the values gives the value the joint reached and the reason
(the loop that can't close further, or the two parts a contact set keeps apart), and after OK the
status line repeats it. The value box keeps what you typed. The dialog opens on the joint
picked in the browser, or on the first joint that moves when none is picked.

The solver places what it reaches from the root and from grounded components. A chain of joints
that is joined to nothing grounded is never placed, so driving one of its joints moves nothing.
The drive says so: `joint.drive` answers `moved: false` with a `reason` and the `unconnected`
part ("Pedestal:1 is not connected to anything grounded"), the Drive Joints dialog shows the same
row, `motion.play` lists such joints under `unmoved`, and `joint.list` names every such chain under
`floating`. Ground a part of the chain, or join it to a grounded part (a rigid as-built joint keeps
it where it is), and it moves.

`joint.drive` answers the joint's values as `joint.list` gives them, in degrees and millimetres
with their units and limits, and `moved` says whether any part moved. A joint's origins are mated
by default: B's z axis is turned against A's, as two faces meet. `flip: true` lines them up
instead, so two origins given at one point with the same z stay as they are. For scripts that
drive many steps, `quiet: true` leaves the per-part degrees of freedom out of joint answers, and
`motion.play` with `quiet: true` gives the poses of only the parts that moved.

**Insert Design** (`component.insert_design {path, translate?}`) adds a component linked to
another saved design, so each part can live in its own document. The part's solid bodies come
in as they sit in that design, and they are kept in the assembly, so it opens even without the
part file. The link is stored relative to the assembly's folder once the assembly is saved.
After the part file changes, **Update Links** (`component.update_links`) reads it again.
**Break Link** (`component.break_link`) keeps the bodies and forgets the file, and
`component.links` lists the links and whether their files are found. A linked component holds
the part's bodies only: its timeline, parameters and sheet-metal data (flat pattern, bend
table) stay in the part's own design.

STEP export writes the component tree as an assembly.

Right-click a component in the Browser and choose **Properties** to inspect
material-aware mass, volume, surface area, centre of mass and world bounds. Descendants and
repeated instances contribute to assembly totals; pending moves are included. A component
uses its first placement; `occurrence.properties` selects a specific instance. Root Properties
measures the complete assembly. Volumes are summed, including overlapping bodies, rather than
measuring their union. The same workflow is available through `component.properties` and
`occurrence.properties`; see
[placement and measurement details](https://github.com/bherbruck/solidcraft/blob/main/docs/assembly-properties.md).
