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
