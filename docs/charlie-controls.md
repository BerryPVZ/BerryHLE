# Charlie the Steak controls

Charlie 1.1.2 uses awakeAfterUsingCoder: to replace placeholder custom views
with populated nested nibs. Both decoders now call that hook and cache its
replacement; NSObject supplies the default identity implementation.
Replacement objects are retained for the decoding cache.

UINib removes nil First Responder placeholders from its returned top-level
array. Otherwise guest fast enumeration stops before the real view, making
loadInstanceFromNib return nil. This affected the level selector and chef hat.

Nib-created gesture recognizers now survive initWithCoder:, event connections
use addTarget:action: for recognizers, and outlet collections assign through
the owning setter instead of mutating an immutable snapshot. This attaches
the level selector's saved tap and pan recognizers to their view.

Controlled guest verification on Windows: the chef hat has six subviews,
a non-nil hatButton, isHidden=false and a live superview. Its original handler
changes isOpened from false to true. The level button's original handler
adds a LevelSelectorView with 16 subviews and a close button to the game view;
the close handler closes it. No visibility flags or game unlocks are forced.
The temporary probe calls the original handlers and is not included in the
production build; these checks do not constitute manual visual gameplay QA.

The decoder replacement contract is described in Apple's documentation:
https://developer.apple.com/documentation/objectivec/nsobject-swift.class/awakeafter(using:)?language=objc
