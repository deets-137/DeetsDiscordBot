//! `/vc tone`: a 440 Hz sine at -12 dBFS (`bot/voice.py` `ToneSource`), as f32 LE stereo for
//! songbird's `RawAdapter`. It ends after its seconds (a read returns 0: the track ends).

use std::io::{Read, Seek, SeekFrom};

use symphonia::core::io::MediaSource;

const RATE: u64 = deets_tap::RATE as u64;
const BLOCK: u64 = 960;

pub struct Tone {
    frame: u64,
    total: u64,
    pending: Vec<u8>,
}

impl Tone {
    pub fn new(secs: u64) -> Self {
        Self { frame: 0, total: secs * RATE, pending: Vec::new() }
    }
}

impl Read for Tone {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            let n = (self.total - self.frame).min(BLOCK);
            for i in 0..n {
                let t = (self.frame + i) as f32 / RATE as f32;
                let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.25;
                self.pending.extend_from_slice(&s.to_le_bytes());
                self.pending.extend_from_slice(&s.to_le_bytes());
            }
            self.frame += n;
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Seek for Tone {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

impl MediaSource for Tone {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}
