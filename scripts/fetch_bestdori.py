import json
from bestdori.post import Post
from collections import Counter

p = Post(id='162425')
info = p.get_details()

# Save full data
with open('d:\\DevProjects\\Mutsumi\\scripts\\bestdori_chart_162425.json', 'w', encoding='utf-8') as f:
    json.dump(info, f, indent=2, ensure_ascii=False)

print('Saved!')
print('Title:', info.get('title'))
print('Artists:', info.get('artists'))
print('Level:', info.get('level'))
print('Diff:', info.get('diff'))

chart = info.get('chart', [])
print('Total chart entries:', len(chart))

# Find BPM entries
bpms = [c for c in chart if c.get('type') == 'BPM']
print('BPM entries:', len(bpms))
for b in bpms:
    print(f'  beat={b.get("beat")}, bpm={b.get("bpm")}')

# Count note types
types = Counter(c.get('type') for c in chart)
print('Note types:', dict(types))

# Show single notes range
singles = [c for c in chart if c.get('type') in ('Single', 'Directional')]
if singles:
    min_beat = min(s.get('beat') for s in singles)
    max_beat = max(s.get('beat') for s in singles)
    print(f'Single/Directional notes range: beat {min_beat} to {max_beat}, count={len(singles)}')

slides = [c for c in chart if c.get('type') == 'Slide']
print(f'Slide notes count: {len(slides)}')