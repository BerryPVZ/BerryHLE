# Berry fixes and Globbert input/artwork integration

Base: `KlugKlugTG/HyperHLE-Reborn` at
`5eb32084320dc820f8fbc76d06d1345a103c23ed`.
Source: `BerryPVZ/HyperHLE-Fork` at
`1d98d5925e64641f3694761129c12b9ee0c9866e`.
The common ancestor is `9f77db96aaaa6b31bf78db7e7c78cae89fb3ee55`;
Reborn has exactly 261 commits after it. Berry has 42 commits absent from
Reborn, comprising 25 non-merge changes.

## Included

The final source incorporates Berry's stdio pushback correction, Objective-C
super structure returns, rotated-layer geometry, safe framebuffer readback,
Foundation date/invocation/proxy/property-list/set handling, BTC Sports/Free
compatibility, Android previous-exit reports, archived sprite decoding,
Core Text baseline drawing and ownership, Mach-O image enumeration, deferred
layout, old-zlib compatibility, resource-directory isolation, PetsWar/Great
Wall audio fixes, and the Food Fight rendering, input, audio and animation
fixes through `55d09ce`.

Flex's optional CLI runtime hooks and executable metadata parser are retained.
They remain disabled by default. See [FLEX.md](../FLEX.md).

## Excluded and reconciled

`src/environment/app_picker.rs` and `src/bundle.rs` match the Reborn base.
Berry's picker animations, glossy icon override, Flex editor, and picker
method-browser interface are excluded. No picker redesign is included.

Reborn's newer Retina image scales and @3x fallbacks, 16-bit grayscale bitmap
byte ordering, Core Text custom fonts/glyph APIs, attributed-string ranges,
method-signature lookup and file locking are preserved. Equivalent older
Berry implementations were reconciled into those implementations instead of
replacing them. The transient BTC component-recovery hook added in `72c4e84`
and removed by `3fb1f76` is absent from the final source, matching Berry's tip.

Reborn's two framebuffer diagnostic paths now handle failed optional readbacks
without preventing presentation or destructuring an invalid result.
The existing duplicate `NSCalendar calendarWithIdentifier:` implementation is
removed so export uniqueness validation passes. Windows OpenAL debug builds
use the same DLL CRT as Rust, avoiding unresolved debug-CRT symbols.

## Globbert

Globbert 1.1 (`com.andydev.globbert`) defines the custom classes
`OneFingerRotationGestureRecognizer`, `TapGestureRecognizer`, and
`SmallWheelGestureRecognizer`. Their guest implementations call the superclass
touch hooks and `setState:`. Dispatch now delivers their began/moved/ended
hooks and the original UIEvent, and delivers cancellation on stale-touch
cleanup. The base hooks, reset and state setter are implemented so those
super calls work. Movement remains owned by the original hit-test view, while
recognizers on ancestor views observe it.

The plain PNG fallback in `UIImage imageNamed:` is restored ahead of Retina
fallbacks. Globbert's extensionless `R&S_00015` and `R&S_Ident_00023` identify
1024-by-768 PNGs in the supplied IPA. The headphones image
`4_HEADPHONES_H37)225_389-216.png` is also present (244 by 31 pixels).
Image lookup regression tests cover these names, suffix placement and dots
in resource directories. The headphones animation uses the bundled
`Headphones.plist`, individual image loads and frame updates; the integrated
layer/layout fixes are shared with that path.

## Validation

All 414 library unit tests pass with `ALSOFT_DRIVERS=null`. The Windows debug
executable builds, and `--help` exits successfully. The normal audio backend
caused a post-test shutdown abort in this session; the null backend exits
cleanly. Android compilation and SDK-dependent guest TestApp tests were not
run.

Graphical Globbert verification is blocked by this session: both the newly
built executable and the user's existing downloaded executable exit with
`0xc0000409` immediately before window creation. Headless launch loads and
starts Globbert, then stops at its first window-dependent call, as expected.
Consequently the graphics and drag behavior require an in-game check on a
normal desktop session. The user's IPA and log are diagnostic input and are
not included in the source archive or patch.

## Globbert sustained holds

Globbert's SQ1/SQ3/SQ5 recognizers call a shared `squishStopped` delegate
callback when rejecting a press. Sending the recognized sibling's target
action synchronously from `setState:` started its hold before that cleanup,
so the same input event could immediately stop it again. Target actions are
now retained, coalesced and dispatched after the complete input event,
including all sibling recognizers and views. Failed or disabled recognizers
do not deliver queued actions.

Fresh recognizers are no longer reset before their first touch, and adding
another finger to a live custom sequence preserves its state and initial
position. Each custom recognizer tracks its active touches until release or
cancellation; failed recognizers do not receive later move/end callbacks.
Stationary touches no longer expire after ten seconds when another finger
arrives on a single-touch view. Explicit release, cancellation and replacement
of the same physical finger still terminate the old touch.

Three regressions cover sibling cleanup before hold start, preserving a live
sequence until its last touch ends, and nested/coalesced action dispatch.
Visual gameplay verification retains the limitation described above.
