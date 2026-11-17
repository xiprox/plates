"""The prototype of Plates' tone: a struck bar built from its modes, and saved as a 48 kHz,
24-bit stereo WAV. render.py renders the tone src/plates/src/tone.rs rebuilds, and
record/areas/rack/log.md says how the choices were measured.
"""
import math, random, struct

RATE = 48000

def chime(f0, length, modes, strike, seed, rise_s):
    """modes: (ratio, level, fade in s, beat in Hz, spread). strike: (level, Hz, q, fade in s)."""
    rnd = random.Random(seed)
    n = int(RATE * length)
    L = [0.0] * n; R = [0.0] * n
    for (ratio, amp, tau, beat, width) in modes:
        f = f0 * ratio
        ph1, ph2 = rnd.random() * 0.4, rnd.random() * 0.4
        w1 = 2 * math.pi * (f - beat / 2) / RATE
        w2 = 2 * math.pi * (f + beat / 2) / RATE
        decay = math.exp(-1 / (tau * RATE))
        g = amp
        for i in range(n):
            a = g * min(1.0, i / (rise_s * RATE))
            s1 = math.sin(w1 * i + ph1); s2 = math.sin(w2 * i + ph2)
            L[i] += a * ((0.5 + width) * s1 + (0.5 - width) * s2)
            R[i] += a * ((0.5 - width) * s1 + (0.5 + width) * s2)
            g *= decay
    amp, center, q, tau = strike
    w = 2 * math.pi * center / RATE
    r = math.exp(-w / (2 * q))
    b1, b2 = 2 * r * math.cos(w), -r * r
    for ch in (L, R):
        y1 = y2 = 0.0
        for i in range(int(RATE * tau * 8)):
            x = (rnd.random() * 2 - 1) * amp * math.exp(-i / (tau * RATE))
            y = x + b1 * y1 + b2 * y2
            ch[i] += y * (1 - r)
            y2, y1 = y1, y
    return L, R

def finish(L, R, peak_db, tail=0.25):
    n = len(L); f = int(RATE * tail)
    for k in range(f):
        g = 0.5 - 0.5 * math.cos(math.pi * k / f)
        L[n - 1 - k] *= g; R[n - 1 - k] *= g
    top = max(max(abs(v) for v in L), max(abs(v) for v in R))
    k = pow(10, peak_db / 20) / top
    return [v * k for v in L], [v * k for v in R]

def save24(path, L, R, seed=7):
    rnd = random.Random(seed)
    frames = bytearray()
    for l, r in zip(L, R):
        for v in (l, r):
            q = int(round(v * 8388607 + (rnd.random() - rnd.random())))
            frames += struct.pack('<i', max(-8388608, min(8388607, q)))[:3]
    fmt = struct.pack('<HHIIHHHHIH14s', 0xFFFE, 2, RATE, RATE * 6, 6, 24, 22, 24, 3,
                      1, bytes.fromhex('000000001000800000aa00389b71'))
    with open(path, 'wb') as fh:
        fh.write(b'RIFF' + struct.pack('<I', 4 + 8 + len(fmt) + 8 + len(frames)) + b'WAVE')
        fh.write(b'fmt ' + struct.pack('<I', len(fmt)) + fmt)
        fh.write(b'data' + struct.pack('<I', len(frames)) + frames)

# The shipped tone: one felt-mallet strike on a glass bar at A5. The glass bar's modes are
# 1 : 2.756 : 5.404 with a weak octave; the felt (`soft`) cuts the upper modes' levels and the
# strike's thud, and the note's own pair is in tune, since its 0.45 Hz beat made the ring hang.
SOFT, FADE = 0.14, 0.62
MODES = [
    (1.000, 1.00, 0.40 * FADE, 0.0, 0.12),
    (2.000, 0.06 * pow(SOFT, 0.6), 0.18 * FADE, 0.70, 0.18),
    (2.756, 0.12 * pow(SOFT, 0.8), 0.12 * FADE, 0.90, 0.20),
    (5.404, 0.03 * pow(SOFT, 1.6), 0.05 * FADE, 1.40, 0.24),
]
THUD = (0.009, 850, 0.7, 0.0055)
CONTACT = 0.0045
