# Buddyman Kick 2 audio and presentation

Tested application: com.crazylion.buddyman.tm.kick2, supplied 1.6 IPA.

## Silent audio

The game's LLThreadProxy implements methodSignatureForSelector: and
forwardInvocation:, but missing Objective-C methods previously returned zero
without entering this forwarding path. Sound requests never reached the audio
worker. Full forwarding now builds an NSInvocation from the original ARM
register and stack arguments and dispatches through the guest proxy. The proxy
continues to choose its worker thread; gain, pitch and pan are preserved.
Scalar return values up to eight bytes are supported by the existing invocation
implementation; aggregate return forwarding remains unsupported.

The game's CAF files also contain variable-size AAC packets. ExtAudioFileRead
previously returned zero frames for these, and FileLengthFrames reported zero.
Extended Audio File Services now keeps a decoded PCM copy for frame counts,
reads and seeking. The original AAC format and AudioFile compressed packet
access remain intact. Existing client-format conversion operates on decoded PCM.
Decode failure returns a format error rather than successful empty audio.

## Lag

The supplied per-app profile uses landscape-right and zero presentation
rotation. Buddyman renders its own landscape image and rotates its layer in
the opposite direction to normal UIKit autorotation. With that explicit profile,
the fullscreen detector accepts its inverse quarter-turn. All existing checks
for screen coverage, visibility, opacity and nested rotations still apply.
This lets the GPU display the renderbuffer directly instead of reading it into
RAM and composing it again each frame. Other apps retain their existing rules.

The Windows package is built with the optimized release profile. Earlier local
packages used unoptimized Rust debug builds.

## Validation

Library regressions cover forwarding filename/gain/pitch/pan across ARM
registers and stack, plus mono/stereo AAC CAF decoding, nonzero PCM, offsets,
EOF and decoded-file cloning. Fixtures are generated sine waves.
Runtime sound is checked using OpenAL's WAV backend; no game audio or assets
are included in the package. See the packaged validation.txt for measurements.

Replace touchHLE_default_options.txt as well as touchHLE.exe when updating.
A user override for this bundle can override the supplied presentation profile.
