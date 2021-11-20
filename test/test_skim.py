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
DEFAULT_TIMEOUT = 3000

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
        return f'M-{self.key}'

class TmuxOutput(list):
    """A list that contains the output of tmux"""
    # match the status line
    # normal:  `| 10/219 [2]               8/0.`
    # inline:  `> query < 10/219 [2]       8/0.`
    # preview: `> query < 10/219 [2]       8/0.│...`
    RE = re.compile(r'(?:^|[^<-]*). ([0-9]+)/([0-9]+)(?:/[A-Z]*)?(?: \[([0-9]+)\])? *([0-9]+)/(-?[0-9]+)(\.)?(?: │)? *$')
    def __init__(self, iteratable=[]):
        super(TmuxOutput, self).__init__(iteratable)
        self._counts = None

    def counts(self):
        if self._counts is not None:
            return self._counts

        # match_count item_count select_count item_cursor matcher_stopped
        ret = (0, 0, 0, 0, 0, '.')
        for line in self:
            mat = TmuxOutput.RE.match(line)
            if mat is not None:
                ret = mat.groups()
                break;
        self._counts = ret
        return ret

    def match_count(self):
        count = self.counts()[0]
        return int(count) if count is not None else None

    def item_count(self):
        count = self.counts()[1]
        return int(count) if count is not None else None

    def select_count(self):
        count = self.counts()[2]
        return int(count) if count is not None else None

    def item_index(self):
        count = self.counts()[3]
        return int(count) if count is not None else None

    def hscroll(self):
        count = self.counts()[4]
        return int(count) if count is not None else None

    def matcher_stopped(self):
        return self.counts()[5] != '.'

    def ready_with_lines(self, lines):
        return self.item_count() == lines and self.matcher_stopped()

    def ready_with_matches(self, matches):
        return self.match_count() == matches and self.matcher_stopped()

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

        self.win = self._go("new-window", "-d", "-P", "-F", "#I", f"{shell_cmd}")[0]
        self._go("set-window-option", "-t", f"{self.win}", "pane-base-index", "0")
        self.lines = int(subprocess.check_output('tput lines', shell=True).decode('utf8').strip())

    def _go(self, *args, **kwargs):
        """Run tmux command and return result in list of strings (lines)

        :returns: List<String>
        """
        ret = subprocess.check_output(["tmux"] + list(args))
        return ret.decode('utf8').split(INPUT_RECORD_SEPARATOR)

    def kill(self):
        self._go("kill-window", "-t", f"{self.win}", stderr=subprocess.DEVNULL)

    def send_keys(self, *args, pane=None):
        if pane is not None:
            self._go('select-window', '-t', f'{self.win}')
            target = '{self.win}.{pane}'
        else:
            target = self.win

        for key in args:
            if key is None:
                continue
            else:
                self._go('send-keys', '-t', f'{target}', f'{key}')
            time.sleep(0.01)

    def paste(self, content):
        subprocess.run(["tmux", "setb", f"{content}", ";",
                        "pasteb", "-t", f"{self.win}", ";",
                        "send-keys", "-t", f"{self.win}", "Enter"])

    def capture(self, pane = 0):
        def save_capture():
            try:
                self._go('capture-pane', '-t', f'{self.win}.{pane}', stderr=subprocess.DEVNULL)
                self._go("save-buffer", f"{Tmux.TEMPNAME}", stderr=subprocess.DEVNULL)
                return True
            except subprocess.CalledProcessError as ex:
                return False

        if os.path.exists(Tmux.TEMPNAME):
            os.remove(Tmux.TEMPNAME)

        wait(save_capture)
        with open(Tmux.TEMPNAME) as fp:
            content = fp.read()
            return TmuxOutput(content.rstrip().split(INPUT_RECORD_SEPARATOR))

    def until(self, predicate, refresh = False, pane = 0, debug_info = None):
        def wait_callback():
            lines = self.capture()
            pred = predicate(lines)
            if pred:
                self.send_keys(Ctrl('l') if refresh else None)
            return pred
        def timeout_handler():
            lines = self.capture()
            print(lines)
            if debug_info:
                print(debug_info)
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

    def command_until(self, until_predicate, sk_options, stdin="echo -e 'a1\\na2\\na3'"):
        command_keys = stdin + " | " + self.sk(*sk_options)
        self.tmux.send_keys(command_keys)
        self.tmux.send_keys(Key("Enter"))
        self.tmux.until(until_predicate, debug_info="SK args: {}".format(sk_options))
        self.tmux.send_keys(Key('Enter'))


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
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
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

    def test_key_bindings_interactive(self):
        self.tmux.send_keys(f"{SK} -i --cmd-query 'foo bar foo-bar'", Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('c>'))

        # Ctrl-A
        self.tmux.send_keys(Ctrl('a'), Key('('))
        self.tmux.until(lambda lines: lines[-1] == 'c> (foo bar foo-bar')

        ## Meta-F
        self.tmux.send_keys(Alt('f'), Key(')'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (foo) bar foo-bar')

        # CTRL-B
        self.tmux.send_keys(Ctrl('b'), 'var')
        self.tmux.until(lambda lines: lines[-1] == 'c> (foovar) bar foo-bar')

        # Left, CTRL-D
        self.tmux.send_keys(Key('Left'), Key('Left'), Ctrl('d'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (foovr) bar foo-bar')

        # # META-BS
        self.tmux.send_keys(Alt('BSpace'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (r) bar foo-bar')

        # # # CTRL-Y
        self.tmux.send_keys(Ctrl('y'), Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (foovfoovr) bar foo-bar')

        # META-B
        self.tmux.send_keys(Alt('b'), Key('Space'), Key('Space'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (  foovfoovr) bar foo-bar')

        # CTRL-F / Right
        self.tmux.send_keys( Ctrl('f'), Key('Right'), '/')
        self.tmux.until(lambda lines: lines[-1] == 'c> (  fo/ovfoovr) bar foo-bar')

        # CTRL-H / BS
        self.tmux.send_keys( Ctrl('h'), Key('BSpace'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (  fovfoovr) bar foo-bar')

        # CTRL-E
        self.tmux.send_keys(Ctrl('e'), 'baz')
        self.tmux.until(lambda lines: lines[-1] == 'c> (  fovfoovr) bar foo-barbaz')

        # CTRL-U
        self.tmux.send_keys( Ctrl('u'))
        self.tmux.until(lambda lines: lines[-1] == 'c>')

        # CTRL-Y
        self.tmux.send_keys( Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == 'c> (  fovfoovr) bar foo-barbaz')

        # CTRL-W
        self.tmux.send_keys( Ctrl('w'), 'bar-foo')
        self.tmux.until(lambda lines: lines[-1] == 'c> (  fovfoovr) bar bar-foo')

        # # META-D
        self.tmux.send_keys(Alt('b'), Alt('b'), Alt('d'), Ctrl('a'), Ctrl('y'))
        self.tmux.until(lambda lines: lines[-1] == 'c> bar(  fovfoovr) bar -foo')

        # CTRL-M
        self.tmux.send_keys(Ctrl('m'))
        self.tmux.until(lambda lines: not lines[-1].startswith('c>'))

    def test_read0(self):
        nfiles = subprocess.check_output("find .", shell=True).decode("utf-8").strip().split("\n")
        num_of_files = len(nfiles)

        self.tmux.send_keys(f"find . | {self.sk()}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(num_of_files))
        self.tmux.send_keys(Key('Enter'))

        orig = self.readonce().strip()

        self.tmux.send_keys(f"find . -print0 | {self.sk('--read0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(num_of_files))
        self.tmux.send_keys(Key('Enter'))

        self.assertEqual(orig, self.readonce().strip())

    def test_print0(self):
        self.tmux.send_keys(f"echo -e 'a\\nb' | {self.sk('-m', '--print0')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(2))
        self.tmux.send_keys(Key('BTab'), Key('BTab'), Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, 'a\0b\0')

        self.tmux.send_keys(f"echo -e 'a\\naa\\nb' | {self.sk('-f a', '--print0')}", Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, 'a\0aa\0')

    def test_with_nth_preview(self):
        sk_command = self.sk("--delimiter ','", '--with-nth 2..', '--preview', "'echo X{1}Y'")
        self.tmux.send_keys("echo -e 'field1,field2,field3,field4' |" + sk_command, Key('Enter'))
        self.tmux.until(lambda lines: lines.any_include("Xfield1Y"))
        self.tmux.send_keys(Key('Enter'))

    def test_with_nth(self):
        # fields, expected
        tests = [
                ('1', 'field1,'),
                ('2', 'field2,'),
                ('3', 'field3,'),
                ('4', 'field4'),
                ('5', ''),
                ('-1', 'field4'),
                ('-2', 'field3,'),
                ('-3', 'field2,'),
                ('-4', 'field1,'),
                ('-5', ''),
                ('2..', 'field2,field3,field4'),
                ('..3', 'field1,field2,field3,'),
                ('2..3', 'field2,field3,'),
                ('3..2', ''),
                ]

        for field, expected in tests:
            sk_command = self.sk("--delimiter ','", f'--with-nth={field}')
            self.tmux.send_keys("echo -e 'field1,field2,field3,field4' |" + sk_command, Key('Enter'))
            self.tmux.until(lambda lines: lines.ready_with_lines(1))
            lines = self.tmux.capture()
            self.tmux.send_keys(Key('Enter'))
            self.assertEqual(f'> {expected}'.strip(), lines[-3])

    def test_nth(self):
        # fields, query, match_count(0/1)
        tests = [
                ('1', 'field1', 1),
                ('1', 'field2', 0),
                ('-1', 'field4', 1),
                ('-1', 'field3', 0),
                ('-5', 'f', 0),
                ('2..', 'field2', 1),
                ('2..', 'field4', 1),
                ('..3', 'field1', 1),
                ('..3', 'field3,', 1),
                ('2..3', '2,3', 1),
                ('3..2', 'f', 0),
                ]

        for field, query, count in tests:
            sk_command = self.sk(f"--delimiter ',' --nth={field} -q {query}")
            self.tmux.send_keys("echo -e 'field1,field2,field3,field4' |" + sk_command, Key('Enter'))
            self.tmux.until(lambda lines: lines.ready_with_lines(1))
            self.tmux.send_keys(Key('Enter'))

    def test_print_query(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('-q 10', '--print-query')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1000))
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\n10')

    def test_print_cmd(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('--cmd-query 10', '--print-cmd')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1000))
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\n1')

    def test_print_cmd_and_query(self):
        self.tmux.send_keys(f"seq 1 1000 | {self.sk('-q 10', '--cmd-query cmd', '--print-cmd', '--print-query')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1000))
        self.tmux.send_keys(Key('Enter'))

        lines = self.readonce().strip()
        self.assertEqual(lines, '10\ncmd\n10')

    def test_hscroll(self):
        # XXXXXXXXXXXXXXXXX..
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"b{'a'*1000}", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].endswith('..'))
        self.tmux.send_keys(Key('Enter'))

        # ..XXXXXXXXXXXXXXXXXM
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].endswith('b'))
        self.tmux.send_keys(Key('Enter'))

        # ..XXXXXXXMXXXXXXX..
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b{'a'*1000}", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> ..'))
        self.tmux.until(lambda lines: lines[-3].endswith('..'))
        self.tmux.send_keys(Key('Enter'))

    def test_no_hscroll(self):
        self.tmux.send_keys(f"cat <<EOF | {self.sk('-q b', '--no-hscroll')}", Key('Enter'))
        self.tmux.send_keys(f"{'a'*1000}b", Key('Enter'))
        self.tmux.send_keys(f"EOF", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))
        self.tmux.send_keys(Key('Enter'))

    def test_tabstop(self):
        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk()}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> a       b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk('--tabstop 1')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> a b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'aa\\tb' | {self.sk('--tabstop 2')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> aa  b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'aa\\tb' | {self.sk('--tabstop 3')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> aa b'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"echo -e 'a\\tb' | {self.sk('--tabstop 4')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> a   b'))
        self.tmux.send_keys(Key('Enter'))

    def test_inline_info(self):
        INLINE_INFO_SEP = " <"
        ## the dot  accounts for spinner
        RE = re.compile(r'[^0-9]*([0-9]+)/([0-9]+)(?: \[([0-9]+)\])?')
        self.tmux.send_keys(f"echo -e 'a1\\na2\\na3\\na4' | {self.sk('--inline-info')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.match_count() == lines.item_count())
        self.tmux.send_keys("a")
        self.tmux.until(lambda lines: lines[-1].find(INLINE_INFO_SEP) != -1)
        lines = self.tmux.capture()
        self.tmux.send_keys(Key('Enter'))
        query_line = lines[-1]
        bef, after = query_line.split(INLINE_INFO_SEP)
        mat = RE.match(after)
        self.assertTrue(mat is not None)
        ret = tuple(map(lambda x: int(x) if x is not None else 0, mat.groups()))
        self.assertEqual(len(ret), 3)
        self.assertEqual((bef, ret[0], ret[1], ret[2]), ("> a ", 4, 4, 0))

        # test that inline info is does not overwrite query
        self.tmux.send_keys(f"echo -e 'a1\\nabcd2\\nabcd3\\nabcd4' | {self.sk('--inline-info')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(4))
        self.tmux.send_keys("bc", Ctrl("a"), "a")
        self.tmux.until(lambda lines: lines[-1].find(INLINE_INFO_SEP) != -1 and
                        lines[-1].split(INLINE_INFO_SEP)[0] == "> abc ")
        self.tmux.send_keys(Key('Enter'))

    def test_header(self):
        self.command_until(sk_options=['--header', 'hello'],
                           until_predicate=lambda lines: lines[-3].find("hello") != -1)

        self.command_until(sk_options=['--inline-info', '--header', 'hello'],
                           until_predicate=lambda lines: lines[-2].find("hello") != -1)

        self.command_until(sk_options=['--reverse', '--inline-info', '--header', 'hello'],
                           until_predicate=lambda lines: lines[1].find("hello") != -1)

        self.command_until(sk_options=['--reverse', '--header', 'hello'],
                           until_predicate=lambda lines: lines[2].find("hello") != -1)

    def test_header_lines(self):
        self.command_until(sk_options=['--header-lines', '1'],
                           until_predicate=lambda lines: lines[-3].find("  a1") != -1)

        self.command_until(sk_options=['--header-lines', '4'],
                           until_predicate=lambda lines: lines[-5].find("  a3") != -1)

        self.command_until(sk_options=['--inline-info', '--header-lines', '1'],
                           until_predicate=lambda lines: lines[-2].find("  a1") != -1)

        self.command_until(sk_options=['--reverse', '--inline-info', '--header-lines', '1'],
                           until_predicate=lambda lines: lines[1].find("  a1") != -1)

        self.command_until(sk_options=['--reverse', '--header-lines', '1'],
                           until_predicate=lambda lines: lines[2].find("  a1") != -1)

    def test_reserved_options(self):
        options = [
            '--extended',
            '--algo=TYPE',
            '--literal',
            '--no-mouse',
            '--cycle',
            '--hscroll-off=COL',
            '--filepath-word',
            '--jump-labels=CHARS',
            '--border',
            '--inline-info',
            '--header=STR',
            '--header-lines=N',
            '--no-bold',
            '--history-size=10',
            '--sync',
            '--no-sort',
            # --select-1
            '--select-1',
            '-1',
            # --exit-0
            '--exit-0',
            '-0']
        for opt in options:
            self.command_until(sk_options=[opt], until_predicate=find_prompt)

    def test_multiple_option_values_should_be_accepted(self):
        # normally we'll put some default options to SKIM_DEFAULT_OPTIONS and override it in command
        # line. this test will ensure multiple values are accepted.

        options = [
            '--bind=ctrl-a:cancel --bind ctrl-b:cancel',
            '--expect=ctrl-a --expect=ctrl-v',
            '--tiebreak=index --tiebreak=score',
            '--cmd asdf --cmd find',
            '--query asdf -q xyz',
            '--delimiter , --delimiter . -d ,',
            '--nth 1,2 --nth=1,3 -n 1,3',
            '--with-nth 1,2 --with-nth=1,3',
            '-I {} -I XX',
            '--color base --color light',
            '--margin 30% --margin 0',
            '--min-height 30% --min-height 10',
            '--height 30% --height 10',
            '--preview "ls {}" --preview "cat {}"',
            '--preview-window up --preview-window down',
            '--multi -m',
            '--no-multi --no-multi',
            '--tac --tac',
            '--ansi --ansi',
            '--exact -e',
            '--regex --regex',
            '--literal --literal',
            '--no-mouse --no-mouse',
            '--cycle --cycle',
            '--no-hscroll --no-hscroll',
            '--filepath-word --filepath-word',
            '--border --border',
            '--inline-info --inline-info',
            '--no-bold --no-bold',
            '--print-query --print-query',
            '--print-cmd --print-cmd',
            '--print0 --print0',
            '--sync --sync',
            '--extended --extended',
            '--no-sort --no-sort',
            '--select-1 --select-1',
            '--exit-0 --exit-0',
        ]
        for opt in options:
            self.command_until(sk_options=[opt], until_predicate=find_prompt)

        options = [
            ('--prompt a --prompt b -p c', lambda lines: lines[-1].startswith("c")),
            ('-i --cmd-prompt a --cmd-prompt b', lambda lines: lines[-1].startswith("b")),
            ('-i --cmd-query asdf --cmd-query xyz', lambda lines: lines[-1].startswith("c> xyz")),
            ('--interactive -i', lambda lines: find_prompt(lines, interactive=True)),
            ('--reverse --reverse', lambda lines: find_prompt(lines, reverse=True))
        ]
        for opt, pred in options:
            self.command_until(sk_options=[opt], until_predicate=pred)

        self.command_until(stdin="echo -e a\\0b", sk_options=['--read0 --read0'], until_predicate=find_prompt)

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

    def test_ansi_and_read0(self):
        """should keep the NULL character, see #142"""
        self.tmux.send_keys(f"echo -e 'a\\0b' | {self.sk('--ansi')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key('Enter'))
        output = ":".join("{:02x}".format(ord(c)) for c in self.readonce())
        self.assertTrue(output.find("61:00:62:0a") >= 0)

    def test_smart_case_fuzzy(self):
        """should behave correctly on case, #219"""

        # smart case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key('abc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key('aBc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key('ABc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))

    def test_smart_case_exact(self):
        """should behave correctly on case, #219"""

        # smart case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key("'abc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key("'aBc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key("'ABc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))

    def test_ignore_case_fuzzy(self):
        """should behave correctly on case, #219"""

        # ignore case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('--case ignore')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key('abc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key('aBc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key('ABc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))

    def test_ignore_case_exact(self):
        """should behave correctly on case, #219"""

        # ignore case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('--case ignore')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key("'abc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key("'aBc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('u'), Key("'ABc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))

    def test_respect_case_fuzzy(self):
        """should behave correctly on case, #219"""

        # respect case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('--case respect')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key('abc'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))

    def test_respect_case_exact(self):
        """should behave correctly on case, #219"""

        # respect case
        self.tmux.send_keys(f"echo -e 'aBcXyZ' | {self.sk('--case respect')}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Key("'abc"))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))

    def test_query_history(self):
        """query history should work"""

        history_file = f'{self.tempname()}.history'

        self.tmux.send_keys(f"echo -e 'a\nb\nc' > {history_file}", Key('Enter'))
        history_mtime = os.stat(history_file).st_mtime

        self.tmux.send_keys(f"echo -e 'a\nb\nc' | {self.sk('--history', history_file)}", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(3))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> c'))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> b'))
        self.tmux.send_keys('b')
        self.tmux.until(lambda lines: lines.ready_with_matches(0))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))

        self.tmux.send_keys(Ctrl('n'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))
        self.tmux.until(lambda lines: lines[-1].startswith('> bb'))
        self.tmux.send_keys(Ctrl('n'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('> c'))

        self.tmux.send_keys('d')
        self.tmux.until(lambda lines: lines[-1].startswith('> cd'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f'[[ "$(echo -n $(cat {history_file}))" == "a b c cd" ]] && echo ok')
        self.tmux.send_keys(Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('ok'))

    def test_cmd_history(self):
        """query history should work"""

        history_file = f'{self.tempname()}.cmd-history'

        self.tmux.send_keys(f"echo -e 'a\nb\nc' > {history_file}", Key('Enter'))
        self.tmux.send_keys(f"""{self.sk("-i -c 'echo {}'", '--cmd-history', history_file)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('c> c'))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('c> b'))
        self.tmux.send_keys('b')
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Ctrl('p'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('c> a'))

        self.tmux.send_keys(Ctrl('n'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('c> bb'))
        self.tmux.send_keys(Ctrl('n'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-1].startswith('c> c'))

        self.tmux.send_keys('d')
        self.tmux.until(lambda lines: lines[-1].startswith('c> cd'))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f'[[ "$(echo -n $(cat {history_file}))" == "a b c cd" ]] && echo ok')
        self.tmux.send_keys(Key('Enter'))
        self.tmux.until(lambda lines: lines[-1].startswith('ok'))

    def test_execute_with_zero_result_ref(self):
        """execute should not panic with zero results #276"""
        self.tmux.send_keys(f"""echo -n "" | {self.sk("--bind 'enter:execute(less {})'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(0))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.send_keys(Key('q'))
        self.tmux.until(lambda lines: lines.ready_with_lines(0))
        self.tmux.until(lambda lines: lines[-1].startswith('> q')) # less is not executed at all
        self.tmux.send_keys(Ctrl('g'))

    def test_execute_with_zero_result_no_ref(self):
        """execute should not panic with zero results #276"""
        self.tmux.send_keys(f"""echo -n "" | {self.sk("--bind 'enter:execute(less)'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(0))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.send_keys(Key('q'))
        self.tmux.until(lambda lines: lines.ready_with_lines(0))
        self.tmux.send_keys(Ctrl('g'))

    def test_if_non_matched(self):
        """commands only effect if no item is matched"""
        self.tmux.send_keys(f"""echo "a\nb" | {self.sk("--bind 'enter:if-non-matched(backward-delete-char)'", "-q ab")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))
        self.tmux.send_keys(Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.send_keys(Key('Enter')) # not triggered anymore
        self.tmux.until(lambda lines: lines.ready_with_matches(1))

    def test_nul_in_execute(self):
        """NUL should work in preview command see #278"""
        self.tmux.send_keys(f"""echo -ne 'a\\0b' | {self.sk("--preview='echo -en {} | xxd'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines.any_include('6100 62'))

    def test_skip_to_pattern(self):
        self.tmux.send_keys(f"""echo -ne 'a/b/c' | {self.sk("--skip-to-pattern '[^/]*$'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(1))
        self.tmux.until(lambda lines: lines.any_include('..c'))

    def test_multi_selection(self):
        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk("-m")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(3))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))
        self.tmux.send_keys(Key('b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> b'))
        self.tmux.send_keys(Key('TAB'))
        self.tmux.until(lambda lines: lines[-3].startswith('>>b'))
        self.tmux.send_keys(Key('C-h'))
        self.tmux.until(lambda lines: lines[-4].startswith(' >b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))
        self.tmux.send_keys(Key('c'))
        self.tmux.until(lambda lines: lines[-3].startswith('> c'))
        self.tmux.send_keys(Key('TAB'))
        self.tmux.until(lambda lines: lines[-3].startswith('>>c'))
        self.tmux.send_keys(Key('C-h'))
        self.tmux.until(lambda lines: lines[-5].startswith(' >c'))
        self.tmux.until(lambda lines: lines[-4].startswith(' >b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))
        self.tmux.send_keys(Key('Enter'))
        self.assertEqual('b\nc', self.readonce().strip())

    def test_append_and_select(self):
        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk("-m --bind 'ctrl-f:append-and-select'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_lines(3))
        self.tmux.send_keys(Key('xyz'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))
        self.tmux.send_keys(Key('C-f'))
        self.tmux.until(lambda lines: lines[-3].startswith('>>xyz'))
        self.tmux.send_keys(Key('C-u'))
        self.tmux.until(lambda lines: lines[-6].startswith(' >xyz'))
        self.tmux.until(lambda lines: lines[-5].startswith('  c'))
        self.tmux.until(lambda lines: lines[-4].startswith('  b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))

    def test_pre_select_n(self):
        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk("-m --pre-select-n=1")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-5].startswith('  c'))
        self.tmux.until(lambda lines: lines[-4].startswith('  b'))
        self.tmux.until(lambda lines: lines[-3].startswith('>>a'))

    def test_pre_select_items(self):
        args = "-m --pre-select-items=$'b\\nc'"
        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-5].startswith(' >c'))
        self.tmux.until(lambda lines: lines[-4].startswith(' >b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))

    def test_pre_select_pat(self):
        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk("-m --pre-select-pat='[b|c]'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-5].startswith(' >c'))
        self.tmux.until(lambda lines: lines[-4].startswith(' >b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))

    def test_pre_select_file(self):
        pre_select_file = f'{self.tempname()}.pre_select'
        self.tmux.send_keys(f"echo -e 'b\nc' > {pre_select_file}", Key('Enter'))
        args = f'''-m --pre-select-file={pre_select_file}'''

        self.tmux.send_keys(f"""echo -n 'a\nb\nc' | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-5].startswith(' >c'))
        self.tmux.until(lambda lines: lines[-4].startswith(' >b'))
        self.tmux.until(lambda lines: lines[-3].startswith('> a'))

    def test_no_clear_if_empty(self):
        text_file = f'{self.tempname()}.txt'
        self.tmux.send_keys(f"echo -e 'b\\nc' > {text_file}", Key('Enter'))

        args = "-c 'cat {}'" + f''' -i --cmd-query='{text_file}' --no-clear-if-empty'''
        self.tmux.send_keys(f"""{self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines[-4].startswith('  c'))
        self.tmux.until(lambda lines: lines[-3].startswith('> b'))

        self.tmux.send_keys(Key('xx'))
        self.tmux.until(lambda lines: lines.ready_with_matches(0))
        self.tmux.until(lambda lines: lines[-4].startswith('  c'))
        self.tmux.until(lambda lines: lines[-3].startswith('> b'))

    def test_preview_scroll_const(self):
        self.tmux.send_keys(f"""echo foo 123 321 | {self.sk("--preview 'seq 1000' --preview-window left:+123")}""", Key('Enter'))
        self.tmux.until(lambda lines: re.match(r'123.*123/1000', lines[0]))

    def test_preview_scroll_expr(self):
        args = "--preview 'seq 1000' --preview-window left:+{3}"
        self.tmux.send_keys(f"""echo foo 123 321 | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: re.match(r'321.*321/1000', lines[0]))

    def test_preview_scroll_and_offset(self):
        args = "--preview 'seq 1000' --preview-window left:+{2}-2"

        self.tmux.send_keys(f"""echo foo 123 321 | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: re.match(r'121.*121/1000', lines[0]))
        self.tmux.send_keys(Key('Enter'))

        self.tmux.send_keys(f"""echo foo :123: 321 | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: re.match(r'121.*121/1000', lines[0]))
        self.tmux.send_keys(Key('Enter'))

    def test_issue_359_multi_byte_and_regex(self):
        self.tmux.send_keys(f"""echo 'ああa' | {self.sk("--regex -q 'a'")}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> ああa'))

    def test_issue_361_literal_space(self):
        args = '''-q "'foo\\ bar"'''
        self.tmux.send_keys(f"""echo 'foo bar\nfoo  bar' | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> foo bar'))
        self.tmux.send_keys(Key('Enter'))

        # prevent bang ("!" sign) expansion
        self.tmux.send_keys(f"""set +o histexpand""", Key('Enter'))

        args = '''-q "'!foo\\ bar"'''
        self.tmux.send_keys(f"""echo 'foo bar\nfoo  bar' | {self.sk(args)}""", Key('Enter'))
        self.tmux.until(lambda lines: lines.ready_with_matches(1))
        self.tmux.until(lambda lines: lines[-3].startswith('> foo  bar'))
        self.tmux.send_keys(Key('Enter'))

        # revert option back
        self.tmux.send_keys(f"""set -o histexpand""", Key('Enter'))


def find_prompt(lines, interactive=False, reverse=False):
    linen = -1
    prompt = ">"
    if interactive:
        prompt = "c>"
    if reverse:
        linen = 0
    return lines[linen].startswith(prompt)


if __name__ == '__main__':
    unittest.main()
