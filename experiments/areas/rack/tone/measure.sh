#!/bin/sh
# Measures a sound the way Plates' tone was measured against macOS' alert sounds:
# loudness, peak and average level, the energy in 2.5-6 kHz (where the ear is most
# sensitive, so the more there, the sharper), the strike's energy above 6 kHz in its
# first 20ms, and the level of its last 250ms. Needs ffmpeg.
#   sh measure.sh <sound>...
for f in "$@"; do
  rms() { ffmpeg -hide_banner -t "${2:-99}" -i "$f" -af "aformat=channel_layouts=mono${1:+,$1},astats=measure_perchannel=none:measure_overall=RMS_level" -f null - 2>&1 | grep "RMS level dB" | sed 's/.*: //'; }
  lufs=$(ffmpeg -hide_banner -i "$f" -af ebur128 -f null - 2>&1 | grep -A1 "Integrated loudness" | grep "I:" | tail -1 | awk '{print $2}')
  peak=$(ffmpeg -hide_banner -i "$f" -af "astats=measure_perchannel=none:measure_overall=Peak_level" -f null - 2>&1 | grep "Peak level dB" | sed 's/.*: //')
  all=$(rms "" 1.0)
  band=$(rms "highpass=f=2500:poles=2,highpass=f=2500:poles=2,lowpass=f=6000:poles=2,lowpass=f=6000:poles=2" 1.0)
  strike=$(rms "highpass=f=6000:poles=2,highpass=f=6000:poles=2" 0.02)
  end=$(ffmpeg -hide_banner -sseof -0.25 -i "$f" -af "astats=measure_perchannel=none:measure_overall=RMS_level" -f null - 2>&1 | grep "RMS level dB" | sed 's/.*: //')
  printf "%s: %s LUFS, peak %.1f dB, 2.5-6kHz %.1f dB under the whole, strike above 6kHz %.1f dB, last 250ms %.1f dB\n" \
    "$(basename "$f")" "$lufs" "$peak" "$(echo "$all - $band" | bc -l)" "$strike" "$end"
done
# The onset: from the first moment the sound passes -60 dBFS, its energy above 6 kHz and above
# 10 kHz in its first 5ms, against its whole.
for f in "$@"; do
  trimmed="ffmpeg -hide_banner -i $f -af aformat=channel_layouts=mono,silenceremove=start_periods=1:start_threshold=-60dB"
  whole=$(ffmpeg -hide_banner -i "$f" -af "aformat=channel_layouts=mono,astats=measure_perchannel=none:measure_overall=RMS_level" -f null - 2>&1 | grep "RMS level dB" | sed 's/.*: //')
  hi6=$(eval "$trimmed,highpass=f=6000:poles=2,highpass=f=6000:poles=2,atrim=end=0.005,astats=measure_perchannel=none:measure_overall=RMS_level -f null -" 2>&1 | grep "RMS level dB" | sed 's/.*: //')
  hi10=$(eval "$trimmed,highpass=f=10000:poles=2,highpass=f=10000:poles=2,atrim=end=0.005,astats=measure_perchannel=none:measure_overall=RMS_level -f null -" 2>&1 | grep "RMS level dB" | sed 's/.*: //')
  printf "%s onset: first 5ms above 6kHz %.1f dB and above 10kHz %.1f dB against the whole\n" \
    "$(basename "$f")" "$(echo "$hi6 - $whole" | bc -l)" "$(echo "$hi10 - $whole" | bc -l)"
done
