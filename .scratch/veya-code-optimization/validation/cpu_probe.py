"""Read-only Windows x64 CPU/RIP probe; brief thread pauses always resume in finally."""
import argparse
import collections
import ctypes as c
import ctypes.wintypes as w
import json
import os
import time

parser = argparse.ArgumentParser()
parser.add_argument('pid', type=int)
parser.add_argument('--thread', type=int)
parser.add_argument('--symbols', default='target/release-package/release')
args = parser.parse_args()
k = c.WinDLL('kernel32', use_last_error=True)
d = c.WinDLL('dbghelp', use_last_error=True)
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenProcess.restype = w.HANDLE
k.OpenThread.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenThread.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]
k.GetProcessTimes.argtypes = [w.HANDLE, *([c.POINTER(w.FILETIME)] * 4)]
k.SuspendThread.argtypes = [w.HANDLE]
k.SuspendThread.restype = w.DWORD
k.ResumeThread.argtypes = [w.HANDLE]
k.ResumeThread.restype = w.DWORD
k.GetThreadContext.argtypes = [w.HANDLE, c.c_void_p]
d.SymInitializeW.argtypes = [w.HANDLE, w.LPCWSTR, w.BOOL]
d.SymCleanup.argtypes = [w.HANDLE]
d.SymSetOptions.argtypes = [w.DWORD]
d.SymFromAddr.argtypes = [w.HANDLE, c.c_uint64, c.POINTER(c.c_uint64), c.c_void_p]

class Symbol(c.Structure):
    _fields_ = [('size', w.ULONG), ('type', w.ULONG), ('reserved', c.c_uint64 * 2),
                ('index', w.ULONG), ('symbol_size', w.ULONG), ('module', c.c_uint64),
                ('flags', w.ULONG), ('value', c.c_uint64), ('address', c.c_uint64),
                ('register', w.ULONG), ('scope', w.ULONG), ('tag', w.ULONG),
                ('name_len', w.ULONG), ('max_name_len', w.ULONG), ('name', c.c_char * 1)]

hp = k.OpenProcess(0x400 | 0x10, False, args.pid)
if not hp:
    raise c.WinError(c.get_last_error())

def cpu():
    values = [w.FILETIME() for _ in range(4)]
    if not k.GetProcessTimes(hp, *[c.byref(x) for x in values]):
        raise c.WinError(c.get_last_error())
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in values[2:]) / 1e7

out = {'pid': args.pid, 'logical_processors': os.cpu_count(), 'machine_cpu_percent': []}
for _ in range(3):
    start = cpu()
    clock = time.perf_counter()
    time.sleep(2)
    out['machine_cpu_percent'].append(round((cpu() - start) / (time.perf_counter() - clock) / os.cpu_count() * 100, 3))
if args.thread:
    ht = k.OpenThread(0x2 | 0x8 | 0x40, False, args.thread)
    if not ht:
        raise c.WinError(c.get_last_error())
    addresses = collections.Counter()
    try:
        for _ in range(100):
            buf = c.create_string_buffer(1232 + 16)
            base = (c.addressof(buf) + 15) & ~15
            c.c_uint32.from_address(base + 48).value = 0x100003
            if k.SuspendThread(ht) == 0xffffffff:
                raise c.WinError(c.get_last_error())
            try:
                if not k.GetThreadContext(ht, base):
                    raise c.WinError(c.get_last_error())
                addresses[c.c_uint64.from_address(base + 248).value] += 1
            finally:
                if k.ResumeThread(ht) == 0xffffffff:
                    raise c.WinError(c.get_last_error())
            time.sleep(0.011)
    finally:
        k.CloseHandle(ht)
    d.SymSetOptions(0x2 | 0x4 | 0x200 | 0x80000)
    initialized = d.SymInitializeW(hp, os.path.abspath(args.symbols), True)
    out['symbols_initialized'] = bool(initialized)
    resolved = collections.Counter()
    for address, count in addresses.items():
        buf = c.create_string_buffer(c.sizeof(Symbol) + 2048)
        symbol = Symbol.from_buffer(buf)
        symbol.size = c.sizeof(Symbol)
        symbol.max_name_len = 2048
        displacement = c.c_uint64()
        if initialized and d.SymFromAddr(hp, address, c.byref(displacement), buf):
            name = c.string_at(c.addressof(buf) + Symbol.name.offset, symbol.name_len).decode('utf-8', 'replace')
        else:
            name = hex(address)
        resolved[name] += count
    out['thread'] = args.thread
    out['sampled_functions'] = resolved.most_common(20)
    if initialized:
        d.SymCleanup(hp)
k.CloseHandle(hp)
print(json.dumps(out, ensure_ascii=False, indent=2))
