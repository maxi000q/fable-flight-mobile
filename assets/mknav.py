# Radio navigation aids for nav.rs, from OurAirports' navaids.csv (public domain):
#   python3 assets/mknav.py <dir with navaids.csv> assets/navaids.tsv.gz
# One line each: kind (V: VOR, VOR-DME, VORTAC; N: NDB, NDB-DME), ident,
# frequency (kHz), lat, lon, elevation (ft), DME (0/1), name, airport.
import csv, gzip, sys
src, out = sys.argv[1], sys.argv[2]
KIND = {'VOR': ('V', 0), 'VOR-DME': ('V', 1), 'VORTAC': ('V', 1), 'NDB': ('N', 0), 'NDB-DME': ('N', 1)}
def clean(s): return ' '.join(s.replace('\t', ' ').split())
lines = []
for r in csv.DictReader(open(src + '/navaids.csv', encoding='utf-8')):
    k = KIND.get(r['type'])
    if not k or not r['frequency_khz']: continue
    try: lat, lon, f = float(r['latitude_deg']), float(r['longitude_deg']), int(float(r['frequency_khz']))
    except ValueError: continue
    elev = r['elevation_ft'] or '0'
    lines.append('\t'.join([k[0], clean(r['ident']), str(f), '%.5f' % lat, '%.5f' % lon, str(int(float(elev))), str(k[1]), clean(r['name']), r['associated_airport']]))
data = ('\n'.join(lines) + '\n').encode('utf-8')
open(out, 'wb').write(gzip.compress(data, 9, mtime=0))
print(len(lines), 'navaids', len(data), 'bytes raw')
