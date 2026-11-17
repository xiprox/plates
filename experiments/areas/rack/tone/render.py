"""Renders Plates' tone, as src/plates/src/tone.rs builds it, to a WAV.

    python3 -I render.py <out.wav>
"""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(sys.argv[0])))
import synth

L, R = synth.chime(880.0, 2.4, synth.MODES, synth.THUD, seed=5, rise_s=synth.CONTACT)
L, R = synth.finish(L, R, -16.0)
synth.save24(sys.argv[1], L, R)
