"""Same fixture/configuration baseline, cold thumbnails and cached thumbnails."""
from pathlib import Path
import hashlib, json, shutil, sqlite3, subprocess, time
import lazy_check as lc
import wincheck as wc

root = Path(__file__).resolve().parent
source = root / 'appdata-baseline/Veya/veya.db'
cases = [
    ('guarded-v4', root.parent.parent / 'veya-quick-paste/validation/veya-candidate-v4.exe', source),
    ('guarded-rowcold', root / 'veya-lazy-row.exe', source),
    ('guarded-rowwarm', root / 'veya-lazy-row.exe', root / 'appdata-guarded-rowcold/Veya/veya.db'),
]
evidence = []
for name, binary, fixture in cases:
    data = root / ('appdata-' + name) / 'Veya'
    data.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(fixture, data / 'veya.db')
    proc = lc.start(binary, 'appdata-' + name)
    item = {'case': name, 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
    try:
        for _ in range(100):
            candidates = [item for item in wc.windows() if item['pid'] == proc.pid and item['visible']]
            if candidates: break
            time.sleep(.05)
        h = candidates[0]['hwnd']
        wc.foreground(h)
        wc.U.SetWindowPos(h,-1,0,0,0,0,0x13)
        samples = []
        for _ in range(60):
            assert proc.poll() is None
            samples.append(lc.memory(proc.pid)); time.sleep(.1)
        item['home'] = lc.memory(proc.pid)
        item['sampled_peak_private'] = max(s['private'] for s in samples)
        item['home_windows'] = wc.state()
        subprocess.run(['powershell', '-NoProfile', '-File', str(root / 'screenshot.ps1'), '-Name', name+'-home', '-VeyaOnly', '-WindowHwnd', str(h)], check=True, creationflags=subprocess.CREATE_NO_WINDOW)
        assert (root/(name+'-home.png')).exists()
        assert int(wc.U.GetForegroundWindow() or 0) == h
        wc.chord(0x12, 0x56); time.sleep(2)
        item['quick'] = lc.memory(proc.pid)
        subprocess.run(['powershell', '-NoProfile', '-File', str(root / 'screenshot.ps1'), '-Name', name+'-quick', '-VeyaOnly', '-WindowHwnd', str(h)], check=True, creationflags=subprocess.CREATE_NO_WINDOW)
        assert (root/(name+'-quick.png')).exists()
        wc.chord(0x1B); time.sleep(2)
        item['hidden'] = lc.memory(proc.pid)
        with sqlite3.connect(data / 'veya.db') as conn:
            item['raw_count'] = conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
            if 'row' in name:
                item['thumbnails'] = conn.execute('SELECT count(*) FROM image_thumbnail').fetchone()[0]
        evidence.append(item)
        print(json.dumps(item), flush=True)
    finally:
        wc.U.SetWindowPos(h,-2,0,0,0,0,0x13)
        if proc.poll() is None: proc.terminate(); proc.wait(timeout=5)
(root / 'memory-guarded.json').write_text(json.dumps(evidence, indent=2), encoding='utf-8')
