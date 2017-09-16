#!/usr/bin/env python3
# -*- coding: utf-8 -*-

# The integration test of skim
# Modeled after fzf's test: https://github.com/junegunn/fzf/blob/master/test/test_go.rb

import subprocess
import unittest
import os
from datetime import datetime
import time
import re
import inspect

INPUT_RECORD_SEPARATOR = '\n'
DEFAULT_TIMEOUT = 20

SCRIPT_PATH = os.path.realpath(__file__)
BASE = os.path.expanduser(os.path.join(os.path.dirname(SCRIPT_PATH), '../'))
SK = f"SKIM_DEFAULT_OPTIONS= SKIM_DEFAULT_COMMAND= {BASE}/target/debug/sk"

def wait(func):
    since = datetime.now()
    while (datetime.now() - since).microseconds < DEFAULT_TIMEOUT * 1000:
        ret = func()
        if ret is not None and ret:
            return
        time.sleep(0.0005)
    raise BaseException('Timeout on wait')

class Key(object):
    """Represent a key to send to tmux"""
    def __init__(self, key):
        super(Key, self).__init__()
        self.key = key
    def __repr__(self):
        return self.key

class Ctrl(Key):
    """Represent a control key"""
    def __init__(self, key):
        super(Ctrl, self).__init__(key)
    def __repr__(self):
        return f'C-{self.key.upper()}'

class Alt(Key):
    """Represent an alt key"""
    def __init__(self, key):
        super(Alt, self).__init__(key)
    def __repr__(self):
        return f'Escape {self.key.upper()}'

class TmuxOutput(list):
    """A list that contains the output of tmux"""
    RE = re.compile(r'^. ([0-9]+)/([0-9]+)(?: \[([0-9]+)\])?')
    def __init__(self, iteratable=[]):
        super(TmuxOutput, self).__init__(iteratable)
        self._counts = None

    def counts(self):
        if self._counts is not None:
            return self._counts

        ret = (0, 0, 0)
        for line in self:
            mat = TmuxOutput.RE.match(line)
            if mat is not None:
                ret = tuple(map(lambda x: int(x) if x is not None else 0, mat.groups()))
                break;
        self._counts = ret
        return ret

    def match_count(self):
        return self.counts()[0]

    def item_count(self):
        return self.counts()[1]

    def select_count(self):
        return self.counts()[2]

    def any_include(self, val):
        if isinstance(val, re._pattern_type):
            method = lambda l: val.match(l)
        else:
            method = lambda l: l.find(val) >= 0
        for line in self:
            if method(line):
                return True
        return False

class Tmux(object):
    TEMPNAME = '/tmp/skim-test.txt'

    """Object to manipulate tmux and get result"""
    def __init__(self):
        super(Tmux, self).__init__()
        self.win = self._go('new-window -d -P -F "#I"')[0]
        self._go(f"set-window-option -t {self.win} pane-base-index 0")
        self.lines = int(subprocess.check_output('tput lines', shell=True).decode('utf8').strip())

    def _go(self, *args):
        """Run tmux command and return result in list of strings (lines)

        :returns: List<String>
        """

        ret = subprocess.check_output(f"tmux {' '.join(args)}", shell=True)
        return ret.decode('utf8').split(INPUT_RECORD_SEPARATOR)

    def kill(self):
        self._go(f"kill-window -t {self.win} 2> /dev/null")

    def send_keys(self, *args, pane=None):
        if pane is not None:
            self._go(f'select-window -t {self.win}')
            target = '{self.win}.{pane}'
        else:
            target = self.win

        for key in args:
            if key is not None:
                self._go(f'send-keys -t {target} "{key}"')

    def paste(self, content):
        content = content.replace("'", "'\\''")
        subprocess.run(f'''tmux setb '{content}'\; pasteb -t {self.win}\; send-keys -t {self.win} Enter''', shell=True)

    def capture(self, pane = 0):
        def save_capture():
            try:
                self._go(f'capture-pane -t {self.win}.{pane}\; save-buffer {Tmux.TEMPNAME} 2> /dev/null')
                return True
            except subprocess.CalledProcessError as ex:
                return False

        if os.path.exists(Tmux.TEMPNAME):
            os.remove(Tmux.TEMPNAME)

        wait(save_capture)
        with open(Tmux.TEMPNAME) as fp:
            content = fp.read()
            return TmuxOutput(content.rstrip().split(INPUT_RECORD_SEPARATOR))

    def until(self, predicate, refresh = False, pane = 0):
        def wait_callback():
            lines = self.capture()
            pred = predicate(lines)
            if pred:
                self.send_keys(Ctrl('l') if refresh else None)
            return pred
        wait(wait_callback)

    def prepare(self):
        try:
            self.send_keys(Ctrl('u'), Key('hello'))
            self.until(lambda lines: lines[-1].endswith('hello'))
        except Exception as e:
            raise e
        self.send_keys(Ctrl('u'))

class TestBase(unittest.TestCase):
    TEMPNAME = '/tmp/output'
    def __init__(self, *args, **kwargs):
        super(TestBase, self).__init__(*args, **kwargs)
        self._temp_suffix = 0

    def tempname(self):
        curframe = inspect.currentframe()
        frames = inspect.getouterframes(curframe)

        names = [f.function for f in frames if f.function.startswith('test_')]
        fun_name = names[0] if len(names) > 0 else 'test'

        return '-'.join((TestBase.TEMPNAME, fun_name, str(self._temp_suffix)))

    def writelines(self, path, lines):
        if os.path.exists(path):
            os.remove(path)

        with open(path, 'w') as fp:
            fp.writelines(lines)

    def readonce(self):
        try:
            wait(lambda: os.path.exists(self.tempname()))
            with open(self.tempname()) as fp:
                return fp.read()
        finally:
            if os.path.exists(path):
                os.remove(path)
            self._temp_suffix += 1
            tmux.prepare

    def sk(self, *opts):
        tmp = self.tempname()
        return f'{SK} {" ".join(map(str, opts))} > {tmp}.tmp; mv {tmp}.tmp {tmp}'


class TestSkim(TestBase):
    def setUp(self):
        self.tmux = Tmux()

    def tearDown(self):
        self.tmux.kill()
        pass

    def test_vanilla(self):
        self.tmux.send_keys(Key(f'seq 1 100000 | {self.sk()}'), Key('Enter'))
        self.tmux.until(lambda lines: re.match(r'^>', lines[-1]) and re.match(r'^  100000', lines[-2]))
        lines = self.tmux.capture()
        self.assertEqual('  2', lines[-4])
        self.assertEqual('> 1', lines[-3])
        self.assertTrue(re.match('^  100000/100000 *0', lines[-2]))
        self.assertEqual('>',   lines[-1])

if __name__ == '__main__':
    unittest.main()
