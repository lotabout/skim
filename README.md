[![Crates.io](https://img.shields.io/crates/v/skim.svg)](https://crates.io/crates/skim)
[![Build Status](https://travis-ci.org/lotabout/skim.svg?branch=master)](https://travis-ci.org/lotabout/skim)

> Life is short, skim!

Half of our life is spent on navigation: files, lines, commands… You need skim!
It is a general fuzzy finder that saves you time.

It is blazingly fast as it reads the data source asynchronously.

![skim demo](https://cloud.githubusercontent.com/assets/1527040/21603846/09138f6e-d1db-11e6-9466-711cc5b1ead8.gif)

skim provides a single executable: `sk`, basically anywhere you would want to use
`grep` try `sk` instead.

# Table of contents

- [Installation](#installation)
- [Usage](#usage)
    - [As Filter](#as-filter)
    - [As Interactive Interface](#as-interactive-interface)
    - [Key Bindings](#key-bindings)
    - [Search Syntax](#search-syntax)
    - [Exit code](#exit-code)
- [Customization](#customization)
    - [Keymap to redefine](#keymap)
    - [Sort Criteria](#sort-criteria)
    - [Color Scheme](#color-scheme)
    - [Misc](#misc)
- [Advance Topics](#advance-topics)
    - [Interactive Mode](#interactive-mode)
    - [Executing external programs](#executing-external-programs)
    - [Preview Window](#preview-window)
    - [Fields Support](#fields-support)
    - [Use as a Library](#use-as-a-library)
- [FAQ](#faq)
    - [How to ignore files?](#how-to-ignore-files)
    - [Some files are not shown in vim plugin](#some-files-are-not-shown-in-vim-plugin)
- [Difference to fzf](#difference-to-fzf)
- [How to contribute](#how-to-contribute)

# Installation

skim project contains several components:

1. `sk` executable -- the core.
2. `sk-tmux` -- script for launching `sk` in a tmux pane.
3. Vim/Nvim plugin -- to call `sk` inside Vim/Nvim. check [skim.vim](https://github.com/lotabout/skim.vim) for more Vim support.

## Linux

Clone this repository and run the install script:

```sh
git clone --depth 1 git@github.com:lotabout/skim.git ~/.skim
~/.skim/install
```

Next: add `~/.skim/bin` to your PATH by putting the following line into your `~/.bashrc`

```
export PATH="$PATH:$HOME/.skim/bin"
```

As an alternative, you can directly [download the sk
executable](https://github.com/lotabout/skim/releases), but extra utilities are recommended.

## OSX

Using Homebrew:

```
brew install sk
```

But the Linux way described above will also work.

## Install from crates.io

```sh
cargo install skim
```

## Install as Vim plugin

Once you have cloned the repository, add the following line to your .vimrc:

```vim
set rtp+=~/.skim
```

Or you can have vim-plug manage skim (recommended):

```vim
Plug 'lotabout/skim', { 'dir': '~/.skim', 'do': './install' }
```

## Build Manually

Clone the repo and run:

```sh
cargo install
```

Alternatively, run:

```sh
cargo build --release
```

then put the resulting `target/release/sk` executable on your PATH.

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
the ones you selected in Vim.

## As Interactive Interface

`skim` can invoke other commands dynamically. Normally you would want to
integrate it with [rg](https://github.com/BurntSushi/ripgrep)
[ag](https://github.com/ggreer/the_silver_searcher) or
[ack](https://github.com/petdance/ack2) for searching contents in a project
directory:

```
# work with ag
sk --ansi -i -c 'ag --color "{}"'
# or with rg
sk --ansi -i -c 'rg --color=always --line-number "{}"'
```

![interactive mode demo](https://cloud.githubusercontent.com/assets/1527040/21603930/655d859a-d1db-11e6-9fec-c25099d30a12.gif)

## Key Bindings

Some commonly used keybindings:

| Key               | Action                                     |
|------------------:|--------------------------------------------|
| Enter             | Accept (select current one and quit)       |
| ESC/Ctrl-G        | Abort                                      |
| Ctrl-P/Up         | Move cursor up                             |
| Ctrl-N/Down       | Move cursor Down                           |
| TAB               | Toggle selection and move down (with `-m`) |
| Shift-TAB         | Toggle selection and move up (with `-m`)   |

## Search Syntax

`skim` borrowed `fzf`'s syntax for matching items:

| Token    | Match type                 | Description                       |
|----------|----------------------------|-----------------------------------|
| `text`   | fuzzy-match                | items that match `text`           |
| `^music` | prefix-exact-match         | items that start with `music`     |
| `.mp3$`  | suffix-exact-match         | items that end with `.mp3`        |
| `'wild`  | exact-match (quoted)       | items that include `wild`         |
| `!fire`  | inverse-exact-match        | items that do not include `fire`  |
| `!.mp3$` | inverse-suffix-exact-match | items that do not end with `.mp3` |

`skim` also supports the combination of tokens.

- space has the meaning of `AND`. With the term `src main`, `skim` will search
    for items that match **both** `src` and `main`.
- ` | ` means `OR` (note the spaces around `|`). With the term `.md$ |
    .markdown$`, `skim` will search for items ends with either `.md` or
    `.markdown`.
- `OR` have higher precedence. So `readme .md$ | .markdown$` is grouped into
    `readme AND (.md$ OR .markdown$)`.

In case that you want to use regular expressions, `skim` provides `regex` mode:

```
sk --regex
```

You can switch to `regex` mode dynamically by pressing `Ctrl-R` (Rotate Mode).

## exit code

| Exit Code | Meaning                           |
|-----------|-----------------------------------|
| 0         | Exit normally                     |
| 1         | No Match found                    |
| 130       | Abort by Ctrl-C/Ctrl-G/ESC/etc... |

# Customization

## Keymap

Specify the bindings with comma seperated pairs(no space allowed), example:

`sk --bind 'alt-a:select-all,alt-d:deselect-all'`

| Action               | Default key                 |
|----------------------|-----------------------------|
| abort                | esc, ctrl-c, ctrl-g         |
| accept               | enter                       |
| backward-char        | left, ctrl-b                |
| backward-delete-char | ctrl-h, backspace           |
| backward-kill-word   | alt-backspace               |
| backward-word        | alt-b, shift-left           |
| beginning-of-line    | ctrl-a                      |
| cancel               | None                        |
| clear-screen         | ctrl-l                      |
| delete-char          | del                         |
| delete-charEOF       | ctrl-d                      |
| deselect-all         | None                        |
| down                 | ctrl-j, ctrl-n, down        |
| end-of-line          | ctrl-e, end                 |
| forward-char         | ctrl-f, right               |
| forward-word         | alt-f, shift-right          |
| ignore               | None                        |
| kill-line            | ctrl-k                      |
| kill-word            | alt-d                       |
| page-down            | page-down                   |
| page-up              | page-up                     |
| rotate-mode          | ctrl-r                      |
| scroll-left          | alt-h                       |
| scroll-right         | alt-l                       |
| select-all           | None                        |
| toggle               | None                        |
| toggle-all           | None                        |
| toggle+down          | tab                         |
| toggle-interactive   | ctrl-q                      |
| toggle-out           | None                        |
| toggle-preview       | None                        |
| toggle-sort          | None                        |
| toggle+up            | shift-tab                   |
| unix-line-discard    | ctrl-u                      |
| unix-word-rubout     | ctrl-w                      |
| up                   | ctrl-p, ctrl-k, up          |

Additionaly, use `+` to concatenate actions, such as `execute-silent(echo {} | pbcopy)+abort`.

## Sort Criteria

There are four sort keys for results: `score, index, begin, end`, you can
specify how the records are sorted by `sk --tiebreak score,index,-begin` or any
other order you want.

## Color Scheme

It is a high chance that you are a better artist than me. Luckily you won't
be stuck with the default colors, `skim` supports customization of the color scheme.

```
--color=[BASE_SCHEME][,COLOR:ANSI]
```

The configuration of colors starts with the name of the base color scheme,
followed by custom color mappings. For example:


```
sk --color=current_bg:24
sk --color=light,fg:232,bg:255,current_bg:116,info:27
```

You can choose the `BASE SCHEME` among the following(default: dark on
256-color terminal, otherwise 16):


| Base Scheme | Description                               |
|-------------|-------------------------------------------|
| dark        | Color scheme for dark 256-color terminal  |
| light       | Color scheme for light 256-color terminal |
| 16          | Color scheme for 16-color terminal        |
| bw          | No colors                                 |

While the customisable `COLOR`s are

| Color            | Description                                      |
|------------------|--------------------------------------------------|
| fg               | Text                                             |
| bg               | Background                                       |
| matched          | Text color of matched items                      |
| matched_bg       | Background color of matched items                |
| current          | Text color (current line)                        |
| current_bg       | Background color (current line)                  |
| current_match    | Text color of matched items (current line)       |
| current_match_bg | Background color of matched items (current line) |
| spinner          | Streaming input indicator                        |
| info             | Info area                                        |
| prompt           | Prompt                                           |
| cursor           | Cursor                                           |
| selected         | Text color of "selected" indicator               |
| border           | Border color of preview window                   |


## Misc

- `--ansi`: to parse ANSI color codes(e.g `\e[32mABC`) of the data source
- `--regex`: use the query as regular expression to match the data source

# Advanced Topics

## Interactive mode

With "interactive mode", you could invoke command dynamically. Try out:

```
sk --ansi -i -c 'rg --color=always --line-number "{}"'
```

How it works?

![skim's interactive mode](https://user-images.githubusercontent.com/1527040/53381293-461ce380-39ab-11e9-8e86-7c3bbfd557bc.png)

- Skim could accept two kinds of source: command output or piped input
- Skim have two kinds of prompt: query prompt to specify the query pattern,
    command prompt to specify the "arguments" of the command
- `-c` is used to specify the command to execute while defaults to `SKIM_DEFAULT_COMMAND`
- `-i` is to tell skim open command prompt on startup, which will show `c>` by default.

If you want to further narrow down the result returned by the command, press
`Ctrl-Q` to toggle interactive mode.

## Executing external programs

You can set up key bindings for starting external processes without leaving skim (`execute`, `execute-silent`).

```
# Press F1 to open the file with less without leaving skim
# Press CTRL-Y to copy the line to clipboard and aborts skim (requires pbcopy)
sk --bind 'f1:execute(less -f {}),ctrl-y:execute-silent(echo {} | pbcopy)+abort'
```

## Preview Window

This is a great feature of fzf that skim borrows. For example, we use 'ag' to
find the matched lines, once we narrow down to the target lines, we want to
finally decide which lines to pick by checking the context around the line.
`grep` and `ag` has an option `--context`, skim can do better with preview
window. For example:

```
sk --ansi -i -c 'ag --color "{}"' --preview "preview.sh {}"
```

(Note the [preview.sh](https://github.com/junegunn/fzf.vim/blob/master/bin/preview.sh) is a script to print the context given filename:lines:columns)
You got things like this:

![preview demo](https://user-images.githubusercontent.com/1527040/30677573-0cee622e-9ebf-11e7-8316-c741324ecb3a.png)

### How does it work?

If the preview command is given by the `--preview` option, skim will replace the
`{}` with the current highlighted line surrounded by single quotes, call the
command to get the output, and print the output on the preview window.

Sometimes you don't need the whole line for invoking the command. In this case
you can use `{}`, `{1..}`, `{..3}` or `{1..5}` to select the fields. The
syntax is explained in the section "Fields Support".

Last, you might want to configure the position of preview windows, use
`--preview-window`.
- `--preview-window up:30%` to put the window in the up position with height
    30% of the total height of skim.
- `--preview-window left:10:wrap`, to specify the `wrap` allows the preview
    window to wrap the output of the preview command.
- `--preview-window wrap:hidden` to hide the preview window at startup, later
    it can be shown by the action `toggle-preview`.

## Fields support

Normally only plugin users need to understand this.

For example, you have the data source with the format:

```
<filename>:<line number>:<column number>
```

However, you want to search `<filename>` only when typing in queries. That
means when you type `21`, you want to find a `<filename>` that contains `21`,
but not matching line number or column number.

You can use `sk --delimiter ':' --nth 1` to achieve this.

Also you can use `--with-nth` to re-arrange the order of fields.

**Range Syntax**

- `<num>` -- to specify the `num`-th fields, starting with 1.
- `start..` -- starting from the `start`-th fields, and the rest.
- `..end` -- starting from the `0`-th field, all the way to `end`-th field,
    including `end`.
- `start..end` -- starting from `start`-th field, all the way to `end`-th
    field, including `end`.

## Use as a library

Skim can now be used as a library in your Rust crates. The basic idea is to
throw anything that is `BufRead`(we can easily turn a `File` for `String` into
`BufRead`) and skim will do its job and bring us back the user selection
including the selected items(with their indices), the query, etc.

First, add skim into your `Cargo.toml`:

```toml
[dependencies]
skim = "0.6.6"
```

Then try to run this simple example:

```rust
extern crate skim;
use skim::{Skim, SkimOptionsBuilder};
use std::io::Cursor;

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .build()
        .unwrap();

    let input = "aaaaa\nbbbb\nccc".to_string();

    let selected_items = Skim::run_with(&options, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.get_index(), item.get_output_text(), "\n");
    }
}
```

Check more examples under [examples/](https://github.com/lotabout/skim/tree/master/examples) directory.

# FAQ

## How to ignore files?

Skim invokes `find .` to fetch a list of files for filtering. You can override
that by setting the environment variable `SKIM_DEFAULT_COMMAND`. For example:

```sh
$ SKIM_DEFAULT_COMMAND="fd --type f || git ls-tree -r --name-only HEAD || rg --files || find ."
$ sk
```

You could put it in your `.bashrc` or `.zshrc` if you like it to be default.

## Some files are not shown in Vim plugin

If you use the Vim plugin and execute the `:SK` command, you might find some
of your files not shown.

As described in [#3](https://github.com/lotabout/skim/issues/3), in the Vim
plugin, `SKIM_DEFAULT_COMMAND` is set to the command by default:

```
let $SKIM_DEFAULT_COMMAND = "git ls-tree -r --name-only HEAD || rg --files || ag -l -g \"\" || find ."
```

That means the files not recognized by git will not shown. Either override the
default with `let $SKIM_DEFAULT_COMMAND = ''` or find the missing file by
yourself.

# Difference to fzf

[fzf](https://github.com/junegunn/fzf) is a command-line fuzzy finder written
in Go and [skim](https://github.com/lotabout/skim) tries to implement a new one
in Rust!

This project is written from scratch. Some decisions of implementation are
different from fzf. For example:

1. The fuzzy search algorithm is different.
2. ~~UI of showing matched items. `fzf` will show only the range matched while
   `skim` will show each character matched.~~ (fzf has this now)
3. `skim` has an interactive mode.
4. ~~`skim`'s range syntax is git style~~: now it is the same with fzf.

# How to contribute

[Create new issues](https://github.com/lotabout/skim/issues/new) if you meet any bugs
or have any ideas. Pull requests are warmly welcomed.
