//! The tone Plates rings once when a timer runs out: a nudge that something's waiting, for when
//! you're done with what you're thinking about (user, 2026-11-17). It's one strike of a felt
//! mallet on a glass bar, A5, which rings out over about 1.3s, about as loud as macOS' Glass.
//! Plates builds it as it first plays, at 48 kHz in 24-bit stereo, as macOS' own alert sounds
//! are. record/areas/rack/log.md says how each choice below was measured against them.

use std::f64::consts::TAU;
use std::sync::LazyLock;

const RATE: u32 = 48_000;
const LENGTH: f64 = 2.4;
const PITCH: f64 = 880.;

/// One of the bar's modes, the tones it rings with at once: its pitch as a multiple of the
/// note's, its level, how fast it fades (the seconds its level takes to fall to 1/e), and a pair
/// of tones it rings as, apart by `beat` Hz, mixed across the channels by `spread`. Higher modes
/// fade faster, so the note starts bright and ends round. The note's own pair is in tune, since
/// its beat made the ring swell back after it had faded.
struct Mode {
    ratio: f64,
    level: f64,
    fade: f64,
    beat: f64,
    spread: f64,
    /// Where each tone of the pair starts in its cycle, so the channels differ a little.
    phases: (f64, f64),
}

/// A glass bar's modes, which a felt mallet excites little above the note.
const MODES: [Mode; 4] = [
    Mode {
        ratio: 1.,
        level: 1.,
        fade: 0.248,
        beat: 0.,
        spread: 0.12,
        phases: (0.249_160_677_955_880_8, 0.296_714_795_704_291_77),
    },
    Mode {
        ratio: 2.,
        level: 0.018_442_851_602_644_3,
        fade: 0.1116,
        beat: 0.7,
        spread: 0.18,
        phases: (0.318_077_426_226_278_66, 0.376_980_113_510_820_17),
    },
    Mode {
        ratio: 2.756,
        level: 0.024_893_368_873_032_198,
        fade: 0.0744,
        beat: 0.9,
        spread: 0.2,
        phases: (0.295_959_429_895_972_3, 0.368_929_998_666_166_8),
    },
    Mode {
        ratio: 5.404,
        level: 0.001_290_999_612_185_100_7,
        fade: 0.031,
        beat: 1.4,
        spread: 0.24,
        phases: (0.011_602_091_313_445_896, 0.186_249_061_751_242_15),
    },
];

/// How long the felt stays on the bar, over which the note rises: long enough to round the
/// strike, and short of the 5ms or so past which a strike sounds like a swell.
const CONTACT: f64 = 0.0045;

/// The felt's thud: its level, a resonance near the note in Hz, how sharp that resonance is, and
/// how fast the thud fades, in seconds, so it's gone in a few ms.
const THUD: (f64, f64, f64, f64) = (0.009, 850., 0.7, 0.0055);

/// The tone's loudest sample, in dB below full scale.
const PEAK_DB: f64 = -16.;

/// How long the last of the ring takes to fade to silence, by when it's below -90 dBFS.
const TAIL: f64 = 0.25;

/// The tone as a WAV file of 24-bit stereo samples.
pub static WAV: LazyLock<Vec<u8>> = LazyLock::new(|| wav(&samples(), 24));

/// The same tone in 16 bits, for a player that refuses 24.
pub static WAV16: LazyLock<Vec<u8>> = LazyLock::new(|| wav(&samples(), 16));

/// Plays the tone, or its 16-bit copy if the OS's player refuses the 24-bit one. Whether
/// Windows' `PlaySoundW` reports a format it can't play, rather than returning as it starts, is
/// unverified until a timer runs out on Windows.
pub fn play() {
    if !crate::native::play_sound(&WAV) {
        crate::native::play_sound(&WAV16);
    }
}

/// A small fixed-seed random source, for the thud's noise and the dither, so every build makes
/// the same tone.
struct Noise(u64);

impl Noise {
    /// A value in [-1, 1).
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 52) as f64 - 1.
    }
}

fn samples() -> Vec<(f64, f64)> {
    let rate = f64::from(RATE);
    let n = (rate * LENGTH) as usize;
    let mut out = vec![(0., 0.); n];
    for m in &MODES {
        let hz = PITCH * m.ratio;
        let w1 = TAU * (hz - m.beat / 2.) / rate;
        let w2 = TAU * (hz + m.beat / 2.) / rate;
        let fall = (-1. / (m.fade * rate)).exp();
        let mut level = m.level;
        for (i, s) in out.iter_mut().enumerate() {
            let a = level * (i as f64 / (CONTACT * rate)).min(1.);
            let t1 = (w1 * i as f64 + m.phases.0).sin();
            let t2 = (w2 * i as f64 + m.phases.1).sin();
            s.0 += a * ((0.5 + m.spread) * t1 + (0.5 - m.spread) * t2);
            s.1 += a * ((0.5 - m.spread) * t1 + (0.5 + m.spread) * t2);
            level *= fall;
        }
    }
    // The thud rings a two-pole resonance with decaying noise, a little differently per channel.
    let (level, hz, q, fade) = THUD;
    let w = TAU * hz / rate;
    let r = (-w / (2. * q)).exp();
    let (b1, b2) = (2. * r * w.cos(), -r * r);
    let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
    for channel in 0..2 {
        let (mut y1, mut y2) = (0., 0.);
        for (i, s) in out.iter_mut().take((rate * fade * 8.) as usize).enumerate() {
            let x = noise.next() * level * (-(i as f64) / (fade * rate)).exp();
            let y = x + b1 * y1 + b2 * y2;
            if channel == 0 {
                s.0 += y * (1. - r);
            } else {
                s.1 += y * (1. - r);
            }
            (y2, y1) = (y1, y);
        }
    }
    let tail = (rate * TAIL) as usize;
    for k in 0..tail {
        let g = 0.5 - 0.5 * (std::f64::consts::PI * k as f64 / tail as f64).cos();
        let s = &mut out[n - 1 - k];
        (s.0, s.1) = (s.0 * g, s.1 * g);
    }
    let top = out
        .iter()
        .fold(0f64, |m, s| m.max(s.0.abs()).max(s.1.abs()));
    let k = 10f64.powf(PEAK_DB / 20.) / top;
    out.iter().map(|s| (s.0 * k, s.1 * k)).collect()
}

// A WAV file: a header that says how the samples are laid out, then the samples, little-endian.
// Samples deeper than 16 bits take the extensible format, which Windows defines for sample
// resolutions its basic format can't describe.
// (documented, https://learn.microsoft.com/windows/win32/api/mmreg/ns-mmreg-waveformatextensible)
fn wav(samples: &[(f64, f64)], bits: u16) -> Vec<u8> {
    let bytes = bits / 8;
    let block = 2 * bytes;
    let data = samples.len() as u32 * u32::from(block);
    let fmt: Vec<u8> = if bits > 16 {
        let mut f = Vec::new();
        f.extend_from_slice(&0xFFFEu16.to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&RATE.to_le_bytes());
        f.extend_from_slice(&(RATE * u32::from(block)).to_le_bytes());
        f.extend_from_slice(&block.to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f.extend_from_slice(&22u16.to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        // Front left and front right, then the PCM format's GUID.
        f.extend_from_slice(&3u32.to_le_bytes());
        f.extend_from_slice(&[
            1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71,
        ]);
        f
    } else {
        let mut f = Vec::new();
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&RATE.to_le_bytes());
        f.extend_from_slice(&(RATE * u32::from(block)).to_le_bytes());
        f.extend_from_slice(&block.to_le_bytes());
        f.extend_from_slice(&bits.to_le_bytes());
        f
    };
    let mut out = Vec::with_capacity(28 + fmt.len() + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(4 + 8 + fmt.len() as u32 + 8 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    out.extend_from_slice(&fmt);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    // Dither of one step either way, shaped as a triangle, so the quiet end of the ring fades
    // smoothly rather than in steps.
    let full = f64::from((1u32 << (bits - 1)) - 1);
    let mut noise = Noise(0xD1B5_4A32_D192_ED03);
    for &(l, r) in samples {
        for v in [l, r] {
            let d = (noise.next() + noise.next()) / 2.;
            let q = (v * full + d).round().clamp(-full - 1., full) as i32;
            out.extend_from_slice(&q.to_le_bytes()[..usize::from(bytes)]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level_db(s: &[(f64, f64)]) -> f64 {
        let e = s.iter().map(|x| x.0 * x.0 + x.1 * x.1).sum::<f64>() / (2 * s.len()) as f64;
        10. * e.log10()
    }

    #[test]
    fn the_tone_peaks_soft_and_fades_steadily_to_silence() {
        let s = samples();
        assert_eq!(s.len(), (f64::from(RATE) * LENGTH) as usize);
        let top = s.iter().fold(0f64, |m, x| m.max(x.0.abs()).max(x.1.abs()));
        assert!((20. * top.log10() - PEAK_DB).abs() < 1e-6);
        // Every 300ms while it's audible, the ring is quieter than it was, so it never swells
        // back, and it ends below -90 dBFS.
        let window = RATE as usize / 10;
        let levels: Vec<f64> = s.chunks(window).map(level_db).collect();
        for w in levels.windows(4).filter(|w| w[0] > -70.) {
            assert!(w[0] - w[3] > 5., "the ring hangs: {levels:?}");
        }
        assert!(level_db(&s[s.len() - RATE as usize / 4..]) < -90.);
    }

    #[test]
    fn both_files_are_stereo_wavs_of_the_tone() {
        for (file, bits) in [(&*WAV, 24usize), (&*WAV16, 16)] {
            assert_eq!(&file[..4], b"RIFF");
            assert_eq!(&file[8..16], b"WAVEfmt ");
            let samples = (f64::from(RATE) * LENGTH) as usize;
            let data = samples * 2 * bits / 8;
            assert_eq!(&file[file.len() - data - 8..file.len() - data - 4], b"data");
        }
    }
}
