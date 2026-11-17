//! The tone Plates rings once when a timer runs out, so a reminder reaches you while you look
//! elsewhere (user, 2026-11-17). Plates builds it as it first plays, rather than shipping a sound
//! file: one soft note, A5, 400ms long, at 28% of full scale. Each OS plays it through its own
//! player, which `native::play_sound` calls.

use std::sync::LazyLock;

const RATE: u32 = 44_100;

/// A note: when it starts, in seconds, its pitch in Hz, and how fast it fades, as the seconds its
/// level takes to fall to 1/e.
struct Note {
    start: f32,
    hz: f32,
    fade: f32,
}

const NOTES: [Note; 1] = [Note {
    start: 0.,
    hz: 880.,
    fade: 0.11,
}];

/// Each note's partials: a multiple of its pitch, its level, and its own fade in seconds. The
/// octave above dies away first, so a note starts bright and ends round.
const PARTIALS: [(f32, f32, f32); 2] = [(1., 1., 10.), (2., 0.12, 0.08)];

const LENGTH: f32 = 0.4;
/// The loudest sample as a share of full scale, which keeps the tone soft at any OS volume.
const PEAK: f32 = 0.28;
/// How long a note takes to reach its level, so it starts without a click.
const ATTACK: f32 = 0.006;
/// How long the tone's last samples take to fade to silence, so it ends without a click.
const TAIL: f32 = 0.03;

/// The tone as a WAV file of 16-bit mono samples, which both OSes' players read from memory.
pub static WAV: LazyLock<Vec<u8>> = LazyLock::new(|| wav(&samples()));

pub fn play() {
    crate::native::play_sound(&WAV);
}

fn samples() -> Vec<f32> {
    let n = (RATE as f32 * LENGTH) as usize;
    let mut out = vec![0f32; n];
    for note in &NOTES {
        let first = (RATE as f32 * note.start) as usize;
        for (i, sample) in out.iter_mut().enumerate().skip(first) {
            let t = (i - first) as f32 / RATE as f32;
            let level = (t / ATTACK).min(1.) * (-t / note.fade).exp();
            let wave: f32 = PARTIALS
                .iter()
                .map(|&(mult, amp, fade)| {
                    amp * (-t / fade).exp() * (std::f32::consts::TAU * note.hz * mult * t).sin()
                })
                .sum();
            *sample += level * wave;
        }
    }
    let tail = (RATE as f32 * TAIL) as usize;
    for k in 0..tail {
        out[n - 1 - k] *= k as f32 / tail as f32;
    }
    let top = out.iter().fold(0f32, |m, s| m.max(s.abs()));
    out.iter().map(|s| s / top * PEAK).collect()
}

// A WAV file is a 44-byte header, which says the samples are PCM and how they're laid out, and
// then the samples, little-endian.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    // Bytes per second, bytes per sample, and bits per sample.
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s * f32::from(i16::MAX)) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tone_is_a_soft_wav_that_starts_and_ends_quiet() {
        let s = samples();
        assert_eq!(s.len(), (RATE as f32 * LENGTH) as usize);
        let top = s.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!((top - PEAK).abs() < 1e-4);
        assert!(s[0].abs() < 0.01 && s[s.len() - 1].abs() < 0.01);
        let file = wav(&s);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(file.len(), 44 + s.len() * 2);
    }
}
