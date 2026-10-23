// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/
//! Packet-based IMA4 conversion. Keep decoded leftovers and the sample-rate
//! cursor across calls: a 64-frame source packet rarely fits an output slice.

use super::*;

#[derive(Default)]
pub(super) struct Stream {
    pcm: Vec<u8>,
    position: f64,
    eof: bool,
}

impl Stream {
    fn render(&mut self, frames: usize, source: &PcmShape, dest: &PcmShape) -> Vec<u8> {
        let source_frames = self.pcm.len() / source.bytes_per_frame as usize;
        let step = source.sample_rate / dest.sample_rate;
        let mut samples = Vec::new();
        for _ in 0..frames {
            let index = self.position.floor() as usize;
            if index >= source_frames || (!self.eof && index + 1 >= source_frames) {
                break;
            }
            let next = (index + 1).min(source_frames - 1);
            let fraction = (self.position - index as f64) as f32;
            for channel in 0..source.channels as usize {
                let a = read_f32_sample(&self.pcm, source, index, channel);
                let b = read_f32_sample(&self.pcm, source, next, channel);
                samples.extend_from_slice(&(a + (b - a) * fraction).to_le_bytes());
            }
            self.position += step;
        }
        let consumed = (self.position.floor() as usize).min(source_frames);
        self.pcm.drain(..consumed * source.bytes_per_frame as usize);
        self.position -= consumed as f64;
        // Resampling is complete; let the existing PCM converter handle
        // channel count, destination bit depth and endianness.
        let intermediate = PcmShape {
            sample_rate: dest.sample_rate,
            channels: source.channels,
            bits: 32,
            is_float: true,
            is_big_endian: false,
            bytes_per_frame: source.channels * 4,
        };
        convert_pcm(&samples, &intermediate, dest)
    }
}

pub(super) fn fill(
    env: &mut Environment,
    converter_ref: AudioConverterRef,
    converter: &OpaqueAudioConverter,
    input: AudioConverterComplexInputDataProc,
    user_data: MutVoidPtr,
    packet_count: MutPtr<u32>,
    output: MutPtr<AudioBufferList>,
) -> OSStatus {
    // Apple documents input callbacks in packets, not decoded frames:
    // https://developer.apple.com/documentation/audiotoolbox/audioconvertercomplexinputdataproc
    let source = converter.source_format;
    let Some(dest_shape) = pcm_shape_from_asbd(&converter.dest_format) else {
        return kAudioConverterErr_FormatNotSupported;
    };
    let channels = source.channels_per_frame;
    if !(1..=2).contains(&channels)
        || source.frames_per_packet != 64
        || source.bytes_per_packet != channels * 34
        || !source.sample_rate.is_finite()
        || source.sample_rate <= 0.0
        || !dest_shape.sample_rate.is_finite()
        || dest_shape.sample_rate <= 0.0
        || packet_count.is_null()
        || output.is_null()
    {
        return kAudioConverterErr_InvalidInputSize;
    }
    let planar = converter.dest_format.format_flags & kAudioFormatFlagIsNonInterleaved != 0;
    let buffers = if planar { dest_shape.channels } else { 1 };
    let count: u32 = env.mem.read(output.cast());
    if count < buffers {
        return kAudioConverterErr_InvalidInputSize;
    }
    let stride = if planar {
        dest_shape.bits / 8
    } else {
        dest_shape.bytes_per_frame
    };
    let mut requested = env.mem.read(packet_count) as usize;
    for channel in 0..buffers {
        let buffer: AudioBuffer = env
            .mem
            .read(ConstPtr::from_bits(output.to_bits() + 4 + 12 * channel));
        if buffer.mData.is_null() {
            return kAudioConverterErr_InvalidInputSize;
        }
        requested = requested.min((buffer.mDataByteSize / stride) as usize);
    }
    if requested == 0 {
        env.mem.write(packet_count, 0);
        return 0;
    }
    let key = converter_ref.to_bits();
    let mut stream = env
        .framework_state
        .audio_toolbox
        .audio_converter
        .ima4
        .remove(&key)
        .unwrap_or_default();
    let source_shape = PcmShape {
        sample_rate: source.sample_rate,
        channels,
        bits: 16,
        is_float: false,
        is_big_endian: false,
        bytes_per_frame: channels * 2,
    };
    let need_frames = (stream.position
        + requested as f64 * source.sample_rate / dest_shape.sample_rate)
        .ceil() as usize
        + 1;
    let available = stream.pcm.len() / source_shape.bytes_per_frame as usize;
    if !stream.eof && need_frames > available {
        let packets = (need_frames - available).div_ceil(64) as u32;
        let Some(byte_size) = packets.checked_mul(source.bytes_per_packet) else {
            return kAudioConverterErr_InvalidInputSize;
        };
        let staging = env.mem.alloc(byte_size);
        let list = env.mem.alloc_and_write(AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: channels,
                mDataByteSize: byte_size,
                mData: staging,
            }],
        });
        let size = env.mem.alloc_and_write(packets);
        let descriptions = env
            .mem
            .alloc_and_write(MutPtr::<AudioStreamPacketDescription>::null());
        let status: OSStatus =
            input.call_from_host(env, (converter_ref, size, list, descriptions, user_data));
        let returned: AudioBufferList = env.mem.read(list);
        let supplied: u32 = env.mem.read(size);
        let data = returned_pcm(
            &env.mem,
            &returned,
            supplied.min(packets),
            source.bytes_per_packet,
        );
        let decoded = crate::audio::decode_ima4_packets(&data, channels);
        // The callback can replace mData with borrowed memory. Free only our
        // staging allocation, never the returned sample pointer.
        env.mem.free(descriptions.cast());
        env.mem.free(size.cast());
        env.mem.free(list.cast());
        env.mem.free(staging);
        if status != 0 {
            env.framework_state
                .audio_toolbox
                .audio_converter
                .ima4
                .insert(key, stream);
            env.mem.write(packet_count, 0);
            return status;
        }
        let Some(decoded) = decoded else {
            env.framework_state
                .audio_toolbox
                .audio_converter
                .ima4
                .insert(key, stream);
            env.mem.write(packet_count, 0);
            return kAudioConverterErr_InvalidInputSize;
        };
        stream.pcm.extend(decoded);
        stream.eof = supplied == 0;
    }
    let converted = stream.render(requested, &source_shape, &dest_shape);
    let produced = converted.len() / dest_shape.bytes_per_frame as usize;
    for channel in 0..buffers {
        let ptr = MutPtr::<AudioBuffer>::from_bits(output.to_bits() + 4 + 12 * channel);
        let mut buffer = env.mem.read(ptr);
        let bytes = if planar {
            pcm_plane(
                &converted,
                (dest_shape.bits / 8) as usize,
                dest_shape.channels as usize,
                channel as usize,
            )
        } else {
            converted.clone()
        };
        env.mem
            .bytes_at_mut(buffer.mData.cast(), bytes.len() as u32)
            .copy_from_slice(&bytes);
        buffer.mDataByteSize = bytes.len() as u32;
        env.mem.write(ptr, buffer);
    }
    env.mem.write(packet_count, produced as u32);
    env.framework_state
        .audio_toolbox
        .audio_converter
        .ima4
        .insert(key, stream);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_output_preserves_resampling_cursor_and_decoded_leftovers() {
        let source = PcmShape {
            sample_rate: 22000.0,
            channels: 1,
            bits: 16,
            is_float: false,
            is_big_endian: false,
            bytes_per_frame: 2,
        };
        let dest = PcmShape {
            sample_rate: 44100.0,
            channels: 2,
            bits: 32,
            is_float: true,
            is_big_endian: false,
            bytes_per_frame: 8,
        };
        let pcm: Vec<_> = (0..192i16)
            .flat_map(|x| (x * 100 - 9600).to_le_bytes())
            .collect();
        let mut whole = Stream {
            pcm: pcm.clone(),
            eof: true,
            ..Default::default()
        };
        let expected = whole.render(1000, &source, &dest);
        let mut split = Stream {
            pcm,
            eof: true,
            ..Default::default()
        };
        let mut actual = Vec::new();
        for count in [1, 17, 64, 3, 129, 1000] {
            actual.extend(split.render(count, &source, &dest));
        }
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            assert!(
                (f32::from_le_bytes(a.try_into().unwrap())
                    - f32::from_le_bytes(b.try_into().unwrap()))
                .abs()
                    < 1e-6
            );
        }
        assert!(split.pcm.is_empty());
    }
}
