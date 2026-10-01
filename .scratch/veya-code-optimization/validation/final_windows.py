"""Final build smoke: lazy details, paging, cancellation and actual text paste."""
import json, hashlib, sqlite3, subprocess, sys, time
from pathlib import Path
import lazy_check as lc
import wincheck as w
root=Path(__file__).resolve().parent
data=root/'appdata-final-ui'
w.fixture(data)
db=data/'Veya/veya.db'
full='FULL_MODAL_START\n'+('example line 0123456789\n'*140)+'MODAL_TAIL'
with sqlite3.connect(db) as conn:
    now=int(time.time()*1000)
    conn.execute('INSERT INTO clipboard_record VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)',
        (42,'text',full,'full42','validation.exe',4242,'Details Fixture','exact',now+100,0,None,None,None))
candidate=root/'veya-lazy-final.exe'
proc=lc.start(candidate,'appdata-final-ui')
host=None; h=None; target=None
evidence={'sha256':hashlib.sha256(candidate.read_bytes()).hexdigest(),'pid':proc.pid}
try:
    for _ in range(100):
        windows=[item for item in w.windows() if item['pid']==proc.pid and item['visible']]
        if windows: break
        time.sleep(.05)
    h=windows[0]['hwnd']
    def force():
        w.foreground(h); w.U.SetWindowPos(h,-1,0,0,0,0,0x13)
        assert int(w.U.GetForegroundWindow() or 0)==h
    def search(query):
        force(); w.chord(0x11,0x4b)
        assert int(w.U.GetForegroundWindow() or 0)==h
        w.chord(0x11,0x41); w.type_text(query); time.sleep(.2)
    def shot(name):
        subprocess.run(['powershell','-NoProfile','-File',str(root/'screenshot.ps1'),'-Name',name,'-VeyaOnly','-WindowHwnd',str(h)],check=True,creationflags=subprocess.CREATE_NO_WINDOW)
        assert (root/(name+'.png')).exists()
    force(); time.sleep(.5)
    rect=next(item for item in w.windows() if item['hwnd']==h)['rect']; x,y=rect[:2]
    w.click(x+443,y+676); time.sleep(.3)
    shot('final-next-page')
    evidence['next_page_screenshot']='final-next-page.png'
    search('MODAL_TAIL')
    w.click(x+675,y+278)
    evidence['plain_copy_full']=lc.clipboard_text()==full
    assert evidence['plain_copy_full']
    w.click(x+1015,y+277); time.sleep(.2)
    shot('final-full-modal')
    evidence['full_modal_screenshot']='final-full-modal.png'
    w.click(x+850,y+80) # Backdrop closes the modal.
    search('NO_MATCH_OLD_REQUEST')
    search('OUTSIDE_FIRST_PAGE_UNIQUE')
    shot('final-latest-search')
    evidence['latest_query_screenshot']='final-latest-search.png'
    host=lc.start(sys.executable,arguments=(str(root/'wincheck.py'),'host'))
    for _ in range(50):
        try:
            target=json.loads((root/'host.json').read_text())
            if target['pid']==host.pid: break
        except (FileNotFoundError,json.JSONDecodeError): pass
        time.sleep(.05)
    assert target['pid']==host.pid
    w.U.SetWindowPos(h,-2,0,0,0,0,0x13)
    w.foreground(target['window'],target['edit'],'FINAL:')
    w.chord(0x12,0x56)
    w.U.SetWindowPos(h,-1,0,0,0,0,0x13)
    assert int(w.U.GetForegroundWindow() or 0)==h
    w.type_text('OUTSIDE_FIRST_PAGE_UNIQUE')
    w.chord(0x1B)
    w.foreground(target['window'],target['edit'],'FINAL:')
    w.chord(0x12,0x56)
    w.U.SetWindowPos(h,-1,0,0,0,0,0x13)
    assert int(w.U.GetForegroundWindow() or 0)==h
    w.type_text('OUTSIDE_FIRST_PAGE_UNIQUE'); w.chord(0x0D)
    evidence['inserted']=lc.native_text(target['edit'])
    assert evidence['inserted']=='FINAL:OUTSIDE_FIRST_PAGE_UNIQUE'
    evidence['state']=w.state()
    with sqlite3.connect(db) as conn:
        evidence['raw_count']=conn.execute('SELECT count(*) FROM clipboard_record').fetchone()[0]
    assert evidence['raw_count']==42
    (root/'final-windows.json').write_text(json.dumps(evidence,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(evidence,ensure_ascii=False))
finally:
    if h: w.U.SetWindowPos(h,-2,0,0,0,0,0x13)
    if host:
        if target and target['pid']==host.pid: w.U.SendMessageW(target['window'],0x10,0,0)
        try: host.wait(timeout=3)
        except Exception: host.terminate()
    if proc.poll() is None: proc.terminate(); proc.wait(timeout=5)
