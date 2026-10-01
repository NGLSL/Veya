"""Isolated Windows behavior fixture/control. Python stdlib only; no product edits."""
import ctypes as C
from ctypes import wintypes as W
import json, os, sys, time, sqlite3, hashlib, struct, zlib
from pathlib import Path
U=C.WinDLL('user32',use_last_error=True); K=C.WinDLL('kernel32',use_last_error=True)
U.CreateWindowExW.restype=W.HWND
U.CreateWindowExW.argtypes=[W.DWORD,W.LPCWSTR,W.LPCWSTR,W.DWORD,C.c_int,C.c_int,C.c_int,C.c_int,W.HWND,W.HMENU,W.HINSTANCE,C.c_void_p]
U.SendMessageW.restype=C.c_ssize_t
U.SendMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]
U.GetForegroundWindow.restype=W.HWND
U.GetWindowTextW.argtypes=[W.HWND,W.LPWSTR,C.c_int]
U.SetForegroundWindow.argtypes=[W.HWND]
U.GetWindowThreadProcessId.argtypes=[W.HWND,C.POINTER(W.DWORD)]
U.GetWindowRect.argtypes=[W.HWND,C.POINTER(W.RECT)]
U.IsWindowVisible.argtypes=[W.HWND]
U.SetFocus.argtypes=[W.HWND]
U.SetFocus.restype=W.HWND
U.ShowWindow.argtypes=[W.HWND,C.c_int]
U.SetWindowPos.argtypes=[W.HWND,W.HWND,C.c_int,C.c_int,C.c_int,C.c_int,W.UINT]
U.SetClipboardData.restype=W.HANDLE
U.SetClipboardData.argtypes=[W.UINT,W.HANDLE]
K.GlobalAlloc.restype=W.HGLOBAL
K.GlobalAlloc.argtypes=[W.UINT,C.c_size_t]
K.GlobalLock.restype=C.c_void_p
K.GlobalLock.argtypes=[W.HGLOBAL]
K.GlobalUnlock.argtypes=[W.HGLOBAL]
class GUIINFO(C.Structure):
    _fields_=[('cbSize',W.DWORD),('flags',W.DWORD),('active',W.HWND),('focus',W.HWND),('capture',W.HWND),('menu',W.HWND),('move',W.HWND),('caret',W.HWND),('rect',W.RECT)]
class KEY(C.Structure):
    _fields_=[('vk',W.WORD),('scan',W.WORD),('flags',W.DWORD),('time',W.DWORD),('extra',C.c_size_t)]
class MOUSE(C.Structure):
    _fields_=[('x',W.LONG),('y',W.LONG),('data',W.DWORD),('flags',W.DWORD),('time',W.DWORD),('extra',C.c_size_t)]
class UNION(C.Union): _fields_=[('key',KEY),('mouse',MOUSE)]
class INPUT(C.Structure): _fields_=[('type',W.DWORD),('u',UNION)]
def press(vk,up=False):
    i=INPUT(1,UNION(key=KEY(vk,0,2 if up else 0,0,0))); assert U.SendInput(1,C.byref(i),C.sizeof(i))==1
def chord(*keys):
    for k in keys: press(k)
    for k in reversed(keys): press(k,True)
    time.sleep(.45)
def type_text(s):
    for n in range(0,len(s.encode('utf-16-le')),2):
        scan=int.from_bytes(s.encode('utf-16-le')[n:n+2],'little')
        for f in [4,6]:
            i=INPUT(1,UNION(key=KEY(0,scan,f,0,0))); assert U.SendInput(1,C.byref(i),C.sizeof(i))==1
    time.sleep(.7)
def foreground(h,e=None,text=None):
    U.ShowWindow(h,9)
    U.SetWindowPos(h,-1,0,0,0,0,0x13)
    r=W.RECT();U.GetWindowRect(h,C.byref(r));click(r.left+30,r.top+70)
    U.SetWindowPos(h,-2,0,0,0,0,0x13)
    U.SendMessageW(h,0x8001,0,0)
    if e:
        if text is not None:
            b=C.create_unicode_buffer(text);U.SendMessageW(e,12,0,C.addressof(b));U.SendMessageW(e,0xB1,len(text),len(text))
    time.sleep(.3)
def click(x,y):
    U.SetCursorPos(x,y)
    for f in [2,4]:
        i=INPUT(0,UNION(mouse=MOUSE(0,0,0,f,0,0))); assert U.SendInput(1,C.byref(i),C.sizeof(i))==1
    time.sleep(.7)
def clipboard_probe(text='VEYA_SEQUENCE_PROBE'):
    before=U.GetClipboardSequenceNumber()
    assert U.OpenClipboard(None), C.get_last_error()
    try:
        assert U.EmptyClipboard()
        encoded=(text+'\0').encode('utf-16-le')
        h=K.GlobalAlloc(2,len(encoded));p=K.GlobalLock(h)
        C.memmove(p,encoded,len(encoded));K.GlobalUnlock(h)
        result=U.SetClipboardData(13,h)
        assert result, C.get_last_error()
        locked=U.GetClipboardSequenceNumber()
    finally: assert U.CloseClipboard()
    closed=U.GetClipboardSequenceNumber();assert U.OpenClipboard(None)
    try:
        U.GetClipboardData.restype=W.HANDLE
        data=U.GetClipboardData(13);read=C.wstring_at(K.GlobalLock(data));K.GlobalUnlock(data)
        readonly=U.GetClipboardSequenceNumber()
    finally:assert U.CloseClipboard()
    return dict(before=before,set_success=bool(result),while_open_after_set=locked,after_close=closed,read_text=read,readonly_open=readonly,readonly_close=U.GetClipboardSequenceNumber())
def windows():
    out=[]
    @C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    def cb(h,_):
        title=C.create_unicode_buffer(1024); U.GetWindowTextW(h,title,1024)
        if title.value in ['Veya','Veya Validation Target','Veya Validation Target Two'] or 'VEYA_BROWSER_' in title.value or 'Notepad' in title.value or '记事本' in title.value:
            p=W.DWORD(); U.GetWindowThreadProcessId(h,C.byref(p)); r=W.RECT();U.GetWindowRect(h,C.byref(r))
            out.append(dict(hwnd=int(h),pid=p.value,title=title.value,visible=bool(U.IsWindowVisible(h)),rect=[r.left,r.top,r.right,r.bottom],size=[r.right-r.left,r.bottom-r.top]))
        return True
    U.EnumWindows(cb,0); return out
def state():
    info=GUIINFO();info.cbSize=C.sizeof(info);U.GetGUIThreadInfo(0,C.byref(info))
    return dict(time=time.time(),foreground=int(U.GetForegroundWindow() or 0),focus=int(info.focus or 0),windows=windows())
def target_text(edit):
    b=C.create_unicode_buffer(65536);U.SendMessageW(edit,13,65536,C.addressof(b));return b.value
def host():
    title=sys.argv[2] if len(sys.argv)>2 else 'Veya Validation Target'
    PROC=C.WINFUNCTYPE(C.c_ssize_t,W.HWND,W.UINT,W.WPARAM,W.LPARAM)
    U.DefWindowProcW.restype=C.c_ssize_t;U.DefWindowProcW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM]
    child=[None]
    @PROC
    def proc(h,m,w,l):
        if m==0x8001:
            U.ShowWindow(h,9);U.SetForegroundWindow(h);U.SetFocus(child[0]);return 0
        if m==7 and child[0]: U.SetFocus(child[0]);return 0
        if m==2: U.PostQuitMessage(0);return 0
        return U.DefWindowProcW(h,m,w,l)
    class CLASS(C.Structure):
        _fields_=[('style',W.UINT),('proc',PROC),('cls',C.c_int),('wnd',C.c_int),('instance',W.HINSTANCE),('icon',W.HICON),('cursor',W.HANDLE),('background',W.HBRUSH),('menu',W.LPCWSTR),('name',W.LPCWSTR)]
    wc=CLASS();wc.proc=proc;wc.name='VeyaValidationTarget';wc.background=6
    assert U.RegisterClassW(C.byref(wc))
    h=U.CreateWindowExW(0,wc.name,title,0x10CF0000,60,80,700,380,None,None,None,None)
    e=U.CreateWindowExW(0,'EDIT','ORIGINAL:',0x50010004|0x200000|0x1000,12,12,650,310,h,None,None,None)
    child[0]=e
    U.ShowWindow(h,5);U.ShowWindow(h,5)
    U.SetForegroundWindow(h);U.SetFocus(e)
    Path(__file__).with_name('host'+('2' if title.endswith('Two') else '')+'.json').write_text(json.dumps(dict(pid=os.getpid(),window=int(h),edit=int(e))),encoding='utf-8')
    msg=W.MSG()
    while U.GetMessageW(C.byref(msg),None,0,0)>0:
        U.TranslateMessage(C.byref(msg));U.DispatchMessageW(C.byref(msg))
def fixture(root):
    p=Path(root)/'Veya';p.mkdir(parents=True,exist_ok=True); db=p/'veya.db'
    c=sqlite3.connect(db)
    c.executescript('''CREATE TABLE clipboard_record(sequence INTEGER PRIMARY KEY,content_type TEXT NOT NULL,content TEXT NOT NULL,content_hash TEXT NOT NULL,source_app TEXT NOT NULL,source_pid INTEGER NOT NULL,source_window TEXT NOT NULL DEFAULT '',source_confidence TEXT NOT NULL,created_at_ms INTEGER NOT NULL,pinned INTEGER NOT NULL DEFAULT 0,payload BLOB,image_width INTEGER,image_height INTEGER);CREATE TABLE app_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);''')
    now=int(time.time()*1000)
    for seq in range(1,41):
        text='OUTSIDE_FIRST_PAGE_UNIQUE' if seq==1 else f'QUICK_FIXTURE_{seq:02d}'
        c.execute('INSERT INTO clipboard_record VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',(seq,'text',text,hashlib.sha256(text.encode()).hexdigest(),'validation.exe',4242,'Fixture','exact',now+seq,0,None,None,None))
    missing=str(Path(root)/'missing-file.txt')
    c.execute('INSERT INTO clipboard_record VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',(41,'files',missing,'missing-fixture','validation.exe',4242,'Fixture','exact',now-100,0,json.dumps([missing]).encode(),None,None))
    c.commit();print(db)
def perf_fixture(root):
    fixture(root)
    c=sqlite3.connect(Path(root)/'Veya'/'veya.db');c.execute('DELETE FROM clipboard_record')
    def chunk(name,data):return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
    width=height=2048
    row=b'\0'+bytes(range(256))*32
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(row*height))+chunk(b'IEND',b'')
    now=int(time.time()*1000)
    for seq in range(1,101):
        if seq%2:
            kind='text';text=f'LONG_RECORD_{seq:03d}\n'+('0123456789abcdef'*65536);blob=None;w=h=None
        else:
            kind='image';text=f'图片 {width}×{height}';blob=png;w=h=width
        c.execute('INSERT INTO clipboard_record VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',(seq,kind,text,f'perf-{seq}','validation.exe',4242,'Memory Fixture','exact',now+seq,0,blob,w,h))
    c.commit();print(json.dumps(dict(records=100,text_records=50,text_chars=1048592,image_records=50,image_size=[width,height],image_bytes=len(png))))
if __name__=='__main__':
    mode=sys.argv[1]
    if mode=='host': host()
    elif mode=='fixture':fixture(sys.argv[2])
    elif mode=='perf-fixture':perf_fixture(sys.argv[2])
    elif mode=='state':print(json.dumps(state(),ensure_ascii=False))
    elif mode=='clipboard-probe':print(json.dumps(clipboard_probe()))
    elif mode=='target':
        hostdata=json.loads(Path(__file__).with_name('host.json').read_text())
        if len(sys.argv)>2: foreground(hostdata['window'],hostdata['edit'],sys.argv[2])
        print(json.dumps(dict(state=state(),text=target_text(hostdata['edit'])),ensure_ascii=False))
    elif mode=='hotkey':chord(0x12,0x56);print(json.dumps(state(),ensure_ascii=False))
    elif mode=='key':chord(int(sys.argv[2],0));print(json.dumps(state(),ensure_ascii=False))
    elif mode=='type':type_text(sys.argv[2]);print(json.dumps(state(),ensure_ascii=False))
    elif mode=='click':click(int(sys.argv[2]),int(sys.argv[3]));print(json.dumps(state(),ensure_ascii=False))
