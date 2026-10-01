"""Owned-process quick search/caret/paste check. Run with installed Veya stopped."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path
import sqlite3
import struct
import sys
import tempfile
import time
import zlib

import lazy_check as lc
import wincheck as w

ROOT = Path(__file__).resolve().parent
QUERY = 'OUTSIDE_FIRST_PAGE_UNIQUE'
G = C.WinDLL('gdi32')
w.U.GetDC.argtypes = [W.HWND]
w.U.GetDC.restype = W.HDC
w.U.ReleaseDC.argtypes = [W.HWND, W.HDC]
G.CreateCompatibleDC.argtypes = [W.HDC]
G.CreateCompatibleDC.restype = W.HDC
G.CreateDIBSection.argtypes = [W.HDC, C.c_void_p, W.UINT,
                             C.POINTER(C.c_void_p), W.HANDLE, W.DWORD]
G.CreateDIBSection.restype = W.HANDLE
G.SelectObject.argtypes = [W.HDC, W.HANDLE]
G.SelectObject.restype = W.HANDLE
G.BitBlt.argtypes = [W.HDC, C.c_int, C.c_int, C.c_int, C.c_int,
                    W.HDC, C.c_int, C.c_int, W.DWORD]
G.BitBlt.restype = W.BOOL
G.DeleteDC.argtypes = [W.HDC]
G.DeleteObject.argtypes = [W.HANDLE]
G.GdiFlush.restype = W.BOOL


class BitmapHeader(C.Structure):
    _fields_ = [('size', W.DWORD), ('width', W.LONG), ('height', W.LONG),
                ('planes', W.WORD), ('bit_count', W.WORD), ('compression', W.DWORD),
                ('image_size', W.DWORD), ('x_pixels_per_meter', W.LONG),
                ('y_pixels_per_meter', W.LONG), ('colors_used', W.DWORD),
                ('colors_important', W.DWORD)]


class RegionCapture:
    def __init__(self, width, height):
        self.width, self.height = width, height
        self.screen = self.memory = self.bitmap = self.previous = None
        self.bits = C.c_void_p()
        try:
            self.screen = w.U.GetDC(None)
            assert self.screen, 'GetDC failed'
            self.memory = G.CreateCompatibleDC(self.screen)
            assert self.memory, 'CreateCompatibleDC failed'
            header = BitmapHeader(C.sizeof(BitmapHeader), width, -height, 1, 32,
                                  0, width * height * 4, 0, 0, 0, 0)
            self.bitmap = G.CreateDIBSection(self.screen, C.byref(header), 0,
                                             C.byref(self.bits), None, 0)
            assert self.bitmap and self.bits.value, 'CreateDIBSection failed'
            self.previous = G.SelectObject(self.memory, self.bitmap)
            assert self.previous and self.previous != C.c_void_p(-1).value, 'SelectObject failed'
        except Exception:
            self.close()
            raise

    def capture(self, x, y):
        assert G.BitBlt(self.memory, 0, 0, self.width, self.height,
                        self.screen, x, y, 0x00CC0020), 'BitBlt failed'
        assert G.GdiFlush(), 'GdiFlush failed'
        return C.string_at(self.bits, self.width * self.height * 4)

    def close(self):
        if self.previous and self.memory:
            G.SelectObject(self.memory, self.previous)
            self.previous = None
        if self.bitmap:
            G.DeleteObject(self.bitmap)
            self.bitmap = None
        if self.memory:
            G.DeleteDC(self.memory)
            self.memory = None
        if self.screen:
            w.U.ReleaseDC(None, self.screen)
            self.screen = None


def require_foreground(hwnd, pid):
    actual = W.DWORD()
    w.U.GetWindowThreadProcessId(hwnd, C.byref(actual))
    assert actual.value == pid and w.U.IsWindowVisible(hwnd), 'Owned window not visible'
    assert int(w.U.GetForegroundWindow() or 0) == hwnd, 'Foreground changed; no keys sent'


def wait_for(test, timeout=8):
    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        result = test()
        if result:
            return result
        time.sleep(.05)
    raise RuntimeError('Timed out waiting for owned window state')


def png_region(bgra, width, height, output):
    rows = bytearray()
    for yy in range(height):
        rows.append(0)
        for xx in range(width):
            offset = (yy * width + xx) * 4
            rows.extend((bgra[offset + 2], bgra[offset + 1], bgra[offset]))
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)
    output.write_bytes(b'\x89PNG\r\n\x1a\n' +
                      chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) +
                      chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def caret_check(hwnd, pid, prefix):
    rect = next(item['rect'] for item in w.windows() if item['hwnd'] == hwnd)
    # Only the input's interior is sampled, excluding list loading and borders.
    x, y = rect[0] + 22, rect[1] + 55
    width, height = 395, 30
    capture = RegionCapture(width, height)
    frames, unique, screenshots = [], {}, []
    started = time.perf_counter()
    try:
        while time.perf_counter() - started < 3:
            require_foreground(hwnd, pid)
            pixels = capture.capture(x, y)
            digest = hashlib.sha256(pixels).hexdigest()
            frames.append({'seconds': time.perf_counter() - started, 'hash': digest})
            if digest not in unique:
                unique[digest] = pixels
                if len(screenshots) < 4:
                    path = ROOT / f'{prefix}-caret-{len(screenshots)}.png'
                    png_region(pixels, width, height, path)
                    screenshots.append(str(path))
            time.sleep(.1)
    finally:
        capture.close()
    transitions = sum(a['hash'] != b['hash'] for a, b in zip(frames, frames[1:]))
    differences = []
    if len(unique) == 2:
        a, b = unique.values()
        differences = [(index % width, index // width)
                       for index in range(width * height)
                       if a[index * 4:index * 4 + 3] != b[index * 4:index * 4 + 3]]
    columns = sorted({xx for xx, _ in differences})
    # A blinking caret changes a narrow vertical strip, recurring without input.
    passed = (len(unique) == 2 and transitions >= 2 and len(differences) >= 3
              and bool(columns) and columns[-1] - columns[0] <= 3)
    return {'passed': passed, 'duration_seconds': time.perf_counter() - started,
            'region': [x, y, width, height], 'frames': frames, 'distinct_frames': len(unique),
            'transitions': transitions, 'changed_sample_pixels': len(differences),
            'changed_columns': columns, 'screenshots': screenshots,
            'method': 'One BitBlt per top-down BGRA frame; two recurring frames differing in a vertical caret strip'}


def run(binary, output):
    binary = binary.resolve(strict=True)
    data = Path(tempfile.mkdtemp(prefix='appdata-cpu-behavior-', dir=ROOT))
    w.fixture(data)
    db = data / 'Veya/veya.db'
    with sqlite3.connect(db) as conn:
        raw_before = conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
    evidence = {'exe': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'appdata': str(data), 'raw_before': raw_before, 'passed': False}
    proc = host = None
    target = hwnd = None
    try:
        proc = lc.start(binary, data)
        evidence['pid'] = proc.pid
        manager = wait_for(lambda: next((item for item in w.windows()
                           if item['pid'] == proc.pid and item['visible']), None))
        hwnd = manager['hwnd']
        host = lc.start(sys.executable, arguments=(str(ROOT / 'wincheck.py'), 'host'))
        def own_host():
            try:
                result = json.loads((ROOT / 'host.json').read_text())
                return result if result['pid'] == host.pid else None
            except (FileNotFoundError, json.JSONDecodeError):
                return None
        target = wait_for(own_host)
        evidence['host_pid'] = host.pid
        def owned_state():
            state = w.state()
            state['windows'] = [item for item in state['windows']
                                if item['pid'] in (proc.pid, host.pid)]
            return state
        def show():
            w.U.SetWindowPos(hwnd, -2, 0, 0, 0, 0, 0x13)
            w.foreground(target['window'], target['edit'], 'CPU:')
            require_foreground(target['window'], host.pid)
            w.chord(0x12, 0x56)
            wait_for(lambda: w.U.IsWindowVisible(hwnd) and
                     int(w.U.GetForegroundWindow() or 0) == hwnd)
            w.U.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            require_foreground(hwnd, proc.pid)
        show()
        require_foreground(hwnd, proc.pid)
        w.type_text(QUERY)
        time.sleep(.3)
        evidence['caret'] = caret_check(hwnd, proc.pid, data.name)
        require_foreground(hwnd, proc.pid)
        w.chord(0x1B)
        wait_for(lambda: not w.U.IsWindowVisible(hwnd))
        evidence['after_esc'] = owned_state()
        assert evidence['after_esc']['foreground'] == target['window']
        assert evidence['after_esc']['focus'] == target['edit']
        show()
        require_foreground(hwnd, proc.pid)
        w.type_text(QUERY)
        time.sleep(.3)
        require_foreground(hwnd, proc.pid)
        w.chord(0x0D)
        wait_for(lambda: lc.native_text(target['edit']) == 'CPU:' + QUERY)
        evidence['actual_insert'] = lc.native_text(target['edit'])
        evidence['after_enter'] = owned_state()
        assert not w.U.IsWindowVisible(hwnd)
        assert evidence['after_enter']['foreground'] == target['window']
        assert evidence['after_enter']['focus'] == target['edit']
        with sqlite3.connect(db) as conn:
            evidence['raw_after'] = conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
        assert evidence['raw_after'] == raw_before, 'Replay generated a raw record'
        assert evidence['caret']['passed'], 'Caret did not show recurring narrow-strip blink; inspect frames'
        evidence['passed'] = True
    except Exception as error:
        evidence['error'] = str(error)
    finally:
        if hwnd:
            w.U.SetWindowPos(hwnd, -2, 0, 0, 0, 0, 0x13)
        if host is not None and host.poll() is None:
            if target and target['pid'] == host.pid:
                w.U.SendMessageW(target['window'], 0x10, 0, 0)
            try:
                host.wait(timeout=3)
            except Exception:
                host.terminate()
                host.wait(timeout=5)
        if proc is not None and proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)
        encoded = json.dumps(evidence, ensure_ascii=False, indent=2)
        output.write_text(encoded + '\n', encoding='utf-8')
        print(encoded, flush=True)
    return 0 if evidence['passed'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('exe', type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'cpu-behavior.json')
    args = parser.parse_args()
    raise SystemExit(run(args.exe, args.output))
