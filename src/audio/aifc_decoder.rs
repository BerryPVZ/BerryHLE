/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Apple IMA4 in AIFF-C: each 34-byte channel packet expands to 64 samples.
use super::{decode_ima4, symphonia_formats::SymphoniaDecodedToPcm};

pub fn decode_aifc_ima4(bytes: &[u8]) -> Result<SymphoniaDecodedToPcm, ()> {
    fn be32(bytes: &[u8]) -> Result<u32, ()> {
        Ok(u32::from_be_bytes(
            bytes.get(..4).ok_or(())?.try_into().map_err(|_| ())?,
        ))
    }
    if bytes.get(..4) != Some(b"FORM") || bytes.get(8..12) != Some(b"AIFC") {
        return Err(());
    }
    let end = 8usize.checked_add(be32(&bytes[4..])? as usize).ok_or(())?;
    let bytes = bytes.get(..end).ok_or(())?;
    let (mut comm, mut sound) = (None, None);
    let mut pos = 12usize;
    while pos < end {
        let header = bytes.get(pos..pos.checked_add(8).ok_or(())?).ok_or(())?;
        let len = be32(&header[4..])? as usize;
        let start = pos + 8;
        let chunk_end = start.checked_add(len).ok_or(())?;
        let chunk = bytes.get(start..chunk_end).ok_or(())?;
        match &header[..4] {
            b"COMM" => comm = Some(chunk),
            b"SSND" => sound = Some(chunk),
            _ => {}
        }
        pos = chunk_end.checked_add(len & 1).ok_or(())?;
    }
    let comm = comm.ok_or(())?;
    if comm.get(18..22) != Some(b"ima4") || comm.get(6..8) != Some(&[0, 16]) {
        return Err(());
    }
    let channels = u16::from_be_bytes(comm[..2].try_into().map_err(|_| ())?) as usize;
    if !(1..=2).contains(&channels) {
        return Err(());
    }
    // For IMA4, AIFF-C's numSampleFrames counts compressed packet groups.
    let packets = be32(&comm[2..])? as usize;
    let exponent = u16::from_be_bytes(comm[8..10].try_into().map_err(|_| ())?);
    if exponent & 0x8000 != 0 || exponent == 0 || exponent == 0x7fff {
        return Err(());
    }
    let mantissa = u64::from_be_bytes(comm[10..18].try_into().map_err(|_| ())?);
    let rate = (mantissa as f64) * 2f64.powi(exponent as i32 - 16383 - 63);
    if !rate.is_finite() || !(1.0..=384000.0).contains(&rate) {
        return Err(());
    }
    let sound = sound.ok_or(())?;
    let offset = be32(sound)? as usize;
    let start = 8usize.checked_add(offset).ok_or(())?;
    let packet_size = 34 * channels;
    let encoded_len = packets.checked_mul(packet_size).ok_or(())?;
    let encoded = sound
        .get(start..start.checked_add(encoded_len).ok_or(())?)
        .ok_or(())?;
    let decoded_len = packets.checked_mul(64 * channels * 2).ok_or(())?;
    let mut pcm = Vec::new();
    pcm.try_reserve_exact(decoded_len).map_err(|_| ())?;
    for group in encoded.chunks_exact(packet_size) {
        if group.chunks_exact(34).any(|packet| packet[1] & 0x7f > 88) {
            return Err(());
        }
        let left = decode_ima4(group[..34].try_into().map_err(|_| ())?);
        let right = if channels == 2 {
            Some(decode_ima4(group[34..].try_into().map_err(|_| ())?))
        } else {
            None
        };
        for i in 0..64 {
            pcm.extend_from_slice(&left[i].to_le_bytes());
            if let Some(right) = &right {
                pcm.extend_from_slice(&right[i].to_le_bytes());
            }
        }
    }
    Ok(SymphoniaDecodedToPcm {
        bytes: pcm,
        sample_rate: rate.round() as u32,
        channels: channels as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(channels: u16) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(tag);
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(data);
            if data.len() & 1 != 0 {
                out.push(0);
            }
        }
        let mut bytes = b"FORM\0\0\0\0AIFC".to_vec();
        chunk(&mut bytes, b"JUNK", &[42]);
        let mut comm = Vec::new();
        comm.extend_from_slice(&channels.to_be_bytes());
        comm.extend_from_slice(&1u32.to_be_bytes());
        comm.extend_from_slice(&16u16.to_be_bytes());
        comm.extend_from_slice(&[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
        comm.extend_from_slice(b"ima4");
        chunk(&mut bytes, b"COMM", &comm);
        let mut sound = vec![0, 0, 0, 3, 0, 0, 0, 0, 9, 9, 9];
        for channel in 0..channels {
            let mut packet = [0; 34];
            packet[..2].copy_from_slice(&(channel * 128).to_be_bytes());
            sound.extend_from_slice(&packet);
        }
        chunk(&mut bytes, b"SSND", &sound);
        let len = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&len.to_be_bytes());
        bytes
    }
    #[test]
    fn decodes_packet_groups_and_sound_offset() {
        for channels in [1, 2] {
            let pcm = decode_aifc_ima4(&fixture(channels)).unwrap();
            assert_eq!(pcm.sample_rate, 44100);
            assert_eq!(pcm.channels, channels as u32);
            assert_eq!(pcm.bytes.len(), 64 * channels as usize * 2);
            for frame in pcm.bytes.chunks_exact(channels as usize * 2) {
                assert_eq!(&frame[..2], &[0, 0]);
                if channels == 2 {
                    assert_eq!(&frame[2..], &[128, 0]);
                }
            }
        }
    }
    #[test]
    fn rejects_truncated_files_and_unsupported_channels() {
        let bytes = fixture(2);
        for len in 0..bytes.len() {
            assert!(decode_aifc_ima4(&bytes[..len]).is_err());
        }
        assert!(decode_aifc_ima4(&fixture(3)).is_err());
        let mut bad_index = fixture(1);
        let ssnd = bad_index.windows(4).position(|w| w == b"SSND").unwrap();
        bad_index[ssnd + 8 + 8 + 3 + 1] = 127;
        assert!(decode_aifc_ima4(&bad_index).is_err());
        let mut oversized = fixture(1);
        oversized[32..36].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode_aifc_ima4(&oversized).is_err());
    }
}
