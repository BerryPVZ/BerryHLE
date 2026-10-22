The two AAC CAF fixtures are generated sine waves, not game assets.
They are dedicated to the public domain (CC0).

Generate ADTS AAC using FFmpeg:

    ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.12 -ac 1 -c:a aac -f adts mono.aac
    ffmpeg -f lavfi -i sine=frequency=660:sample_rate=44100:duration=0.12 -ac 2 -c:a aac -f adts stereo.aac

The CAF wrapper contains a big-endian desc chunk (44100 Hz, aac, variable
packet bytes, 1024 frames per packet, 1/2 channels, zero bits per sample),
a pakt chunk listing each payload size as a CAF variable-length integer,
and a data chunk with edit count zero followed by AAC payloads without
the seven-byte ADTS headers. No encoder priming or remainder is trimmed.
Tests check decoding, PCM format, nonzero samples, offsets, EOF, cloning,
and preservation of the original compressed packet API.
