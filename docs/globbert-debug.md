# Optional Globbert debug menu

Add this line to touchHLE_options.txt beside the executable:

    com.andydev.globbert: --globbert-debug

If that file already has a Globbert entry, append --globbert-debug to its
existing line. The menu is off by default and ignored for other apps.
--no-globbert-debug disables it explicitly.

Tap Globbert debug at the top right to open or collapse the overlay. Filter
by animation name, switch between all clips and endings, use Prev/Next, and
tap a clip to play it. Stop interrupts the current clip. Return to game runs
Globbert's normal game-entry routine. A selection from the welcome screen
opens the game automatically. Closing the panel leaves gameplay accessible.

The browser reads animated plist resources from the installed game. It finds
482 sequences in the tested Globbert 1.1 IPA, including 20 clips under the
DEX/Exit/TheEnd/-End ending filter. All clips remain available in All animations.
Both structured Frames dictionaries and flat frame sequences are supported.
No game assets are included with the emulator.

Playback uses Globbert's own frame engine and sound callbacks. Existing
animation threads are cancelled and allowed to finish before a new clip is
started, and debug requests clear an active squish hold. This is a developer
preview: clips can bypass the normal game progression and prerequisites.

Verification: scripted real pointer taps open the overlay, search and select
clips. DEX1 launches; TheEnd advances to frame 12; Headphones advances to frame
45. Return to game resumes a normal D001C-P2 animation, collapse restores a
34-point-high control, and the ending filter returns 20 entries. Rendered
frames were inspected for readable labels and visible animation content.
These are representative clips, not an exhaustive run of all 482 sequences.
Temporary automation and framebuffer capture code are excluded from builds.
