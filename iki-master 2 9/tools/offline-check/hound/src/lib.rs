//! Minimal offline stand-in for the `hound` crate: PCM 8/16/24/32-bit and float WAV reading only.
use std::io::{Read, Seek};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleFormat { Float, Int }
#[derive(Clone, Copy, Debug)]
pub struct WavSpec { pub channels: u16, pub sample_rate: u32, pub bits_per_sample: u16, pub sample_format: SampleFormat }
pub struct WavReader<R> { spec: WavSpec, data: Vec<u8>, _r: std::marker::PhantomData<R> }
impl<R: Read + Seek> WavReader<R> {
    pub fn new(mut r: R) -> Result<Self, Error> {
        let mut all = Vec::new();
        r.read_to_end(&mut all).map_err(|e| Error(e.to_string()))?;
        if all.len() < 12 || &all[0..4] != b"RIFF" || &all[8..12] != b"WAVE" { return Err(Error("not a wav".into())); }
        let mut i = 12; let mut spec = None; let mut data = None;
        while i + 8 <= all.len() {
            let id = &all[i..i + 4]; let n = u32::from_le_bytes(all[i + 4..i + 8].try_into().unwrap()) as usize;
            let body = &all[i + 8..(i + 8 + n).min(all.len())];
            if id == b"fmt " {
                let tag = u16::from_le_bytes(body[0..2].try_into().unwrap());
                spec = Some(WavSpec {
                    channels: u16::from_le_bytes(body[2..4].try_into().unwrap()),
                    sample_rate: u32::from_le_bytes(body[4..8].try_into().unwrap()),
                    bits_per_sample: u16::from_le_bytes(body[14..16].try_into().unwrap()),
                    sample_format: if tag == 3 { SampleFormat::Float } else { SampleFormat::Int },
                });
            } else if id == b"data" { data = Some(body.to_vec()); }
            i += 8 + n + (n & 1);
        }
        match (spec, data) { (Some(spec), Some(data)) => Ok(Self { spec, data, _r: Default::default() }), _ => Err(Error("bad wav".into())) }
    }
    pub fn spec(&self) -> WavSpec { self.spec }
    pub fn samples<S: Sample>(&mut self) -> impl Iterator<Item = Result<S, Error>> + '_ {
        let bytes = (self.spec.bits_per_sample as usize).div_ceil(8);
        let float = self.spec.sample_format == SampleFormat::Float;
        self.data.chunks_exact(bytes).map(move |c| Ok(S::from_bytes(c, float)))
    }
}
#[derive(Debug)] pub struct Error(String);
impl std::fmt::Display for Error { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) } }
pub trait Sample: Sized { fn from_bytes(c: &[u8], float: bool) -> Self; }
fn int_of(c: &[u8]) -> i32 { match c.len() { 1 => c[0] as i32 - 128, 2 => i16::from_le_bytes([c[0], c[1]]) as i32, 3 => (((c[0] as i32) | ((c[1] as i32) << 8) | ((c[2] as i32) << 16)) << 8) >> 8, _ => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) } }
impl Sample for i16 { fn from_bytes(c: &[u8], _: bool) -> Self { int_of(c) as i16 } }
impl Sample for i32 { fn from_bytes(c: &[u8], _: bool) -> Self { int_of(c) } }
impl Sample for f32 { fn from_bytes(c: &[u8], float: bool) -> Self { if float { f32::from_le_bytes([c[0], c[1], c[2], c[3]]) } else { int_of(c) as f32 } } }
