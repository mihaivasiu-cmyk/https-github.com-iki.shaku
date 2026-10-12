//! Minimal 16-bit PCM WAV writer/reader (for exporting loops) with no dependencies.

/// Encode mono (`channels = 1`) or interleaved samples as a 16-bit PCM WAV file.
pub fn encode_i16(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

/// Decode what [`encode_i16`] writes: (samples, channels, sample_rate).
pub fn decode_i16(b: &[u8]) -> Option<(Vec<f32>, u16, u32)> {
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let channels = u16::from_le_bytes([b[22], b[23]]);
    let sr = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
    let n = u32::from_le_bytes([b[40], b[41], b[42], b[43]]) as usize;
    let data = b.get(44..44 + n)?;
    Some((data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(), channels, sr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let x: Vec<f32> = (0..100).map(|i| (i as f32 / 50.0 - 1.0) * 0.9).collect();
        let w = encode_i16(&x, 1, 48000);
        assert_eq!(w.len(), 44 + 200);
        let (y, ch, sr) = decode_i16(&w).unwrap();
        assert_eq!((ch, sr), (1, 48000));
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-3));
        assert!(decode_i16(b"nope").is_none());
    }
}
