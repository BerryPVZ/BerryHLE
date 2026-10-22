# PetsWar graphics and particle crash

PetsWar 1.0 (`com.iphogame.petswar`) has two separate issues.

Its character configuration plists contain geometry such as
`{{0.0f, 0.0f}, {40.0f, 53.0f}}`, anchors such as `{0.5f, 0.0f}`, and shadow
offsets such as `{3.0f, -4.0f}`. The strict Rust float parser rejected the
suffixes, making the geometry conversion return zeros. Character textures
were present on the GPU with linear filtering, but their sprite frames had
zero area. Geometry parsing now accepts a single `f`/`F` suffix after a valid
number; malformed numbers still fail. A regression test covers the actual
PetsWar rectangle, anchor and offset forms.

The reported crash was an assertion in GLES1-on-GL2's EnableClientState for
`GL_POINT_SIZE_ARRAY_OES` (0x8b9c). The backend now tracks the array pointer,
stride, type, enable flag and captured VBO binding. For point draws it reads
float or fixed-point sizes, draws each indexed/non-indexed point at its own
size, and restores scalar point size and buffer binding. This also allows
the particle effects to render instead of merely ignoring the extension.
Point-size arrays are not forwarded to the desktop driver as an unsupported
client-state enum. Splitting points into scalar draws has some overhead.

Verification used the real ARM guest, New Game and its confirmation screen.
The baseline battle showed shadows, projectiles and background with most
character bodies missing. With both fixes the same opening battle showed
the cats, dogs, elephants and other defenders/enemies, animated projectiles
and point-size particle effects. The app remained running after the original
crash location. Texture readback during diagnosis confirmed the sprites were
uploaded and the minification filters were already linear, so no texture-filter
override was added.

The texture and particle diagnostics used during investigation are excluded
from the shipped executable. No game textures or other game files are bundled.
This does not certify every level or every possible crash path.
