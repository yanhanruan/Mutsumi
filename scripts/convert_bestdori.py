"""
Convert Bestdori chart data (ID 162425 - 春日影, level 27) to Note[] format.
BPM = 194 (Bestdori chart), offsetMs and timing derived from BPM.
"""
import json

# Load chart data
with open('d:\\DevProjects\\Mutsumi\\scripts\\bestdori_chart_162425.json', 'r', encoding='utf-8') as f:
    data = json.load(f)

# Chart config
BPM = 194  # from Bestdori chart (BPM entry at beat=0)
# The original song config in rhythmSongs.ts uses BPM=97 and offsetMs=2000
# Since Bestdori BPM is 194 (double 97), beats remain compatible
# We'll preserve the original offsetMs
OFFSET_MS = 2000

# Lane to direction mapping (7-lane Bestdori → 4-direction)
# Bestdori lanes 0-6 map to our 4 directions
# Based on BanG Dream layout: 0=far left, 1=left, 2=center-left, 3=center, 4=center-right, 5=right, 6=far right
LANE_TO_DIR = {
    0: 'left',
    1: 'left',
    2: 'down',
    3: 'up',
    4: 'right',
    5: 'right',
    6: 'right',
}

def beat_to_ms(beat):
    """Convert beat number to milliseconds."""
    return beat * (60000.0 / BPM) + OFFSET_MS

def nearest_lane(lane_val):
    """Round fractional lane to nearest integer lane."""
    return max(0, min(6, round(lane_val)))

def lane_to_dir(lane_val):
    """Convert lane value to direction string."""
    lane_int = nearest_lane(lane_val)
    return LANE_TO_DIR.get(lane_int, 'right')

chart = data['chart']
notes = []

# Process each chart entry
for entry in chart:
    etype = entry.get('type')
    
    if etype == 'BPM':
        continue  # We'll use our own BPM
    
    elif etype in ('Single', 'Directional'):
        beat = entry['beat']
        lane = entry['lane']
        time_ms = round(beat_to_ms(beat))
        direction = lane_to_dir(lane)
        notes.append({
            'time': time_ms,
            'direction': direction,
            'holdMs': 0
        })
    
    elif etype == 'Slide':
        # Extract first and last non-hidden connection points as notes
        conns = entry.get('connections', [])
        if not conns:
            continue
        
        # Start point
        start = conns[0]
        start_beat = start['beat']
        start_lane = start['lane']
        start_time = round(beat_to_ms(start_beat))
        start_dir = lane_to_dir(start_lane)
        notes.append({
            'time': start_time,
            'direction': start_dir,
            'holdMs': 0
        })
        
        # End point (if different from start)
        if len(conns) > 1:
            end = conns[-1]
            end_beat = end['beat']
            end_lane = end['lane']
            end_time = round(beat_to_ms(end_beat))
            end_dir = lane_to_dir(end_lane)
            
            # Only add if time is significantly different and direction changed
            if end_time - start_time > 50:
                notes.append({
                    'time': end_time,
                    'direction': end_dir,
                    'holdMs': 0
                })
    else:
        print(f"Unknown type: {etype}")

# Sort by time
notes.sort(key=lambda n: n['time'])

# Print summary
print(f"Total notes converted: {len(notes)}")
print(f"Time range: {notes[0]['time']}ms to {notes[-1]['time']}ms")
print(f"Duration: {(notes[-1]['time'] - notes[0]['time']) / 1000:.1f}s")

# Count directions
from collections import Counter
dir_counts = Counter(n['direction'] for n in notes)
print(f"Direction distribution: {dict(dir_counts)}")

# Output as TypeScript array
ts_lines = []
for i, n in enumerate(notes):
    sep = ',' if i < len(notes) - 1 else ''
    ts_lines.append(f"  {{ time: {n['time']}, direction: '{n['direction']}', holdMs: 0 }}{sep}")

ts_output = '[\n' + '\n'.join(ts_lines) + '\n]'
print("\n\n=== TypeScript Output ===")
print(ts_output)

# Save to a file for embedding
with open('d:\\DevProjects\\Mutsumi\\scripts\\converted_notes.txt', 'w', encoding='utf-8') as f:
    f.write(ts_output)

# Also save as a formatted .ts file
with open('d:\\DevProjects\\Mutsumi\\scripts\\haruhikage_bestdori_notes.ts', 'w', encoding='utf-8') as f:
    f.write('// Auto-converted from Bestdori chart ID 162425 (春日影, level 27)\n')
    f.write(f'// BPM={BPM}, Offset={OFFSET_MS}ms, Total notes: {len(notes)}\n')
    f.write('// Source: https://bestdori.com/community/charts/162425\n')
    f.write('// Converter author notes:\n')
    f.write('//   - Slide notes: start+end connection points only\n')
    f.write('//   - 7-lane → 4-direction: {0,1}→left, 2→down, 3→up, {4,5,6}→right\n\n')
    f.write('import { Note } from \'../../src/config/rhythmSongs\'\n\n')
    f.write(f'export const HARUHIKAGE_BESTDORI_NOTES: Note[] = {ts_output};\n')

print("\n\nSaved to scripts/haruhikage_bestdori_notes.ts")
print(f"Total: {len(notes)} notes, from {notes[0]['time']}ms to {notes[-1]['time']}ms")