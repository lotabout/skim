[![Build Status](https://travis-ci.org/lotabout/skim.svg?branch=master)](https://travis-ci.org/lotabout/skim)

> Life is short, skim!

Half of our lifes we spent on navigation: files, lines, commands... You need
skim! Skim is a general fuzzy finder that trys to save you life.

It read the data source asynchronously.

![skim demo](https://cloud.githubusercontent.com/assets/1527040/16977618/cc75fd3c-4e89-11e6-9b4a-133f3a3ed616.gif)

skim provides a single executable: `sk`, basically anywhere you want to use
`grep` try `sk` instead.

# Installation

skim project contains several components:

1. `sk` executable -- the core.
2. `sk-tmux` -- script for launching `sk` in a tmux plane.
3. vim/nvim plugin -- to call `sk` inside vim/nvim. check [skim.vim](https://github.com/lotabout/skim.vim) for more vim support.

You can directly [download sk executable](https://github.com/lotabout/skim/releases), but extra utilities are recommanded.

Please put `sk` into you PATH, or add `~/.skim/bin` into your PATH by putting
the following line into your `~/.bashrc`

```
export PATH="$PATH:$HOME/.skim/bin"
```

**Using Git**

Clone this repository and run the install script:

```sh
git clone --depth 1 git@github.com:lotabout/skim.git ~/.skim
~/.skim/install
```

**Install as vim plugin**
Once you have cloned the repository, add the following line to your .vimrc.

```
set rtp+=~/.fzf
```

Or you can have vim-plug manage fzf (recommended):

```
Plug 'lotabout/skim', { 'dir': '~/.skim', 'do': './install' }
```

**Build Manually**

Current requires nightly rust to build. clone the repo and run:

```
cargo build --release
```

and put the resulting `target/release/sk` executable on your PATH.

# Usage

skim can be used as a general filter(like `grep`) or as an interactive
interface for invoking commands.

## As filter

Try the following

```
# directly invoke skim
sk

# or pipe some input to it: (press TAB key select multiple items with -m enabled)
vim $(find . -name "*.rs" | sk -m)
```
The above command will allow you to select files with ".rs" extension and open
the ones you selected in vim.

## As Interactive Interface

[ag](https://github.com/ggreer/the_silver_searcher) or
[ack](https://github.com/petdance/ack2) are both good alternatives for grep
for search contents in a project directory. Now check what you can do with `sk`'s help:

```
sk --ansi -i -c 'ag --color "{}"'
```

![interactive mode demo](https://cloud.githubusercontent.com/assets/1527040/16977634/e03c9484-4e89-11e6-8255-69394964cb90.gif)

## Key bindings

Some common used keybindings.

| key | Action |
|---:|---|
| Enter | Accept (select current one and quit)  |
| ESC/Ctrl-G/Ctrl-Q | Abort|
| Ctrl-P/Up | Move cursor up|
| Ctrl-N/Down | Move cursor Down|
| TAB | Toggle selection and move down (with `-m`)|
| Shift-TAB | Toggle selection and move up (with `-m`)|

Basically keys work in fzf will work in `skim`.

## exit code

| Exit Code | Meaning |
|---|---|
| 0 | Exit normally |
| 1 | No Match found |
| 130 | Abort by Ctrl-C/Ctrl-G/ESC/etc... |

## Customization

skim can be customized with lots of options. You can use them to create new
bash/zsh/.. functions by yourself. Use your imagination :)

## Key binding & Actions

Specify the bindings with comma seperated pairs(no space allowed), example:

`sk --bind 'alt-a:select-all,alt-d:deselect-all'`

| Action | Default key |
|--------|-------------|
| abort | esc, ctrl-c, ctrl-g, ctrl-q |
| accept| enter |
|backward-char| left, ctrl-b|
|backward-delete-char| ctrl-h, backspace |
|backward-kill-word|alt-backspace |
|backward-word| alt-b, shift-left|
|beginning-of-line|ctrl-a |
|cancel| None |
|clear-screen| ctrl-l |
|delete-char|del|
|delete-charEOF|ctrl-d|
|deselect-all|None|
|down|ctrl-j, ctrl-n, down|
|end-of-line|ctrl-e, end|
|forward-char|ctrl-f, right|
|forward-word|alt-f, shift-right|
|ignore|None|
|kill-line|ctrl-k|
|kill-word|alt-d|
|next-history|None|
|page-down|page-down|
|page-up|page-up|
|previous-history|ctrl-p|
|scroll-left|alt-h|
|scroll-right|alt-l|
|select-all|None|
|toggle|None|
|toggle-all|None|
|toggle-down|tab|
|toggle-in|None|
|toggle-out|None|
|toggle-sort|None|
|toggle-up|shift-tab|
|unix-line-discard|ctrl-u|
|unix-word-rubout|ctrl-w|
|up|ctrl-p, ctrl-k, up|

## Sort criterion

There are 4 information about a match: `score, index, begin, end`, you can
specify how the records are sort by `sk --tiebreak score,index,-begin` or any
other you want.

Note that for interactive mode, only `index` is provided.

## Misc

- `--ansi`: to parse ANSI color codes(e.g `\e[32mABC`) of the data source
- `--regex`: use the query as regular expression to match the data source

## Interactive mode

In interactive mode, `sk` will pass the query to the command you specified and
present the output to you. You can specify the command by `-c` option:

`sk -i -c 'ag --color "{}"'`

In the above example, the replstr `{}` will be replaced with the query you
type before invoking the command. Use `-I <replstr>` to change replstr if you
want.

## Fields support

Normally only plugin users need to understand this.

For example, you got the data source as the format:

```
<filename>:<line number>:<column number>
```

However, you want to search `<filename>` only when typing in queries. That
means when you type `21`, you want to find a `<filename>` that contains `21`,
but not matching line number or column number.

You can use `sk --delimiter ':' --nth 0` to achieve this.

Also you can use `--with-nth` to re-arrange the order of fields.

**Range Syntax**

- `<num>` -- to specify the `num`-th fields, starting with 0.
- `start..` -- starting from the `start`-th fields, and the rest.
- `..end` -- starting from the `0`-th field, all the way to `end`-th field,
    excluding `end`.
- `start..end` -- starting from `start`-th field, all the way to `end`-th
    field, excluding `end`.

## Difference to fzf

[fzf](https://github.com/junegunn/fzf) is a command-line fuzzy finder written
in Go and [skim](https://github.com/lotabout/skim) trys to implement a new one
in Rust!

This project is written from scratch. Some decisions of impelmentation are
different from fzf. For example:

1. The fuzzy search algorithm is different.
2. UI of showing matched items. `fzf` will show only the range matched while
   `skim` will show each character matched.
3. `skim` have interactive mode.
4. `skim`'s range syntax is git style.

## How to contribute

[Create new issues](https://github.com/lotabout/skim/issues/new) if you meet any bugs
or have any ideas. Pull request is warmly welcome.
