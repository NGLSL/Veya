"""Isolated hidden-window CPU regression check; never stops an existing Veya.

Run only after the installed singleton has been stopped by the operator:
    python cpu_runtime.py PATH_TO_CANDIDATE_EXE [--output evidence.json]
The fresh APPDATA fixture is retained for inspection. All CPU percentages are
normalized to the machine's logical processor count, as in Task Manager.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
from pathlib import Path
import tempfile
import time

import lazy_check as lc
import wincheck as wc

K = wc.K
U = wc.U
K.GetProcessTimes.argtypes = [W.HANDLE] + [C.POINTER(W.FILETIME)] * 4
K.GetProcessTimes.restype = W.BOOL


def cpu_seconds(handle):
    created, exited, kernel, user = (W.FILETIME() for _ in range(4))
    if not K.GetProcessTimes(handle, C.byref(created), C.byref(exited),
                             C.byref(kernel), C.byref(user)):
        raise C.WinError(C.get_last_error())
    return sum((v.dwHighDateTime << 32) | v.dwLowDateTime
               for v in (kernel, user)) / 10_000_000


def owned_windows(pid):
    return [w for w in wc.windows() if w['pid'] == pid and w['title'] == 'Veya']


def guarded_toggle(proc, hwnd):
    if proc.poll() is not None:
        raise RuntimeError(f'Candidate exited before Alt+V; exit code {proc.returncode}')
    windows = owned_windows(proc.pid)
    if not any(w['hwnd'] == hwnd and w['visible'] for w in windows):
        raise RuntimeError('Candidate window was not owned and visible before Alt+V')
    if int(U.GetForegroundWindow() or 0) != hwnd:
        raise RuntimeError('Candidate lost foreground before Alt+V; no keys sent')
    wc.chord(0x12, 0x56)


def run(binary, output=None):
    binary = binary.resolve(strict=True)
    data = Path(tempfile.mkdtemp(prefix='appdata-cpu-', dir=lc.ROOT))
    evidence = {
        'exe': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'appdata': str(data), 'logical_processors': os.cpu_count() or 1,
        'threshold_percent': 1.0, 'samples': [], 'passed': False,
    }
    proc = None
    handle = None
    try:
        proc = lc.start(binary, data)
        evidence['pid'] = proc.pid
        deadline = time.perf_counter() + 15
        while time.perf_counter() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f'Candidate exited before creating its own window; '
                                   f'exit code {proc.returncode}; check singleton')
            windows = owned_windows(proc.pid)
            visible = [w for w in windows if w['visible']]
            if len(visible) == 1:
                break
            time.sleep(.1)
        else:
            raise RuntimeError('Could not identify one visible candidate main window')
        hwnd = visible[0]['hwnd']
        evidence['hwnd'] = hwnd
        evidence['before_toggle'] = windows
        # WM_CLOSE follows Iced's native exit behavior. Exercise Veya's actual
        # manager -> quick -> hidden shortcut lifecycle instead.
        wc.foreground(hwnd)
        guarded_toggle(proc, hwnd)
        deadline = time.perf_counter() + 5
        while time.perf_counter() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f'Candidate exited entering quick panel; exit code {proc.returncode}')
            quick_windows = owned_windows(proc.pid)
            quick = [w for w in quick_windows if w['hwnd'] == hwnd and w['visible']
                     and w['size'][0] <= 600 and w['size'][1] <= 700]
            if len(quick) == 1 and int(U.GetForegroundWindow() or 0) == hwnd:
                break
            time.sleep(.05)
        else:
            raise RuntimeError('Alt+V did not produce the owned, foreground quick panel')
        evidence['quick_panel'] = quick[0]
        guarded_toggle(proc, hwnd)
        deadline = time.perf_counter() + 5
        while time.perf_counter() < deadline:
            if proc.poll() is not None:
                raise RuntimeError(f'Candidate exited instead of hiding to tray; exit code {proc.returncode}')
            if not U.IsWindowVisible(hwnd):
                break
            time.sleep(.05)
        else:
            raise RuntimeError('Main window remained visible after second Alt+V')
        time.sleep(1)
        handle = K.OpenProcess(0x1000, False, proc.pid)
        if not handle:
            raise C.WinError(C.get_last_error())
        for index in range(3):
            started = time.perf_counter()
            initial_cpu = cpu_seconds(handle)
            hidden = True
            while time.perf_counter() - started < 2:
                if proc.poll() is not None:
                    raise RuntimeError(f'Candidate exited while sampling; exit code {proc.returncode}')
                windows = owned_windows(proc.pid)
                hidden = hidden and not bool(U.IsWindowVisible(hwnd))
                hidden = hidden and not any(w['visible'] for w in windows)
                time.sleep(.1)
            elapsed = time.perf_counter() - started
            cpu = cpu_seconds(handle) - initial_cpu
            percent = cpu / elapsed / evidence['logical_processors'] * 100
            evidence['samples'].append({
                'index': index + 1, 'wall_seconds': elapsed, 'cpu_seconds': cpu,
                'cpu_percent': percent, 'all_checks_hidden': hidden,
                'windows_at_end': owned_windows(proc.pid),
            })
        evidence['passed'] = all(s['all_checks_hidden'] and
                                 s['cpu_percent'] < evidence['threshold_percent']
                                 for s in evidence['samples'])
        if not evidence['passed']:
            evidence['error'] = 'Hidden-window CPU regression threshold failed'
    except Exception as error:
        evidence['error'] = str(error)
    finally:
        if handle:
            K.CloseHandle(handle)
        if proc is not None and proc.poll() is None:
            # Only this Popen-owned candidate is terminated; no process-name kill.
            proc.terminate()
            proc.wait(timeout=5)
        evidence['candidate_exited'] = proc is None or proc.poll() is not None
        evidence['candidate_exit_code'] = None if proc is None else proc.returncode
        encoded = json.dumps(evidence, ensure_ascii=False, indent=2)
        if output:
            output.write_text(encoded + '\n', encoding='utf-8')
        print(encoded, flush=True)
    return 0 if evidence['passed'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('exe', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    raise SystemExit(run(args.exe, args.output))
