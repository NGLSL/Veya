"""Guarded real UI checks against the currently running isolated candidate."""
import json, sqlite3, subprocess, time
from pathlib import Path
import wincheck as w
import lazy_check as lc
ROOT = Path(__file__).resolve().parent
pid = int(__import__('sys').argv[1])
window = next(item for item in w.windows() if item['pid'] == pid and item['title'] == 'Veya')
h = window['hwnd']

def force():
    w.foreground(h)
    w.U.SetWindowPos(h, -1, 0,0,0,0,0x13)
    assert int(w.U.GetForegroundWindow() or 0) == h, 'test window lost foreground'

def shot(name):
    subprocess.run(['powershell', '-NoProfile', '-File', str(ROOT/'screenshot.ps1'), '-Name', name,
                    '-VeyaOnly', '-WindowHwnd', str(h)], check=True, creationflags=subprocess.CREATE_NO_WINDOW)
    assert (ROOT/(name+'.png')).exists()

try:
    force(); w.chord(0x11, 0x4b)
    assert int(w.U.GetForegroundWindow() or 0) == h
    w.chord(0x11, 0x41); w.type_text('CONTENT_TAIL_ONLY'); time.sleep(.5)
    shot('tail-search-guarded')
    current = next(item for item in w.windows() if item['hwnd'] == h)
    x,y = current['rect'][:2]
    assert int(w.U.GetForegroundWindow() or 0) == h
    # Current text detail's exact plain-copy action.
    w.click(x+675, y+278)
    actual = lc.clipboard_text()
    with sqlite3.connect(ROOT/'appdata-compare-rowwarm/Veya/veya.db') as conn:
        expected = conn.execute('SELECT content FROM clipboard_record WHERE sequence=1').fetchone()[0]
    evidence = {'plain_text_length':len(actual), 'matches_full':actual==expected, 'tail':actual[-32:], 'state':w.state()}
    assert evidence['matches_full'], 'plain-text copy differs from stored full content'
    shot('tail-plain-copy')
    (ROOT/'ui-check.json').write_text(json.dumps(evidence,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(evidence,ensure_ascii=False))
finally:
    w.U.SetWindowPos(h,-2,0,0,0,0,0x13)
