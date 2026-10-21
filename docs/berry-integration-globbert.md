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

## Globbert body drag follow-up

The first hold follow-up did not resolve the reported body behavior. Its
sibling-cleanup explanation was speculative: runtime inspection shows each
squish recognizer attached to a separate view. A controlled guest run shows
that `isSquishing` stays true until release, but the old code leaves the
squish frame stuck at its initial value when the pointer moves.

Globbert's SQ1/SQ3/SQ5 custom recognizers enter Began on touch down and have
empty movement hooks. After an active custom recognizer processes a move,
UIKit now advances it to Changed and schedules its target action. Possible,
ended, cancelled and failed recognizers are not promoted. Actions remain
coalesced per input event, including when a subclass explicitly sets Changed.
This lets Globbert use the latest drag position and keep the resulting
squish until its own release callback runs.

The previous follow-up's active-touch tracking, deferred actions and removal
of the stationary-touch timeout remain in place. The new change does not
inject releases, repeat actions on a timer or alter the guest application.

A temporary, opt-in diagnostic fixture runs the original Globbert guest code
with scripted down/move/hold/up events. It initializes the game view and
interaction flags to isolate the squish behavior from the welcome sequence.
Baseline SQ1 frames: 1 -> 1 -> 1. Corrected SQ1 frames: 1 -> 4 -> 6; frame 6
and isSquishing=true persist during a two-second stationary hold, and the
release sets isSquishing=false. Temporary instrumentation is excluded from
the delivered executable. This is a controlled guest test, not a manual
visual gameplay test. The user's IPA is not included in any deliverable.

The exposed-view fixture also passed SQ3 (frames 3 -> 5 -> 6), SQ5
(frames 1 -> 3 -> 5), and a repeated SQ1 press. All four runs hit the intended
body view through normal hit testing, remained active during the stationary
hold, and stopped on Up. Exposing each view bypasses the overlapping tap
button during fixture setup; normal gameplay screen transitions still need
a user check.
