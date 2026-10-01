"""Measure a visible selected record, then prove Enter inserts that record."""
import ctypes as C
from ctypes import wintypes as W
import json, shutil, sqlite3, time, sys
from pathlib import Path
import lazy_check as lc
import wincheck as w
root = Path(__file__).resolve().parent
G = C.WinDLL('gdi32')
w.U.GetDC.argtypes = [W.HWND]; w.U.GetDC.restype = W.HDC
w.U.ReleaseDC.argtypes = [W.HWND,W.HDC]
G.GetPixel.argtypes = [W.HDC,C.c_int,C.c_int]; G.GetPixel.restype = W.DWORD
marker = 'VEYA_READY_TEXT_101'
evidence = []
host = lc.start(sys.executable, arguments=(str(root/'wincheck.py'),'host'))
target = None
try:
    for _ in range(100):
        try:
            target = json.loads((root/'host.json').read_text())
            if target['pid'] == host.pid: break
        except (FileNotFoundError,json.JSONDecodeError): pass
        time.sleep(.05)
    assert target['pid'] == host.pid
    for name,binary in [('v4',root.parent.parent/'veya-quick-paste/validation/veya-candidate-v4.exe'),
                         ('row',root/'veya-lazy-row.exe')]:
        data = root/('appdata-readiness-'+name)/'Veya'
        data.mkdir(parents=True,exist_ok=False)
        shutil.copyfile(root/'appdata-baseline/Veya/veya.db',data/'veya.db')
        with sqlite3.connect(data/'veya.db') as conn:
            conn.execute('INSERT INTO clipboard_record VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',
                (101,'text',marker,'ready101','validation.exe',4242,'Readiness Fixture','exact',int(time.time()*1000),0,None,None,None))
        begin = time.perf_counter()
        proc = lc.start(binary,'appdata-readiness-'+name)
        h = None
        dc = w.U.GetDC(None)
        try:
            def wait_ready(quick):
                deadline = time.perf_counter()+8
                while time.perf_counter()<deadline:
                    candidates = [item for item in w.windows() if item['pid']==proc.pid and item['visible']]
                    if candidates:
                        current = candidates[0]
                        handle = current['hwnd']
                        w.U.SetWindowPos(handle,-1,0,0,0,0,0x13)
                        if current['size']==([440,580] if quick else [1080,700]):
                            x,y=current['rect'][:2]
                            color=G.GetPixel(dc,x+250 if quick else x+300,y+106 if quick else y+119)
                            if color == 61+(140<<8)+(250<<16):
                                return handle
                    time.sleep(.005)
                raise RuntimeError('selected record not visible before deadline')
            h=wait_ready(False)
            cold_ms=(time.perf_counter()-begin)*1000
            item={'case':name,'cold_selected_record_ms':cold_ms,'quick_selected_record_ms':[],'paste_verified':[]}
            for attempt in range(3):
                w.U.SetWindowPos(h,-2,0,0,0,0,0x13)
                w.foreground(target['window'],target['edit'],'READY:')
                begin=time.perf_counter()
                w.press(0x12); w.press(0x56); w.press(0x56,True); w.press(0x12,True)
                h=wait_ready(True)
                item['quick_selected_record_ms'].append((time.perf_counter()-begin)*1000)
                assert int(w.U.GetForegroundWindow() or 0)==h
                w.chord(0x0D)
                actual=lc.native_text(target['edit'])
                item['paste_verified'].append(actual=='READY:'+marker)
                assert item['paste_verified'][-1],actual
                time.sleep(.1)
            evidence.append(item)
            print(json.dumps(item),flush=True)
        finally:
            if h: w.U.SetWindowPos(h,-2,0,0,0,0,0x13)
            w.U.ReleaseDC(None,dc)
            if proc.poll() is None: proc.terminate(); proc.wait(timeout=5)
    (root/'readiness.json').write_text(json.dumps(evidence,indent=2),encoding='utf-8')
finally:
    if target and target['pid']==host.pid:
        w.U.SendMessageW(target['window'],0x10,0,0)
    if host.poll() is None:
        try: host.wait(timeout=3)
        except Exception: host.terminate()
