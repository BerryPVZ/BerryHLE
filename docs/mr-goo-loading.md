# Mr. Goo Free loading

Tested with Mr Goo Free 2.3 (`com.tayasui.mrpoofree`).

The archived navigation controller's superclass used the ordinary controller
class-name nib search to create its view. That reopened the archive containing
the navigation controller itself, repeating until the message recursion guard
returned nil. UINavigationController now creates its own container view.

After that correction, the app finished loading but remained on its startup
advertisement. It cancels a previous delayed `next` request before scheduling
a replacement. The old cancellation list suppressed that replacement. Delayed
perform cancellation now invalidates only matching timers already scheduled
on the calling thread's run loop, including the object argument when supplied.
New requests do not inherit cancellation, and cancellation no longer leaves
process-global entries tied to object addresses. Delayed selectors are also
scheduled on the calling thread's run loop.

In the real ARM guest, the corrected run reached `End loading`, dispatched
`AdStartViewController next` and the normal navigation callback, displayed the
game menu, and accepted a press on Start. The character, background and game
controls rendered and the game reached repeated EAGL presentation frames.
The original startup recursion did not recur. This is a startup and rendering
check, not a full playthrough or a recording/camera compatibility claim.

## Landscape surface and input

The app's profile now enables landscape UIScreen geometry and zero presentation
rotation. The compositor honors the same explicit rotation option as EAGL.
The Unity EAGLView is kept at landscape bounds as well: this player otherwise
swaps the already-landscape UIScreen dimensions and recreates a portrait
renderbuffer. Both the runtime renderbuffer and viewport were verified at
480x320, with a 960x640 composed frame showing upright menu and game controls.

The screen option is stored on Options rather than setting a process-wide
environment flag, so a prior app-picker UIScreen query cannot cache it off.
For this display-aligned profile, mouse/finger input maps directly into the
480x320 screen instead of being rotated into portrait coordinates. Tests cover
option isolation and input at the center, edges and letterbox bars.
