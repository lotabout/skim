#!/usr/bin/env python3
# -*- coding: utf-8 -*-

# The integration test of skim
# Modeled after fzf's test: https://github.com/junegunn/fzf/blob/master/test/test_go.rb

import subprocess
import unittest
import os
import time
import re
import inspect
import sys

INPUT_RECORD_SEPARATOR = '\n'
DEFAULT_TIMEOUT = 2000

SCRIPT_PATH = os.path.realpath(__file__)
BASE = os.path.expanduser(os.path.join(os.path.dirname(SCRIPT_PATH), '..'))
os.chdir(BASE)
SK = f"SKIM_DEFAULT_OPTIONS= SKIM_DEFAULT_COMMAND= {BASE}/target/release/sk"

def now_mills():
    return int(round(time.time() * 1000))

def wait(func, timeout_handler=None):
    since = now_mills()
    while now_mills() - since < DEFAULT_TIMEOUT:
        time.sleep(0.02)
        ret = func()
        if ret is not None and ret:
            return
    if timeout_handler is not None:
        timeout_handler()
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
        if hasattr(re, '_pattern_type') and isinstance(val, re._pattern_type):
            method = lambda l: val.match(l)
        if hasattr(re, 'Pattern') and isinstance(val, re.Pattern):
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
        def timeout_handler():
            lines = self.capture()
            print(lines)
        wait(wait_callback, timeout_handler)

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
        self.tmux.send_keys(Key('99'))
        self.tmux.until(lambda ls: ls[-2].startswith('  8146/100000'))
        self.tmux.until(lambda ls: ls[-1].startswith('> 99'))

        self.tmux.send_keys(Ctrl('a'), Key('1'))
        self.tmux.until(lambda ls: ls[-2].startswith('  856/100000'))
        self.tmux.until(lambda ls: ls[-1].startswith('> 199'))

        self.tmux.send_keys(Ctrl('f'), Key('3'))
        self.tmux.until(lambda ls: ls[-2].startswith('  46/100000'))
        self.tmux.until(lambda ls: ls[-1].startswith('> 1939'))

        self.tmux.send_keys(Ctrl('b'), Ctrl('h'))
        self.tmux.until(lambda ls: ls[-2].startswith('  856/100000'))
        self.tmux.until(lambda ls: ls[-1].startswith('> 139'))

        self.tmux.send_keys(Ctrl('e'), Ctrl('b'))
        self.tmux.send_keys(Ctrl('k'))
        self.tmux.until(lambda ls: ls[-4].startswith('> 1390'))
        self.tmux.until(lambda ls: ls[-3].startswith('  139'))

        self.tmux.send_keys(Key('Tab'))
        self.tmux.until(lambda ls: ls[-4].startswith('  1390'))
        self.tmux.until(lambda ls: ls[-3].startswith('> 139'))

        self.tmux.send_keys(Key('BTab'))
        self.tmux.until(lambda ls: ls[-4].startswith('> 1390'))
        self.tmux.until(lambda ls: ls[-3].startswith('  139'))

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

    def test_read0(self):
        self.tmux.send_keys(f"find . | wc -l", Key('Enter'))
        self.tmux.until(lambda lines: re.search('\d', lines[-1]) is not None)
        lines = self.tmux.capture()
        num_of_files = int(lines[-1])

        self.tmux.send_keys(f"find . | {self.sk()}", Key('Enter'))
        self.tmux.until(lambda lines: num_of_files == lines.item_count())
        self.tmux.send_keys(Key('Enter'))

        orig = self.readonce().strip()

        self.tmux.send_keys(f"find . -print0 | {self.sk('--read0')}", Key('Enter'))
        self.tmux.until(lambda lines: num_of_files == lines.item_count())
        self.tmux.send_keys(Key('Enter'))

        self.assertEqual(orig, self.readonce().strip())

    def test_print0(self):
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-m', '--print0')}", Key('Enter'))
        self.tmux.until(lambda lines: 2 == lines.item_count())
        self.tmux.send_keys(Key('BTab'), Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, 'a\0b\0')

    def test_with_nth(self):
        sk_command = self.sk("--delimiter ','", '--with-nth 2..', '--preview', "'echo X{1}Y'")
        self.tmux.send_keys("echo -e 'field1,field2,field3,field4' |" + sk_command, Key('Enter'))
        self.tmux.until(lambda lines: lines.any_include("Xfield1Y"))
        self.tmux.send_keys(Key('Enter'))

    def test_print_query(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('-q 10', '--print-query')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.item_count() == 1000)
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\n10')

    def test_print_cmd(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('--cmd-query 10', '--print-cmd')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.item_count() == 1000)
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\n1')

    def test_print_cmd_and_query(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('-q 10', '--cmd-query cmd', '--print-cmd', '--print-query')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.item_count() == 1000)
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\ncmd\n10')

    def test_hscroll(self):
        # XXXXXXXXXXXXXXXXX..
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"b{'a'*1000}", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].endswith('..'))
        self.tmux.send_keys(Key('Enter'))

        # ..XXXXXXXXXXXXXXXXXM
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].endswith('b'))
        self.tmux.send_keys(Key('Enter'))

        # ..XXXXXXXMXXXXXXX..
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b{'a'*1000}", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> ..'))
        self.tmux.until(lambda lines: lines[-3].endswith('..'))
        self.tmux.send_keys(Key('Enter'))

    def test_no_hscroll(self):
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b', '--no-hscroll')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))
        self.tmux.send_keys(Key('Enter'))

    def test_tabstop(self):
        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk()}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> a       b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk('--tabstop 1')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> a b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'aa\\tb' | {self.sk('--tabstop 2')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> aa  b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'aa\\tb' | {self.sk('--tabstop 3')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> aa b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk('--tabstop 4')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.until(lambda lines: lines[-3].startswith('> a   b'))
        self.tmux.send_keys(Key('Enter'))

    def test_reserved_options(self):
        # --extended
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--extended')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --algo=TYPE
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--algo=TYPE')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --literal
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--literal')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --no-mouse
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-mouse')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --cycle
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--cycle')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --hscroll-off=COL
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--hscroll-off=COL')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --filepath-word
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--filepath-word')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --jump-labels=CHARS
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--jump-labels=CHARS')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --border
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--border')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --inline-info
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--inline-info')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --header=STR
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--header=STR')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --header-lines=N
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--header-lines=N')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --no-bold
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-bold')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --history=FILE
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--history=FILE')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --history-size=N
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--history-size=10')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --sync
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--sync')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --no-sort
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-sort')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --select-1
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--select-1')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-1')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --exit-0
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--exit-0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        # --filter
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--filter')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-f')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

    def test_multiple_option_values_should_be_accepted(self):
        # normally we'll put some default options to SKIM_DEFAULT_OPTIONS and override it in command
        # line. this test will ensure multiple values are accepted.

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--bind=ctrl-a:cancel --bind ctrl-b:cancel')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--prompt a --prompt b -p c')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('c'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-i --cmd-prompt a --cmd-prompt b')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--expect=ctrl-a --expect=ctrl-v')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--tiebreak=index --tiebreak=score')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--cmd asdf --cmd find')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--query asdf -q xyz')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('> xyz'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-i --cmd-query asdf --cmd-query xyz')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('c> xyz'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--delimiter , --delimiter . -d ,')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--nth 1,2 --nth=1,3 -n 1,3')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--with-nth 1,2 --with-nth=1,3')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-I {} -I XX')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--color base --color light')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--margin 30% --margin 0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--min-height 30% --min-height 10')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--height 30% --height 10')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"""echo -e 'a\\nb' | {self.sk('--preview "ls {}" --preview "cat {}"')}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--preview-window up --preview-window down')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--multi -m')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-multi --no-multi')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--tac --tac')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--ansi --ansi')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--exact -e')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--interactive -i')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('c>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--regex --regex')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--reverse --reverse')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[0].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--literal --literal')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-mouse --no-mouse')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--cycle --cycle')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-hscroll --no-hscroll')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--filepath-word --filepath-word')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--border --border')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--inline-info --inline-info')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-bold --no-bold')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--print-query --print-query')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--print-cmd --print-cmd')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--read0 --read0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--print0 --print0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--sync --sync')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--extended --extended')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--no-sort --no-sort')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--select-1 --select-1')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--exit-0 --exit-0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('--filter --filter')}", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('>'))
        self.tmux.send_keys(Key('Enter'))

    def test_single_quote_of_preview_command(self):
        # echo "'\"ABC\"'" | sk --preview="echo X{}X" => X'"ABC"'X
        echo_command = '''echo "'\\"ABC\\"'" | '''
        sk_command = self.sk('--preview=\"echo X{}X\"')
        command = echo_command + sk_command
        self.tmux.send_keys(command, Key('Enter'))
        self.tmux.until(lambda lines: lines.any_include('''X'"ABC"'X'''))

        # echo "'\"ABC\"'" | sk --preview="echo X\{}X" => X{}X
        echo_command = '''echo "'\\"ABC\\"'" | '''
        sk_command = self.sk('--preview=\"echo X\\{}X\"')
        command = echo_command + sk_command
        self.tmux.send_keys(command, Key('Enter'))
        self.tmux.until(lambda lines: lines.any_include('''X{}X'''))


if __name__ == '__main__':
    unittest.main()
