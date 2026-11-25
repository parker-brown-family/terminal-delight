#!/usr/bin/env python3
"""Exercise real native HTML exposure on a private Xvfb display and temporary file.

No input goes to the operator's display. Pass --bin, --out and --vulkan (an
installed software Vulkan library, such as Mesa's libvulkan_lvp.so). --brief
defaults to scripts/fixtures/reading-smoke.html, a synthetic decision brief with
one passage of each kind the reading clock times (opening, section, figure,
entry) and no reading sessions. Rebuild it from reading-smoke.src.html with the
briefing skill's `brief.py html` when the skill's page format changes.

A debug build can take longer than the control socket's two seconds to answer.
The script then asks a read (`doc notes`) again, and accepts a slow action
without repeating it, since the checks after every action prove its effect.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import time


def wait_for(function, seconds=45):
    end = time.monotonic() + seconds
    last = None
    while time.monotonic() < end:
        try:
            value = function()
            if value:
                return value
        except (OSError, ValueError, KeyError, AssertionError, subprocess.CalledProcessError) as exc:
            last = exc
        time.sleep(.15)
    raise AssertionError(f'Private reader did not reach the expected state: {last}')


def main():
    parser = argparse.ArgumentParser()
    for flag in ('bin', 'out', 'vulkan'):
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--brief', type=Path,
                        default=Path(__file__).resolve().parent / 'fixtures' / 'reading-smoke.html')
    args = parser.parse_args()
    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=False)
    for name in ('runtime', 'config', 'state', 'cache', 'bin'):
        (root / name).mkdir(mode=0o700)
    for opener in ('xdg-open', 'uwsm-app'):
        stub = root / 'bin' / opener
        stub.write_text('#!/bin/sh\nexit 1\n')
        stub.chmod(0o700)
    page = root / 'reading.html'
    page.write_bytes(args.brief.read_bytes())
    # Input must be an unannotated synthetic fixture, never a real reader's page.
    assert '"sessions": []' in page.read_text() or '"sessions":[]' in page.read_text()
    icd = root / 'software-vulkan.json'
    icd.write_text(json.dumps({'file_format_version': '1.0.1', 'ICD': {
        'api_version': '1.3.0', 'library_path': str(args.vulkan.resolve())}}))
    env = {k: v for k, v in os.environ.items() if not k.startswith(('TD_', 'WAYLAND_', 'HYPRLAND_'))}
    env.update(XDG_RUNTIME_DIR=str(root / 'runtime'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_STATE_HOME=str(root / 'state'), XDG_CACHE_HOME=str(root / 'cache'),
               TD_SESSION='reading-smoke', TD_DOCDEBUG='1', TD_READING_RECORDING='1', TD_MCP_SCRIPTED='1',
               VK_DRIVER_FILES=str(icd), VK_ICD_FILENAMES=str(icd),
               LIBGL_ALWAYS_SOFTWARE='1', LP_NUM_THREADS='2', XDG_SESSION_TYPE='x11',
               PATH=str(root / 'bin') + ':' + os.environ['PATH'])
    display_read, display_write = os.pipe()
    display_log = (root / 'display.log').open('w')
    display = subprocess.Popen(['Xvfb', '-displayfd', str(display_write), '-screen', '0',
                                '2400x1400x24', '-nolisten', 'tcp', '-ac'],
                               pass_fds=(display_write,), stdout=display_log, stderr=display_log)
    os.close(display_write)
    with os.fdopen(display_read) as pipe:
        env['DISPLAY'] = ':' + pipe.readline().strip()
    window = None
    report = {}
    try:
        log = (root / 'native.log').open('w')
        window = subprocess.Popen([str(args.bin.resolve())], env=env, stdout=log, stderr=log,
                                  start_new_session=True)
        ctl_path = root / f'runtime/terminal-delight/ctl-{window.pid}.sock'

        def ask(command):
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(10)
                connection.connect(str(ctl_path))
                connection.sendall((command + '\n').encode())
                data = bytearray()
                while not data.endswith(b'\n'):
                    chunk = connection.recv(65536)
                    if not chunk: break
                    data.extend(chunk)
                return data.decode()

        def ctl(command, strict=True):
            result = ask(command)
            # A debug build can answer after the socket's two seconds. A read
            # is asked again; an action is not, because it may already have
            # happened, and the checks that follow it prove whether it did.
            for _ in range(20):
                if 'window did not answer' not in result or not command.startswith('doc notes'):
                    break
                time.sleep(.5)
                result = ask(command)
            if strict and 'window did not answer' in result and not command.startswith('doc notes'):
                return ''
            if strict:
                assert result.startswith('ok'), result
                return result[2:].strip()
            return result.strip()

        wait_for(lambda: ctl_path.exists())
        wid = wait_for(lambda: subprocess.check_output(
            ['xdotool', 'search', '--onlyvisible', '--pid', str(window.pid)], env=env,
            stderr=subprocess.DEVNULL).decode().splitlines())[-1]
        subprocess.run(['xdotool', 'windowmove', wid, '0', '0'], env=env, check=True)
        subprocess.run(['xdotool', 'windowsize', wid, '1360', '940'], env=env, check=True)
        subprocess.run(['xdotool', 'windowfocus', '--sync', wid], env=env, check=True)
        wait_for(lambda: 'window did not answer' not in ctl('doc notes', strict=False), seconds=120)
        # A slow build answers after the socket's two seconds; the open still
        # happens, and the wait for an active reading below is what proves it.
        opened = ctl('doc here ' + str(page), strict=False)
        assert opened.startswith('ok') or 'window did not answer' in opened, opened

        def notes():
            result = ctl('doc notes')
            return json.loads(result[result.index('{'):])

        def reading():
            return notes()['reading']

        def key(chord):
            subprocess.run(['xdotool', 'key', '--clearmodifiers', chord], env=env, check=True)

        initial = wait_for(lambda: (r if (r := reading()).get('active') else None), seconds=90)
        report['initial'] = initial
        assert {'opening','section','figure','entry'}.issubset({t['kind'] for t in initial['session']['targets']})
        # The note goes on the page's own first section, whatever its id.
        first_section = next(t['anchor'] for t in initial['session']['targets'] if t['kind'] == 'section')
        time.sleep(6)
        first = reading()
        assert first['session'] and first['saved'], first
        assert any((t['visible_ms'] or 0) >= 3000 for t in first['session']['targets']), first
        # At rest the sections are pips beside the progress rail and nothing
        # sits on the page. The list opens only with the pointer on the
        # document's right edge. This pointer is the private display's.
        assert first['rail_pips'] > 0 and first['rail_targets'] == first['rail_pips'], first
        assert not first['rail_open'], first

        def shoot(name, size='1400x1000'):
            subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'x11grab', '-video_size', size,
                            '-i', env['DISPLAY'], '-frames:v', '1', str(root / name)],
                           env=env, check=True, timeout=30)

        def pointer(x, y):
            subprocess.run(['xdotool', 'mousemove', str(x), str(y)], env=env, check=True)

        shoot('sections-rest.png')
        edge = None
        for x in range(1356, 500, -4):
            pointer(x, 470)
            time.sleep(.08)
            if reading()['rail_open']:
                edge = x
                break
        assert edge is not None, 'the pointer never opened the sections list'
        opened = wait_for(lambda: (r if (r := reading())['rail_targets'] > r['rail_pips'] else None))
        time.sleep(.5)
        shoot('sections-open.png')
        pointer(edge - 600, 470)
        wait_for(lambda: not reading()['rail_open'])
        assert reading()['rail_targets'] == first['rail_pips']
        report['sections'] = {'pips': first['rail_pips'], 'listRows': opened['rail_targets'] - opened['rail_pips'],
                              'openedAtX': edge}
        pointer(2000, 1200)
        key('alt+r')
        time.sleep(2)
        def copy_timing():
            if notes().get('said') is None: key('y')
            return notes().get('said') is not None
        wait_for(copy_timing, seconds=90)
        assert notes()['notes'] == 0
        assert 'VISIBLE TIME' in notes()['map']
        report['timeOnlyMap'] = True
        key('Escape')
        # Focus the private X root. This cannot affect the operator's seat.
        subprocess.run(['xdotool', 'windowfocus', '--sync', '0'], env=env, check=True)
        paused = wait_for(lambda: (r if not (r := reading())['active'] else None))
        before = [t['visible_ms'] for t in paused['session']['targets']]
        time.sleep(3)
        assert [t['visible_ms'] for t in reading()['session']['targets']] == before
        report['paused'] = paused
        subprocess.run(['xdotool', 'windowfocus', '--sync', wid], env=env, check=True)
        wait_for(lambda: reading()['active'])
        ctl('doc scroll 700')
        time.sleep(3)
        report['scrolled'] = reading()
        identity = reading()['session']['session']
        key('alt+r')
        wait_for(lambda: reading()['active'])
        time.sleep(2)
        assert reading()['session']['session'] == identity, 'FOCUS keeps one session owner'
        def select_passage():
            if notes()['selected'] is None: key('Tab')
            return notes()['selected'] is not None
        wait_for(select_passage, seconds=90)
        key('Return')
        wait_for(lambda: notes()['open'] is not None)
        modal = wait_for(lambda: (r if not (r := reading())['active'] else None))
        modal_time = [t['visible_ms'] for t in modal['session']['targets']]
        time.sleep(2)
        assert [t['visible_ms'] for t in reading()['session']['targets']] == modal_time
        report['noteModalPauses'] = True
        key('Escape')
        wait_for(lambda: reading()['active'])
        key('ctrl+equal')
        time.sleep(3)
        assert reading()['session']['session'] == identity, 'Zoom keeps one session owner'
        key('ctrl+minus')
        key('Escape')
        subprocess.run(['xdotool', 'windowsize', wid, '900', '940'], env=env, check=True)
        time.sleep(3)
        assert reading()['session']['session'] == identity, 'Resize keeps one session owner'
        ctl(f'doc note add {first_section} Native timing and notes coexist.')
        wait_for(lambda: notes()['unsaved'] == 0)
        assert 'VISIBLE TIME' in notes()['map']
        assert 'Native timing and notes coexist.' in page.read_text()
        assert all(s['host'] == 'terminal-delight' for s in reading()['saved']['sessions']), 'Hidden probes do not add browser sessions'
        # Switch to another pane and then another tab. Inspect the original
        # file while that document is inactive; do not query an arbitrary pane.
        import re
        def stored_targets():
            match = re.search(r'<script[^>]*id="report-reading"[^>]*>(.*?)</script>', page.read_text(), re.S)
            record = json.loads(match[1])
            return next(x['targets'] for x in record['sessions'] if x['session'] == identity)
        key('alt+v')
        time.sleep(6)
        pane_times = stored_targets()
        time.sleep(2)
        assert stored_targets() == pane_times, 'A visible document in another pane pauses'
        key('alt+Left')
        wait_for(lambda: reading()['active'])
        key('alt+Right')
        key('alt+w')
        wait_for(lambda: reading()['active'])
        key('ctrl+shift+t')
        key('Escape')
        time.sleep(6)
        tab_times = stored_targets()
        time.sleep(2)
        assert stored_targets() == tab_times, 'A hidden tab pauses'
        key('ctrl+Page_Up')
        wait_for(lambda: reading()['active'])
        report['paneAndTabPause'] = True
        report['focusZoomResizeOneOwner'] = True
        report['nativeNoteAndMap'] = True
        ctl('doc scroll -10000')
        time.sleep(2)
        subprocess.run(['ffmpeg', '-v', 'error', '-f', 'x11grab', '-video_size', '1400x1000',
                        '-i', env['DISPLAY'], '-frames:v', '1', str(root / 'native.png')],
                       env=env, check=True, timeout=30)
        old_session = report['scrolled']['session']['session']
        ctl('doc close')
        wait_for(lambda: old_session in page.read_text())
        ctl('doc here ' + str(page))
        reopened = wait_for(lambda: (r if (r := reading()).get('saved') and r.get('session') and r['active'] else None), seconds=90)
        assert any(s['session'] == old_session for s in reopened['saved']['sessions'])
        assert reopened['session']['session'] != old_session
        report['reopened'] = reopened
        # A full Document face at desktop width: the same pips, and no panel
        # (this width used to draw a fixed column down the left).
        ctl('doc close')
        subprocess.run(['xdotool', 'windowsize', wid, '2200', '1300'], env=env, check=True)
        ctl('doc beside ' + str(page))
        key('alt+Left')
        key('alt+w')
        wide = wait_for(lambda: (r if (r := reading()).get('active') and r['rail_pips'] > 0 and not r['rail_open'] and r.get('saved') and r.get('session') and any((t['visible_ms'] or 0) >= 2000 for t in r['session']['targets']) else None), seconds=90)
        report['wideSections'] = wide
        time.sleep(2)
        subprocess.run(['ffmpeg', '-v', 'error', '-f', 'x11grab', '-video_size', '2400x1400',
                        '-i', env['DISPLAY'], '-frames:v', '1', str(root / 'native-wide.png')],
                       env=env, check=True, timeout=30)
        report['result'] = 'passed'
        (root / 'proof.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'result': 'passed', 'proof': str(root / 'proof.json'), 'screenshot': str(root / 'native.png')}))
    finally:
        if window is not None:
            os.killpg(window.pid, signal.SIGTERM)
            try:
                window.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(window.pid, signal.SIGKILL)
        display.terminate()
        display.wait(10)


if __name__ == '__main__':
    main()
