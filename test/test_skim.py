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
DEFAULT_TIMEOUT = 1000

SCRIPT_PATH = os.path.realpath(__file__)
BASE = os.path.expanduser(os.path.join(os.path.dirname(SCRIPT_PATH), '../'))
os.chdir(BASE)
SK = f"SKIM_DEFAULT_OPTIONS= SKIM_DEFAULT_COMMAND= {BASE}/target/release/sk"

def wait(func):
    since = datetime.now()
    while (datetime.now() - since).microseconds < DEFAULT_TIMEOUT * 1000:
        ret = func()
        if ret is not None and ret:
            return
        time.sleep(0.0005)
    raise BaseException('Timeout on wait')

class Shell(object):
    """The shell configurations for tmux tests"""
    def __init__(self):
        super(Shell, self).__init__()
    def unsets():
        return 'unset SKIM_DEFAULT_COMMAND SKIM_DEFAULT_OPTIONS;'
    def bash():
        return 'PS1= PROMPT_COMMAND= bash --rcfile None'
    def zsh():
        return 'PS1= PROMPT_COMMAND= HISTSIZE=100 zsh -f'

class Key(object):
    """Represent a key to send to tmux"""
    def __init__(self, key):
        super(Key, self).__init__()
        self.key = key
    def __repr__(self):
        return '"' + self.key + '"'

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
        return f'M-{self.key}'

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
    def __init__(self, shell = 'bash'):
        super(Tmux, self).__init__()

        if shell == 'bash':
            shell_cmd = Shell.unsets() + Shell.bash()
        elif shell == 'zsh':
            shell_cmd = Shell.unsets() + Shell.zsh()
        else:
            raise BaseException('unknown shell')

        self.win = self._go(f"new-window -d -P -F '#I' '{shell_cmd}'")[0]
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
            if key is None:
                continue
            elif isinstance(key, Key):
                self._go(f'send-keys -t {target} {key}')
            else:
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
        path = self.tempname()
        try:
            wait(lambda: os.path.exists(path))
            with open(path) as fp:
                return fp.read()
        finally:
            if os.path.exists(path):
                os.remove(path)
            self._temp_suffix += 1
            self.tmux.prepare()

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

        # testing basic key binding
        self.tmux.send_keys(Key('99'), Ctrl('a'), Key('1'), Ctrl('f'), Key('3'), Ctrl('b'),
                Ctrl('h'), Ctrl('e'), Ctrl('b'), Ctrl('k'), Key('Tab'), Key('BTab'))

        self.tmux.until(lambda ls: ls[-2].startswith('  856/100000'))
        self.tmux.until(lambda ls: ls[-4] == '> 1390')
        lines = self.tmux.capture()
        self.assertEqual('> 1390', lines[-4])
        self.assertEqual('  139', lines[-3])
        self.assertTrue(lines[-2].startswith('  856/100000'))
        self.assertEqual('> 139',   lines[-1])

        self.tmux.send_keys(Key('Enter'))
        self.assertEqual('1390', self.readonce().strip())

    def test_default_command(self):
        self.tmux.send_keys(self.sk().replace('SKIM_DEFAULT_COMMAND=', "SKIM_DEFAULT_COMMAND='echo hello'"))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.until(lambda lines: lines[-2].startswith('  1/1'))
        self.tmux.send_keys(Key('Enter'))
        self.assertEqual('hello', self.readonce().strip())

    def test_key_bindings(self):
        self.tmux.send_keys(f"{SK} -q 'foo bar foo-bar'", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))

        # Ctrl-A
        self.tmux.send_keys(Ctrl('a'), Key('('))
        self.tmux.until(lambda lines: lines[-1] == '> (foo bar foo-bar')

        ## Meta-F
        self.tmux.send_keys(Alt('f'), Key(')'))
        self.tmux.until(lambda lines: lines[-1] == '> (foo) bar foo-bar')

        # CTRL-B
        self.tmux.send_keys(Ctrl('b'), 'var')
        self.tmux.until(lambda lines: lines[-1] == '> (foovar) bar foo-bar')

        # Left, CTRL-D
        self.tmux.send_keys(Key('Left'), Key('Left'), Ctrl('d'))
        self.tmux.until(lambda lines: lines[-1] == '> (foovr) bar foo-bar')

        # # META-BS
        self.tmux.send_keys(Alt('BSpace'))
        self.tmux.until(lambda lines: lines[-1] == '> (r) bar foo-bar')

        # # # CTRL-Y
        self.tmux.send_keys(Ctrl('y'), Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == '> (foovfoovr) bar foo-bar')

        # META-B
        self.tmux.send_keys(Alt('b'), Key('Space'), Key('Space'))
        self.tmux.until(lambda lines: lines[-1] == '> (  foovfoovr) bar foo-bar')

        # CTRL-F / Right
        self.tmux.send_keys( Ctrl('f'), Key('Right'), '/')
        self.tmux.until(lambda lines: lines[-1] == '> (  fo/ovfoovr) bar foo-bar')

        # CTRL-H / BS
        self.tmux.send_keys( Ctrl('h'), Key('BSpace'))
        self.tmux.until(lambda lines: lines[-1] == '> (  fovfoovr) bar foo-bar')

        # CTRL-E
        self.tmux.send_keys(Ctrl('e'), 'baz')
        self.tmux.until(lambda lines: lines[-1] == '> (  fovfoovr) bar foo-barbaz')

        # CTRL-U
        self.tmux.send_keys( Ctrl('u'))
        self.tmux.until(lambda lines: lines[-1] == '>')

        # CTRL-Y
        self.tmux.send_keys( Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == '> (  fovfoovr) bar foo-barbaz')

        # CTRL-W
        self.tmux.send_keys( Ctrl('w'), 'bar-foo')
        self.tmux.until(lambda lines: lines[-1] == '> (  fovfoovr) bar bar-foo')

        # # META-D
        self.tmux.send_keys(Alt('b'), Alt('b'), Alt('d'), Ctrl('a'), Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == '> bar(  fovfoovr) bar -foo')

        # CTRL-M
        self.tmux.send_keys(Ctrl('m'))
        self.tmux.until(lambda lines: not lines[-1].startswith('>'))

if __name__ == '__main__':
    unittest.main()
