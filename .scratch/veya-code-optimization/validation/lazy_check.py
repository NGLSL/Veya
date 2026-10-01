"""Isolated Windows checks; stdlib only. Run after stopping the verified installed Veya."""
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time
import wincheck as wc

ROOT = Path(__file__).resolve().parent
K = wc.K
K.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
K.OpenProcess.restype = W.HANDLE
K.CloseHandle.argtypes = [W.HANDLE]
P = C.WinDLL('psapi')
class MEMORY(C.Structure):
    _fields_ = [('cb', W.DWORD), ('PageFaultCount', W.DWORD)] + [(name, C.c_size_t) for name in
        ['PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage', 'QuotaPagedPoolUsage',
         'QuotaPeakNonPagedPoolUsage', 'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage']]
P.GetProcessMemoryInfo.argtypes = [W.HANDLE, C.POINTER(MEMORY), W.DWORD]

def memory(pid):
    handle = K.OpenProcess(0x1000 | 0x10, False, pid)
    assert handle
    try:
        value = MEMORY(); value.cb = C.sizeof(value)
        assert P.GetProcessMemoryInfo(handle, C.byref(value), value.cb)
        return {'private': value.PrivateUsage, 'working_set': value.WorkingSetSize,
                'peak_private': value.PeakPagefileUsage, 'peak_working_set': value.PeakWorkingSetSize}
    finally:
        K.CloseHandle(handle)

def start(binary, data=None, arguments=()):
    env = os.environ.copy()
    if data: env['APPDATA'] = str(ROOT / data)
    info = subprocess.STARTUPINFO(); info.dwFlags |= subprocess.STARTF_USESHOWWINDOW; info.wShowWindow = 0
    return subprocess.Popen([str(binary), *arguments], env=env, startupinfo=info,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def native_text(edit):
    count = wc.U.SendMessageW(edit, 14, 0, 0)
    buffer = C.create_unicode_buffer(count + 1)
    wc.U.SendMessageW(edit, 13, count + 1, C.addressof(buffer))
    return buffer.value

def clipboard_text():
    wc.U.GetClipboardData.restype = W.HANDLE
    assert wc.U.OpenClipboard(None)
    try:
        handle = wc.U.GetClipboardData(13)
        assert handle
        ptr = K.GlobalLock(handle)
        assert ptr
        try: return C.wstring_at(ptr)
        finally: K.GlobalUnlock(handle)
    finally: wc.U.CloseClipboard()

def run():
    candidate = ROOT / (sys.argv[1] if len(sys.argv) > 1 else 'veya-lazy.exe')
    data = sys.argv[2] if len(sys.argv) > 2 else 'appdata-lazy'
    proc = start(candidate, data)
    print(json.dumps({'candidate_pid': proc.pid}), flush=True)
    host = None
    evidence = {'sha256': hashlib.sha256(candidate.read_bytes()).hexdigest(), 'pid': proc.pid}
    try:
        samples = []
        for _ in range(60):
            if proc.poll() is not None: raise RuntimeError('candidate exited')
            samples.append(memory(proc.pid)); time.sleep(.1)
        evidence['home'] = memory(proc.pid)
        evidence['sampled_home_peak_private'] = max(s['private'] for s in samples)
        print(json.dumps({'home': evidence['home']}), flush=True)
        db = ROOT / data / 'Veya/veya.db'
        with sqlite3.connect(db) as conn:
            evidence['thumbnail_rows_home'] = conn.execute('SELECT count(*) FROM image_thumbnail').fetchone()[0]
            evidence['raw_before'] = conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
        host = start(sys.executable, arguments=(str(ROOT / 'wincheck.py'), 'host'))
        for _ in range(50):
            try:
                target = json.loads((ROOT / 'host.json').read_text())
                if target['pid'] == host.pid: break
            except (FileNotFoundError, json.JSONDecodeError): pass
            time.sleep(.1)
        assert target['pid'] == host.pid
        wc.U.SendMessageW(target['edit'], 0xC5, 2_000_000, 0)
        wc.foreground(target['window'], target['edit'], 'LAZY:')
        wc.chord(0x12, 0x56)
        time.sleep(.5)
        evidence['quick'] = memory(proc.pid)
        evidence['quick_windows'] = wc.state()
        wc.chord(0x28)
        wc.chord(0x0D)
        time.sleep(.5)
        inserted = native_text(target['edit'])
        with sqlite3.connect(db) as conn:
            expected = conn.execute('SELECT content FROM clipboard_record WHERE sequence=99').fetchone()[0]
            evidence['raw_after'] = conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
        evidence['typed_paste'] = {'length': len(inserted), 'matches_full': inserted == 'LAZY:' + expected,
            'tail': inserted[-32:], 'state': wc.state()}
        evidence['hidden'] = memory(proc.pid)
        assert evidence['typed_paste']['matches_full'], 'typed text was truncated or wrong'
        assert evidence['raw_before'] == evidence['raw_after'], 'self replay created raw copy'
        (ROOT / (data + '-runtime.json')).write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding='utf-8')
        print(json.dumps(evidence, ensure_ascii=False), flush=True)
        return evidence
    finally:
        if host:
            try:
                wc.U.SendMessageW(target['window'], 0x10, 0, 0)
                host.wait(timeout=3)
            except Exception: host.terminate()
        if proc.poll() is None:
            proc.terminate(); proc.wait(timeout=5)

if __name__ == '__main__': run()
