"""Whether a ring hangs: its level every 100ms, and the least it falls over any 300ms while
it's above -70 dBFS. Near 0 dB means it stops fading for a moment. Reads 24-bit stereo WAVs.

    python3 -I plateau.py <sound.wav>...
"""
import math, sys

def read24(path):
    b = open(path, 'rb').read()
    i = b.index(b'data') + 8
    n = (len(b) - i) // 6
    L, R = [], []
    for k in range(n):
        o = i + 6 * k
        L.append(int.from_bytes(b[o:o + 3], 'little', signed=True) / 8388608)
        R.append(int.from_bytes(b[o + 3:o + 6], 'little', signed=True) / 8388608)
    return L, R

for path in sys.argv[1:]:
    L, R = read24(path)
    w = 4800
    lv = []
    for k in range(0, len(L) - w, w):
        e = sum(L[i] * L[i] + R[i] * R[i] for i in range(k, k + w)) / (2 * w)
        lv.append(10 * math.log10(e) if e > 0 else -150)
    least = min(lv[k] - lv[k + 3] for k in range(len(lv) - 3) if lv[k] > -70)
    print('%s: least fall over 300ms %.1f dB; every 100ms: %s' % (
        path.split('/')[-1], least, ' '.join('%.0f' % d for d in lv[:24])))
