# BerryHLE update

## Upstream integration

Merged Reborn trunk 3e5ef14 into the Berry branch based on 4907852, preserving
both histories. The upstream side contains 22 additional commits. Conflicts
were resolved by using the upstream audio-unit connection lifecycle and UIKit
subview layout callbacks, retaining the Berry confirmation-button regressions
and all existing game-specific changes. Duplicate autoresizing helpers were
consolidated rather than running both implementations.

## Branding and landscape launcher

The launcher, window titles, Android/iOS display metadata, macOS app bundle,
Windows installer and release artifact names now use BerryHLE. Existing bundle
identifiers, executable names, options, resources and sandbox paths stay stable
so installations and saved data remain compatible. Upstream attribution remains.

On a landscape host display the temporary launcher environment uses landscape
screen coordinates and a proportional logical resolution capped at 1280x540.
The grid selects columns and rows for the available space, keeping icons square.
Portrait retains the classic 4x4 arrangement. These settings apply only to the
launcher: game options are parsed separately when the selected app starts.
Wallpaper is centered in a calculated aspect-fit rectangle. Its whole image is
visible with proportional dimensions; portrait wallpaper has side margins on a
wide screen. This does not rely on CALayer contentsGravity, which the current
compositor does not implement. Lock, dock and Settings share the wide layout.

## PetsWar projectile collisions

The real ARM game's animated projectile path creates a child sprite and sets
the parent's contentSize; its inherited textureRect remains zero. Collision
code uses that zero-size rectangle at the projectile's position. Previously
CGRectIntersectsRect used strict positive-area overlap, so these shots could
never collide even while inside an enemy. Ordinary 13x13 bullet sprites still
hit, explaining why only some projectile types failed.

CGRectIntersection and CGRectIntersectsRect now use closed extents, preserve
non-null zero-area intersections, standardize negative dimensions and reject
null rectangles. Point/edge/corner, separated and reversed-size regressions
cover the behavior. Reference: Apple's Swift Foundation geometry implementation:
https://github.com/swiftlang/swift-corelibs-foundation/blob/main/Sources/Foundation/NSGeometry.swift

## Chinese PvZ HD sound effects

The merged upstream AudioUnit connection fix applies generally, including
com.popcap.ios.chs.PvZiPad (1017.0.0, PvZ-iPad_ZH). It tracks hand-made mixer
connections, starts upstream voices, pumps output-unit render notifications,
uses the documented notification flag values and propagates per-bus gain and
playback rate. No English-only bundle check is involved.
Audio Unit render/input model and properties:
https://developer.apple.com/library/archive/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/TheAudioUnit/TheAudioUnit.html
Render notification timing:
https://developer.apple.com/documentation/audiotoolbox/audiounitaddrendernotify(_:_:_:) The supplied Chinese
IPA was tested with its own scratch copy of saved data.

## Validation

441 Rust library tests passed. The ARM TestApp integration suite was not run
locally; it requires the custom SDK/toolchain and is covered by upstream macOS
CI. Windows runtime checks and their limits are in the packaged validation.txt.
No game IPAs, assets, saves or temporary diagnostic controls are shipped.
